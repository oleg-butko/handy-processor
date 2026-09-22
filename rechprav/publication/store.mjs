// Offline publication transport; never opens the application's database.
import { DatabaseSync } from "node:sqlite";
import { createHash } from "node:crypto";
import { closeSync, openSync, readSync, readFileSync, statSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const APPLICATION_ID = 0x52505031; // RPP1
const LIMIT = 8 * 1024 * 1024;
const VALIDATOR = fileURLToPath(new URL("../target/release/examples/profile_validate", import.meta.url));
export const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}
function check(ok, message) { if (!ok) throw new Error(message); }
function fields(value, keys) {
  check(value !== null && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).length === keys.length && keys.every(k => Object.hasOwn(value, k)), "invalid fields");
}
function id(value) { check(typeof value === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(value), "invalid id"); }
export function readText(path) {
  check(statSync(path).size <= LIMIT, "file exceeds 8 MiB");
  const bytes = readFileSync(path);
  check(bytes.length <= LIMIT, "file exceeds 8 MiB");
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}
export function pack(directory, manualPath) {
  const manifest = readText(join(directory, "composition-manifest.json"));
  const m = JSON.parse(manifest);
  const bundle = { manual_path: resolve(manualPath), manual: readText(manualPath),
    events: readText(m.input_file), decisions: readText(m.decisions_file),
    bindings: readText(m.bindings_file), report: readText(join(directory, "report.json")),
    config: readText(join(directory, "composed.toml")), manifest };
  bundleBytes(bundle);
  return bundle;
}
function bundleBytes(bundle) {
  fields(bundle, ["manual_path", "manual", "events", "decisions", "bindings", "report", "config", "manifest"]);
  check(Object.values(bundle).every(v => typeof v === "string" && v.isWellFormed()), "invalid bundle text");
  const bytes = canonical(bundle);
  check(Buffer.byteLength(bytes) <= LIMIT, "bundle exceeds 8 MiB");
  return bytes;
}

export class PublicationStore {
  #db;
  #fault;
  static create(path, scope) {
    id(scope);
    closeSync(openSync(path, "wx", 0o600));
    const db = new DatabaseSync(path);
    try {
      db.exec(`PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; BEGIN IMMEDIATE;
        CREATE TABLE state (singleton INTEGER PRIMARY KEY CHECK(singleton=1), scope TEXT NOT NULL,
          desired INTEGER NOT NULL, active INTEGER) STRICT;
        CREATE TABLE requests (revision INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE,
          payload TEXT NOT NULL, bundle TEXT NOT NULL, sha256 TEXT NOT NULL,
          validation TEXT, receipt TEXT) STRICT;
        PRAGMA application_id=${APPLICATION_ID}; PRAGMA user_version=1;`);
      db.prepare("INSERT INTO state VALUES (1, ?, 0, NULL)").run(scope);
      db.exec("COMMIT");
    } finally { db.close(); }
    return new PublicationStore(path);
  }
  constructor(path, { testFault = () => {} } = {}) {
    const fd = openSync(path, "r");
    try {
      const header = Buffer.alloc(100);
      check(readSync(fd, header, 0, 100, 0) === 100
        && header.subarray(0, 16).toString("ascii") === "SQLite format 3\0"
        && header.readUInt32BE(68) === APPLICATION_ID && header.readUInt32BE(60) === 1,
      "not a supported publication database");
    } finally { closeSync(fd); }
    this.#db = new DatabaseSync(path, { timeout: 5_000 });
    this.#fault = testFault;
    try {
      check(this.#db.prepare("PRAGMA application_id").get().application_id === APPLICATION_ID
        && this.#db.prepare("PRAGMA user_version").get().user_version === 1, "unsupported publication schema");
      this.#db.exec("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;");
    } catch (error) { this.#db.close(); throw error; }
  }
  close() { this.#db.close(); }
  request(request) {
    fields(request, ["schema_version", "request_id", "expected_revision", "bundle"]);
    id(request.request_id);
    check(request.schema_version === 1 && Number.isSafeInteger(request.expected_revision)
      && request.expected_revision >= 0 && request.expected_revision < 128, "invalid schema or revision");
    const bundle = bundleBytes(request.bundle);
    const payload = canonical(request);
    this.#db.exec("BEGIN IMMEDIATE");
    try {
      const old = this.#db.prepare("SELECT * FROM requests WHERE id=?").get(request.request_id);
      if (old) {
        check(old.payload === payload, "conflicting request id");
        this.#db.exec("COMMIT");
        return { request_id: old.id, generation: old.revision, bundle_sha256: old.sha256, replayed: true };
      }
      const state = this.#db.prepare("SELECT * FROM state").get();
      check(state.desired === request.expected_revision, "stale expected_revision");
      check(state.desired < 128, "publication request limit reached");
      check(JSON.parse(request.bundle.manifest).scope === state.scope, "scope mismatch");
      const revision = state.desired + 1;
      const sha = digest(bundle);
      this.#db.prepare("INSERT INTO requests (revision,id,payload,bundle,sha256) VALUES (?,?,?,?,?)")
        .run(revision, request.request_id, payload, bundle, sha);
      this.#fault("request-after-insert");
      this.#db.prepare("UPDATE state SET desired=? WHERE singleton=1").run(revision);
      this.#fault("request-before-commit");
      this.#db.exec("COMMIT");
      this.#fault("request-after-commit");
      return { request_id: request.request_id, generation: revision, bundle_sha256: sha, replayed: false };
    } catch (error) { if (this.#db.isTransaction) this.#db.exec("ROLLBACK"); throw error; }
  }
  publish(requestId) {
    id(requestId);
    const candidate = this.#db.prepare("SELECT * FROM requests WHERE id=?").get(requestId);
    check(candidate, "unknown request");
    if (candidate.receipt !== null) return { ...JSON.parse(candidate.receipt), replayed: true };
    check(this.#db.prepare("SELECT desired FROM state").get().desired === candidate.revision, "superseded request");
    check(digest(candidate.bundle) === candidate.sha256, "stored bundle hash mismatch");
    const result = spawnSync(VALIDATOR, [], { input: candidate.bundle, encoding: "utf8",
      maxBuffer: LIMIT, timeout: 60_000 });
    check(!result.error && result.status === 0, `validation failed: ${result.error?.message ?? result.stderr}`);
    const validation = JSON.parse(result.stdout);
    const bundle = JSON.parse(candidate.bundle);
    check(validation.schema_version === 1 && validation.bundle_sha256 === candidate.sha256
      && validation.config_sha256 === digest(bundle.config), "validator response mismatch");
    this.#fault("publish-after-validation");
    this.#db.exec("BEGIN IMMEDIATE");
    try {
      const state = this.#db.prepare("SELECT * FROM state").get();
      const current = this.#db.prepare("SELECT * FROM requests WHERE id=?").get(requestId);
      if (current.receipt !== null) {
        this.#db.exec("COMMIT");
        return { ...JSON.parse(current.receipt), replayed: true };
      }
      check(state.desired === candidate.revision, "superseded request after validation");
      check(validation.scope === state.scope, "validated scope mismatch");
      const receipt = { request_id: requestId, generation: candidate.revision,
        bundle_sha256: candidate.sha256, config_sha256: validation.config_sha256 };
      this.#db.prepare("UPDATE requests SET validation=?, receipt=? WHERE id=?")
        .run(canonical(validation), canonical(receipt), requestId);
      this.#fault("publish-after-insert");
      this.#db.prepare("UPDATE state SET active=? WHERE singleton=1").run(candidate.revision);
      this.#fault("publish-before-commit");
      this.#db.exec("COMMIT");
      this.#fault("publish-after-commit");
      return { ...receipt, replayed: false };
    } catch (error) { if (this.#db.isTransaction) this.#db.exec("ROLLBACK"); throw error; }
  }
  snapshot() {
    this.#db.exec("BEGIN");
    try {
      const state = this.#db.prepare("SELECT * FROM state").get();
      const row = state.active === null ? null
        : this.#db.prepare("SELECT * FROM requests WHERE revision=?").get(state.active);
      let active = null;
      if (state.active !== null) {
        check(row && row.validation !== null && row.receipt !== null, "incomplete active snapshot");
        const bundle = JSON.parse(row.bundle), validation = JSON.parse(row.validation), receipt = JSON.parse(row.receipt);
        check(digest(row.bundle) === row.sha256 && validation.bundle_sha256 === row.sha256
          && receipt.bundle_sha256 === row.sha256 && receipt.generation === row.revision
          && receipt.request_id === row.id && receipt.config_sha256 === digest(bundle.config)
          && validation.config_sha256 === receipt.config_sha256 && validation.scope === state.scope,
        "corrupt active snapshot");
        active = { generation: row.revision, request_id: row.id, bundle_sha256: row.sha256, bundle, validation, receipt };
      }
      this.#db.exec("COMMIT");
      return { scope: state.scope, desired_revision: state.desired, active };
    } catch (error) { if (this.#db.isTransaction) this.#db.exec("ROLLBACK"); throw error; }
  }
}

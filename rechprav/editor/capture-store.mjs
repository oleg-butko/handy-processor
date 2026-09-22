// Offline fragment capture prototype. No imports or writes into the running application.
import { DatabaseSync } from "node:sqlite";
import { createHash } from "node:crypto";
import { closeSync, existsSync, fsyncSync, mkdirSync, openSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const APPLICATION_ID = 0x52504331; // RPC1, separate from the application's SQLite schema.
const MAX_REQUESTS = 10_000;
const digest = (text) => createHash("sha256").update(text).digest("hex");
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value).sort().map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}
function check(condition, message) {
  if (!condition) throw new Error(message);
}
function fields(object, required, optional = []) {
  check(object !== null && typeof object === "object" && !Array.isArray(object), "expected object");
  check(required.every((key) => Object.hasOwn(object, key)), "missing field");
  check(Object.keys(object).every((key) => required.includes(key) || optional.includes(key)), "unknown field");
}
function id(value) {
  check(typeof value === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(value), "invalid id");
}
function text(value) {
  check(typeof value === "string" && value.isWellFormed(), "invalid Unicode text");
  check([...value].length <= 128 && Buffer.byteLength(value, "utf8") <= 4096, "fragment exceeds text budget");
}
function boundary(value, offset) {
  return Number.isSafeInteger(offset) && offset >= 0 && offset <= value.length
    && value.slice(0, offset).isWellFormed() && value.slice(offset).isWellFormed();
}

export function validateRequest(request) {
  const common = ["schema_version", "request_id", "fragment_id", "session_id", "scope",
    "expected_revision", "kind", "before", "after"];
  check(request?.kind === "replace" || request?.kind === "undo", "unknown request kind");
  fields(request, [...common, request.kind === "replace" ? "confirmed_asr" : "target_request_id"],
    request.kind === "replace" ? ["selection"] : []);
  check(request.schema_version === 1, "unknown schema");
  for (const key of ["request_id", "fragment_id", "session_id", "scope"]) id(request[key]);
  check(Number.isSafeInteger(request.expected_revision) && request.expected_revision >= 0
    && request.expected_revision < MAX_REQUESTS, "invalid revision");
  text(request.before); text(request.after);
  check(request.before !== request.after, "no text change");
  if (request.kind === "undo") { id(request.target_request_id); return; }
  check(typeof request.confirmed_asr === "boolean", "confirmed_asr must be explicit boolean");
  if (Object.hasOwn(request, "selection")) {
    const s = request.selection;
    fields(s, ["start_utf16", "end_utf16", "replacement"]);
    text(s.replacement);
    check(boundary(request.before, s.start_utf16) && boundary(request.before, s.end_utf16)
      && s.start_utf16 < s.end_utf16, "invalid UTF-16 selection");
    check(request.before.slice(0, s.start_utf16) + s.replacement + request.before.slice(s.end_utf16)
      === request.after, "selection does not reconstruct after");
  }
}

function toEvent(request) {
  if (request.kind === "undo") {
    return { schema_version: 1, event_id: request.request_id, kind: "retract", target_event_id: request.target_request_id };
  }
  const event = { schema_version: 1, event_id: request.request_id, kind: "correction", scope: request.scope,
    before: request.before, after: request.after, confirmed_asr: request.confirmed_asr };
  if (request.selection !== undefined) {
    const s = request.selection;
    event.selection = { byte_start: Buffer.byteLength(request.before.slice(0, s.start_utf16)),
      byte_end: Buffer.byteLength(request.before.slice(0, s.end_utf16)), replacement: s.replacement };
  }
  return event;
}

export class CaptureStore {
  #db;
  #testFault;
  static create(path) {
    const file = openSync(path, "wx", 0o600);
    closeSync(file);
    const db = new DatabaseSync(path);
    try {
      db.exec(`PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL;
        BEGIN IMMEDIATE;
        CREATE TABLE fragments (
          fragment_id TEXT PRIMARY KEY, scope TEXT NOT NULL, text TEXT NOT NULL,
          revision INTEGER NOT NULL CHECK (revision >= 1)
        ) STRICT;
        CREATE TABLE requests (
          sequence INTEGER PRIMARY KEY, request_id TEXT NOT NULL UNIQUE,
          fragment_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('replace','undo')),
          payload TEXT NOT NULL, event TEXT NOT NULL, receipt TEXT NOT NULL
        ) STRICT;
        CREATE TABLE retractions (
          target_id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE
        ) STRICT;
        PRAGMA application_id = ${APPLICATION_ID}; PRAGMA user_version = 1;
        COMMIT;`);
    } finally { db.close(); }
    return new CaptureStore(path);
  }
  // Test-only fault callback permits real process termination at the transaction boundary.
  constructor(path, { testFault = () => {} } = {}) {
    check(existsSync(path), "capture database does not exist; create explicitly");
    this.#db = new DatabaseSync(path, { timeout: 5_000 });
    this.#testFault = testFault;
    try {
      check(this.#db.prepare("PRAGMA application_id").get().application_id === APPLICATION_ID
        && this.#db.prepare("PRAGMA user_version").get().user_version === 1, "not a supported capture database");
      this.#db.exec("PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL;");
    } catch (error) { this.#db.close(); throw error; }
  }
  close() { this.#db.close(); }
  append(request) {
    validateRequest(request);
    const payload = canonical(request);
    let receipt;
    this.#db.exec("BEGIN IMMEDIATE");
    try {
      const prior = this.#db.prepare("SELECT payload, receipt FROM requests WHERE request_id = ?").get(request.request_id);
      if (prior !== undefined) {
        check(prior.payload === payload, "conflicting request id");
        this.#db.exec("COMMIT");
        return { ...JSON.parse(prior.receipt), replayed: true };
      }
      const count = this.#db.prepare("SELECT COUNT(*) AS count FROM requests").get().count;
      check(count < MAX_REQUESTS, "journal request limit reached");
      const frame = this.#db.prepare("SELECT * FROM fragments WHERE fragment_id = ?").get(request.fragment_id);
      if (frame === undefined) {
        check(request.kind === "replace" && request.expected_revision === 0, "unknown fragment or noninitial revision");
      } else {
        check(frame.scope === request.scope, "fragment scope mismatch");
        check(frame.revision === request.expected_revision, "stale or future revision");
        check(frame.text === request.before, "before does not match current fragment");
      }
      if (request.kind === "undo") {
        const target = this.#db.prepare(`SELECT r.* FROM requests r
          LEFT JOIN retractions u ON u.target_id = r.request_id
          WHERE r.fragment_id = ? AND r.kind = 'replace' AND u.target_id IS NULL
          ORDER BY r.sequence DESC LIMIT 1`).get(request.fragment_id);
        check(target !== undefined && target.request_id === request.target_request_id, "undo must target last active replace in this fragment");
        const original = JSON.parse(target.payload);
        check(request.before === original.after && request.after === original.before, "undo is not the exact inverse");
        this.#db.prepare("INSERT INTO retractions (target_id,request_id) VALUES (?,?)").run(request.target_request_id, request.request_id);
      }
      const event = toEvent(request);
      receipt = { request_id: request.request_id, sequence: count + 1, fragment_id: request.fragment_id,
        revision: request.expected_revision + 1, event_id: event.event_id, replayed: false };
      this.#db.prepare("INSERT INTO requests (sequence,request_id,fragment_id,kind,payload,event,receipt) VALUES (?,?,?,?,?,?,?)")
        .run(count + 1, request.request_id, request.fragment_id, request.kind, payload, canonical(event), canonical(receipt));
      this.#testFault("after-insert");
      this.#db.prepare(`INSERT INTO fragments (fragment_id,scope,text,revision) VALUES (?,?,?,?)
        ON CONFLICT(fragment_id) DO UPDATE SET text=excluded.text, revision=excluded.revision`)
        .run(request.fragment_id, request.scope, request.after, receipt.revision);
      this.#testFault("before-commit");
      this.#db.exec("COMMIT");
    } catch (error) {
      this.#db.exec("ROLLBACK");
      throw error;
    }
    this.#testFault("after-commit");
    return receipt;
  }
  snapshot() {
    this.#db.exec("BEGIN");
    try {
      const rows = this.#db.prepare("SELECT payload,event,receipt FROM requests ORDER BY sequence").all();
      const result = { requests: rows.map((r) => JSON.parse(r.payload)), events: rows.map((r) => JSON.parse(r.event)),
        receipts: rows.map((r) => JSON.parse(r.receipt)),
        fragments: this.#db.prepare("SELECT * FROM fragments ORDER BY fragment_id").all() };
      this.#db.exec("COMMIT");
      return result;
    } catch (error) { this.#db.exec("ROLLBACK"); throw error; }
  }
  exportSnapshot(directory) {
    const snapshot = this.snapshot();
    const jsonl = (rows) => rows.map((row) => canonical(row) + "\n").join("");
    const files = { "events.jsonl": jsonl(snapshot.events), "requests.jsonl": jsonl(snapshot.requests),
      "snapshot.json": JSON.stringify(snapshot, null, 2) + "\n" };
    const manifest = { schema_version: 1, event_count: snapshot.events.length,
      output_sha256: Object.fromEntries(Object.entries(files).map(([name, bytes]) => [name, digest(bytes)])) };
    files["capture-manifest.json"] = JSON.stringify(manifest, null, 2) + "\n";
    mkdirSync(directory, { mode: 0o700 });
    for (const [name, data] of Object.entries(files)) {
      const fd = openSync(join(directory, name), "wx", 0o600);
      try { writeFileSync(fd, data); fsyncSync(fd); } finally { closeSync(fd); }
    }
    return manifest;
  }
}

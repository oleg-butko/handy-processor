import test, { after } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { PublicationStore, pack, digest, readText } from "./store.mjs";
import { currentFixtures } from "./testing/fixtures.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const { directory, composed, disabled } = currentFixtures(root);
after(() => rmSync(directory, { recursive: true, force: true }));
const worker = fileURLToPath(new URL("testing/worker.mjs", import.meta.url));
const req = (id = "a", revision = 0, bundle = composed) => ({ schema_version: 1, request_id: id, expected_revision: revision, bundle });
// Preserve the producer's numeric JSON representation (1.0 vs 1) so a mutation
// reaches its intended guard instead of changing unrelated resource metadata.
function patchManifest(original, updated) {
  let result = original;
  function visit(before, after) {
    for (const key of Object.keys(before)) {
      if (before[key] !== null && typeof before[key] === "object") visit(before[key], after[key]);
      else if (before[key] !== after[key]) result = result.replace(JSON.stringify(before[key]), JSON.stringify(after[key]));
    }
  }
  visit(JSON.parse(original), updated);
  return result;
}
function setup(t) {
  const dir = mkdtempSync(join(tmpdir(), "rechprav-publish-"));
  const path = join(dir, "profiles.sqlite");
  const store = PublicationStore.create(path, "technical");
  t.after(() => { store.close(); rmSync(dir, { recursive: true, force: true }); });
  return { dir, path, store };
}
function initial(store) { store.request(req()); store.publish("a"); }
function integrity(path) {
  const db = new DatabaseSync(path, { readOnly: true });
  try { assert.equal(db.prepare("PRAGMA integrity_check").get().integrity_check, "ok"); }
  finally { db.close(); }
}
function child(t, args) {
  const process = spawn(globalThis.process.execPath, [worker, ...args], { stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "", stderr = "";
  process.stdout.on("data", data => stdout += data);
  process.stderr.on("data", data => stderr += data);
  const done = new Promise((resolve, reject) => {
    process.on("error", reject);
    process.on("close", (code, signal) => resolve({ code, signal, stdout, stderr }));
  });
  t.after(() => { if (process.exitCode === null && process.signalCode === null) process.kill("SIGKILL"); });
  return { process, done };
}
async function ready(path) {
  const deadline = Date.now() + 20_000;
  while (!existsSync(path)) {
    assert.ok(Date.now() < deadline, `worker never reached ${path}`);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
}

test("publication preserves exact composed bytes; receipts never reactivate old generations", t => {
  const { store, path } = setup(t);
  assert.equal(store.snapshot().active, null);
  assert.equal(store.request(req()).generation, 1);
  assert.equal(store.request(req()).replayed, true);
  assert.throws(() => store.request(req("a", 0, disabled)), /conflicting request id/);
  const receipt = store.publish("a");
  assert.equal(receipt.replayed, false);
  assert.deepEqual(store.snapshot().active.bundle, composed);
  assert.equal(store.snapshot().active.validation.config_sha256, digest(composed.config));
  store.request(req("b", 1, disabled)); store.publish("b");
  assert.deepEqual(store.snapshot().active.bundle, disabled);
  assert.equal(store.publish("a").replayed, true);
  assert.equal(store.request(req()).replayed, true);
  assert.equal(store.snapshot().active.generation, 2);
  assert.throws(() => store.request(req("late", 0)), /stale expected_revision/);
  assert.throws(() => store.publish("unknown"), /unknown request/);
  integrity(path);
});

test("new intent supersedes an older build, including intent arriving during validation", t => {
  const { path, store } = setup(t);
  initial(store);
  store.request(req("old", 1)); store.request(req("new", 2, disabled));
  assert.throws(() => store.publish("old"), /superseded request/);
  assert.equal(store.snapshot().active.generation, 1);
  const racing = new PublicationStore(path, { testFault(point) {
    if (point === "publish-after-validation") store.request(req("newer", 3));
  } });
  try { assert.throws(() => racing.publish("new"), /superseded request after validation/); }
  finally { racing.close(); }
  assert.equal(store.snapshot().desired_revision, 4);
  assert.equal(store.snapshot().active.generation, 1);
  store.publish("newer");
  assert.equal(store.snapshot().active.generation, 4);
});

test("hash tampering, forged outputs and provenance mismatches keep the previous snapshot", t => {
  const { store } = setup(t); initial(store);
  const mutants = [];
  mutants.push({ name: "hash", change(b) { b.config += "\n"; }, error: /config hash mismatch/ });
  mutants.push({ name: "forged", change(b, m) {
    b.config = b.config.replace('canonical = "слот"', 'canonical = "свод"');
    const report = JSON.parse(b.report); report.config.sets[0].terms[0].canonical = "свод";
    b.report = JSON.stringify(report);
    m.output_sha256["composed.toml"] = digest(b.config); m.output_sha256["report.json"] = digest(b.report);
  }, error: /report differs from replay/ });
  mutants.push({ name: "forged_config", change(b, m) {
    b.config = b.config.replace('canonical = "слот"', 'canonical = "свод"');
    m.output_sha256["composed.toml"] = digest(b.config);
  }, error: /config differs from replay/ });
  mutants.push({ name: "invalid_toml", change(b, m) {
    b.config = "sets = ["; m.output_sha256["composed.toml"] = digest(b.config);
  }, error: /TOML parse error/ });
  mutants.push({ name: "rejected", change(b, m) {
    b.decisions = b.decisions.replace('"accept"', '"reject"');
    m.decisions_sha256 = digest(b.decisions);
  }, error: /report differs from replay/ });
  mutants.push({ name: "source", change(b, m) { m.compiled_source_sha256["src/terms.rs"] = "0".repeat(64); }, error: /compiled sources mismatch/ });
  mutants.push({ name: "resource", change(b, m) { m.resources.resource_sha256.dictionary = "0".repeat(64); }, error: /resources mismatch/ });
  mutants.push({ name: "events", change(b) { b.events += "{}\n"; }, error: /input_sha256 hash mismatch/ });
  mutants.push({ name: "decision", change(b) { b.decisions = ""; }, error: /decisions_sha256 hash mismatch/ });
  mutants.push({ name: "manual", change(b) { b.manual += "\n"; }, error: /reference_config_sha256 hash mismatch/ });
  mutants.push({ name: "bindings", change(b) { b.bindings += "\n"; }, error: /bindings_sha256 hash mismatch/ });
  mutants.push({ name: "environment", change(b, m) { m.environment_sha256 = "0".repeat(64); }, error: /environment mismatch/ });
  let revision = 1;
  for (const mutant of mutants) {
    const b = structuredClone(composed), m = JSON.parse(b.manifest);
    mutant.change(b, m); b.manifest = patchManifest(b.manifest, m);
    store.request(req(mutant.name, revision++, b));
    assert.throws(() => store.publish(mutant.name), mutant.error);
    assert.equal(store.snapshot().active.generation, 1);
    assert.deepEqual(store.snapshot().active.bundle, composed);
  }
});

test("request captures bytes independently of later edits to the caller's bundle", t => {
  const { store } = setup(t);
  const request = req("frozen", 0, structuredClone(composed));
  store.request(request);
  request.bundle.config = "broken"; request.bundle.events = "";
  store.publish("frozen");
  assert.deepEqual(store.snapshot().active.bundle, composed);
});

test("input limits, explicit schema, scope, missing files and fatal UTF-8 decoding", t => {
  const { dir, store } = setup(t);
  for (const input of [{ ...req(), extra: true }, { ...req(), schema_version: 2 }, req("bad id"), req("a", -1),
    { ...req(), bundle: { ...composed, config: "\ud800" } },
    { ...req(), bundle: { ...composed, config: "x".repeat(8 * 1024 * 1024) } }]) {
    assert.throws(() => store.request(input));
  }
  const b = structuredClone(composed), m = JSON.parse(b.manifest); m.scope = "other"; b.manifest = JSON.stringify(m);
  assert.throws(() => store.request(req("scope", 0, b)), /scope mismatch/);
  assert.throws(() => pack(join(dir, "missing"), "missing"), /ENOENT/);
  const invalid = join(dir, "bad.json"); writeFileSync(invalid, Buffer.from([0xff]));
  assert.throws(() => readText(invalid), /encoded data/);
  assert.equal(store.snapshot().desired_revision, 0);
});

test("changed external dictionary is detected by replay without touching original resources", t => {
  const { dir, store } = setup(t); initial(store);
  const dictionary = join(dir, "dictionary.txt"), baseline = join(dir, "baseline.toml"), manual = join(dir, "manual.toml");
  writeFileSync(dictionary, "слот\nсвод\n");
  writeFileSync(baseline, 'dictionary = "dictionary.txt"\n');
  writeFileSync(manual, `baseline_config = ${JSON.stringify(baseline)}\nsets = []\n`);
  writeFileSync(join(dir, "events.jsonl"), ""); writeFileSync(join(dir, "decisions.jsonl"), "");
  writeFileSync(join(dir, "bindings.json"), '{"enabled":true,"bindings":[]}');
  const output = join(dir, "composed");
  const result = spawnSync(join(root, "target/release/examples/feedback_compose"), [join(dir, "events.jsonl"),
    "--bindings", join(dir, "bindings.json"), "--decisions", join(dir, "decisions.jsonl"),
    "--config", manual, "--scope", "technical", "--output-dir", output], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  const bundle = pack(output, manual);
  store.request(req("changed", 1, bundle));
  writeFileSync(dictionary, "слот\nсвод\nввод\n");
  assert.throws(() => store.publish("changed"), /resources mismatch/);
  assert.equal(store.snapshot().active.generation, 1);
});

for (const operation of ["request", "publish"]) {
  for (const point of ["after-insert", "before-commit", "after-commit"]) {
    test(`SIGKILL ${operation}-${point}: restart sees a whole generation`, async t => {
      const { dir, path, store } = setup(t); initial(store);
      const requestFile = join(dir, "request.json"); writeFileSync(requestFile, JSON.stringify(req("b", 1, disabled)));
      if (operation === "publish") store.request(req("b", 1, disabled));
      const marker = join(dir, "paused");
      const process = child(t, [path, operation, operation === "request" ? requestFile : "b", `${operation}-${point}`, marker]);
      await ready(marker);
      if (point !== "after-commit") {
        // Existing and newly opened readers both observe the old complete snapshot.
        assert.deepEqual(store.snapshot().active.bundle, composed);
        const reader = new PublicationStore(path);
        try { assert.deepEqual(reader.snapshot().active.bundle, composed); } finally { reader.close(); }
      }
      process.process.kill("SIGKILL");
      assert.equal((await process.done).signal, "SIGKILL");
      const reopened = new PublicationStore(path);
      try {
        const committed = point === "after-commit";
        assert.equal(reopened.snapshot().desired_revision, operation === "publish" || committed ? 2 : 1);
        assert.equal(reopened.snapshot().active.generation, operation === "publish" && committed ? 2 : 1);
        if (operation === "request") assert.equal(reopened.request(req("b", 1, disabled)).replayed, committed);
        assert.equal(reopened.publish("b").replayed, operation === "publish" && committed);
        assert.deepEqual(reopened.snapshot().active.bundle, disabled);
        integrity(path);
      } finally { reopened.close(); }
    });
  }
}

for (const mode of ["different-requests", "identical-requests", "same-publication"]) {
  test(`competing processes: ${mode}`, async t => {
    const { dir, path, store } = setup(t);
    if (mode === "same-publication") store.request(req());
    const gate = join(dir, "go");
    const processes = [0, 1].map(i => {
      const pathRequest = join(dir, `request-${i}.json`);
      writeFileSync(pathRequest, JSON.stringify(req(mode === "different-requests" ? `r${i}` : "a")));
      const marker = join(dir, `ready-${i}`);
      return { marker, ...child(t, [path, mode === "same-publication" ? "publish" : "request",
        mode === "same-publication" ? "a" : pathRequest, "none", marker, gate]) };
    });
    await Promise.all(processes.map(p => ready(p.marker))); writeFileSync(gate, "go");
    const results = await Promise.all(processes.map(p => p.done));
    if (mode === "different-requests") {
      assert.deepEqual(results.map(r => r.code).sort(), [0, 1]);
      assert.match(results.find(r => r.code === 1).stderr, /stale expected_revision/);
    } else {
      for (const result of results) assert.equal(result.code, 0, result.stderr);
      assert.deepEqual(results.map(r => JSON.parse(r.stdout).replayed).sort(), [false, true]);
    }
    assert.equal(store.snapshot().desired_revision, 1);
    if (mode === "same-publication") assert.deepEqual(store.snapshot().active.bundle, composed);
    integrity(path);
  });
}

test("foreign SQLite and unsupported schema are refused before writes; creation is exclusive", t => {
  const { dir, path } = setup(t);
  assert.throws(() => PublicationStore.create(path, "technical"), /EEXIST/);
  const foreign = join(dir, "foreign.sqlite"), db = new DatabaseSync(foreign);
  db.exec("CREATE TABLE valuable (text TEXT); INSERT INTO valuable VALUES ('preserve');"); db.close();
  const before = readFileSync(foreign);
  assert.throws(() => new PublicationStore(foreign), /not a supported/);
  assert.deepEqual(readFileSync(foreign), before);
  const wrong = join(dir, "schema.sqlite"); copyFileSync(path, wrong);
  const changed = new DatabaseSync(wrong); changed.exec("PRAGMA user_version=2"); changed.close();
  const wrongBefore = readFileSync(wrong);
  assert.throws(() => new PublicationStore(wrong), /not a supported/);
  assert.deepEqual(readFileSync(wrong), wrongBefore);
});

test("stored bundle corruption is refused by reader and publisher", t => {
  const { path, store } = setup(t); initial(store);
  store.request(req("b", 1, disabled));
  const db = new DatabaseSync(path);
  db.prepare("UPDATE requests SET bundle=bundle || ' ' WHERE id=?").run("b");
  assert.throws(() => store.publish("b"), /stored bundle hash mismatch/);
  db.prepare("UPDATE requests SET bundle=bundle || ' ' WHERE id=?").run("a"); db.close();
  assert.throws(() => store.snapshot(), /corrupt active snapshot/);
});

test("request budget is bounded and duplicate delivery remains possible at the limit", t => {
  const { store } = setup(t);
  const bundle = Object.fromEntries(Object.keys(composed).map(k => [k, ""]));
  bundle.manifest = '{"scope":"technical"}';
  for (let i = 0; i < 128; i++) store.request(req(`r${i}`, i, bundle));
  assert.equal(store.request(req("r127", 127, bundle)).replayed, true);
  assert.throws(() => store.request(req("extra", 127, bundle)), /stale expected_revision/);
  assert.throws(() => store.request(req("extra", 128, bundle)), /invalid schema or revision/);
  assert.equal(store.snapshot().active, null);
});

test("historical producer export requires fresh validation after runtime changes", t => {
  const { store } = setup(t); initial(store);
  const old = join(root, "experiments/017-profile-composition-v3");
  const historical = pack(join(old, "composed/composed"), join(old, "inputs/composed.toml"));
  store.request(req("historical", 1, historical));
  assert.throws(() => store.publish("historical"), /producer compiled sources mismatch/);
  assert.equal(store.snapshot().active.generation, 1);
});

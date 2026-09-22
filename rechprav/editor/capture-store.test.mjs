import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { fork, spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { CaptureStore, canonical, validateRequest } from "./capture-store.mjs";

const worker = fileURLToPath(new URL("./testing/process-worker.mjs", import.meta.url));
function request(overrides = {}) {
  return { schema_version: 1, request_id: "slot", fragment_id: "fragment", session_id: "window-1",
    scope: "technical", expected_revision: 0, kind: "replace", before: "сняли свод памяти", after: "сняли слот памяти",
    confirmed_asr: true, ...overrides };
}
function undo(overrides = {}) {
  return { schema_version: 1, request_id: "undo-slot", fragment_id: "fragment", session_id: "window-1",
    scope: "technical", expected_revision: 1, kind: "undo", before: "сняли слот памяти", after: "сняли свод памяти",
    target_request_id: "slot", ...overrides };
}
function setup(t) {
  const directory = mkdtempSync(join(tmpdir(), "rechprav-capture-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const database = join(directory, "capture.sqlite3");
  const store = CaptureStore.create(database);
  return { directory, database, store };
}
function unchanged(store, input, pattern) {
  const before = canonical(store.snapshot());
  assert.throws(() => store.append(input), pattern);
  assert.equal(canonical(store.snapshot()), before);
}

test("durable idempotency survives restart and ignores JSON key ordering", (t) => {
  let { store, database } = setup(t);
  const first = store.append(request());
  store.close();
  store = new CaptureStore(database);
  const reversed = Object.fromEntries(Object.entries(request()).reverse());
  assert.deepEqual(store.append(reversed), { ...first, replayed: true });
  assert.equal(store.snapshot().events.length, 1);
  assert.equal(store.snapshot().fragments[0].revision, 1);
  unchanged(store, request({ after: "сняли свитч памяти" }), /conflicting request/);
  store.close();
});

test("revision and before guards reject stale, future and divergent state without reserving ids", (t) => {
  const { store } = setup(t);
  unchanged(store, request({ expected_revision: 2 }), /noninitial revision/);
  store.append(request());
  for (const expected_revision of [0, 2]) {
    unchanged(store, request({ request_id: "second", expected_revision }), /revision/);
  }
  unchanged(store, request({ request_id: "second", expected_revision: 1 }), /before/);
  const second = request({ request_id: "second", expected_revision: 1, before: "сняли слот памяти", after: "сняли слот", session_id: "window-2", confirmed_asr: false });
  assert.equal(store.append(second).revision, 2);
  assert.equal(store.snapshot().events[1].confirmed_asr, false);
  store.close();
});

test("UTF-16 selection after emoji becomes original UTF-8 byte offsets", (t) => {
  const { store } = setup(t);
  const r = request({ before: "😀 сняли свод памяти", after: "😀 сняли слот памяти",
    selection: { start_utf16: 9, end_utf16: 13, replacement: "слот" } });
  store.append(r);
  const e = store.snapshot().events[0];
  assert.deepEqual(e.selection, { byte_start: 16, byte_end: 24, replacement: "слот" });
  const source = Buffer.from(e.before);
  assert.equal(source.subarray(e.selection.byte_start, e.selection.byte_end).toString(), "свод");
  store.close();
});

test("malformed surrogate, selection and hidden fields cannot enter the journal", (t) => {
  const { store } = setup(t);
  for (const r of [
    request({ before: "\ud800свод" }), request({ after: "\udc00слот" }),
    request({ before: "😀свод", after: "слот", selection: { start_utf16: 1, end_utf16: 6, replacement: "слот" } }),
    request({ selection: { start_utf16: 6, end_utf16: 10, replacement: "свитч" } }),
    request({ selection: { start_utf16: 6, end_utf16: 6, replacement: "слот" } }),
    request({ selection: { start_utf16: 6, end_utf16: 10, replacement: "слот", extra: 1 } }),
    request({ confirmed_asr: "true" }), request({ extra: true }), request({ after: "я".repeat(129) }),
    request({ expected_revision: Number.MAX_SAFE_INTEGER }), request({ request_id: "../escape" }),
    request({ after: "сняли свод памяти" }),
  ]) unchanged(store, r, /./);
  const missing = request(); delete missing.confirmed_asr;
  assert.throws(() => validateRequest(missing), /missing field/);
  store.close();
});

test("undo is atomic, duplicate delivery is inert, redo creates new evidence", (t) => {
  const { store } = setup(t);
  store.append(request());
  const receipt = store.append(undo());
  assert.deepEqual(store.append(undo()), { ...receipt, replayed: true });
  // Re-delivered original correction cannot reactivate the retracted evidence.
  assert.equal(store.append(request()).replayed, true);
  assert.equal(store.snapshot().events.length, 2);
  assert.equal(store.snapshot().fragments[0].text, "сняли свод памяти");
  store.append(request({ request_id: "redo-slot", expected_revision: 2 }));
  const rows = store.snapshot();
  assert.equal(rows.fragments[0].revision, 3);
  assert.deepEqual(rows.events.map((e) => e.event_id), ["slot", "undo-slot", "redo-slot"]);
  assert.equal(rows.events[1].target_event_id, "slot");
  store.close();
});

test("undo cannot target another fragment, a nonlatest edit or a noninverse pair", (t) => {
  const { store } = setup(t);
  store.append(request());
  store.append(request({ request_id: "other", fragment_id: "other" }));
  unchanged(store, undo({ target_request_id: "other" }), /last active replace/);
  unchanged(store, undo({ after: "другое" }), /exact inverse/);
  store.append(request({ request_id: "next", before: "сняли слот памяти", after: "сняли слот", expected_revision: 1, confirmed_asr: false }));
  unchanged(store, undo({ expected_revision: 2, before: "сняли слот" }), /last active replace/);
  store.close();
});

test("scope remains attached to the fragment and other scopes use independent fragments", (t) => {
  const { store } = setup(t);
  store.append(request());
  unchanged(store, undo({ scope: "other" }), /scope mismatch/);
  store.append(request({ request_id: "other", fragment_id: "other", scope: "other", after: "сняли свитч памяти" }));
  assert.deepEqual(store.snapshot().events.map((e) => e.scope), ["technical", "other"]);
  store.close();
});

test("export is a consistent deterministic snapshot and never overwrites an existing directory", (t) => {
  const { store, directory } = setup(t);
  store.append(request());
  const a = join(directory, "a"), b = join(directory, "b");
  assert.deepEqual(store.exportSnapshot(a), store.exportSnapshot(b));
  const original = readFileSync(join(a, "events.jsonl"));
  store.append(undo());
  assert.throws(() => store.exportSnapshot(a), /EEXIST/);
  assert.deepEqual(readFileSync(join(a, "events.jsonl")), original);
  store.close();
});

test("opening a foreign SQLite database or creating over one leaves it unchanged", (t) => {
  const { directory, store } = setup(t); store.close();
  const path = join(directory, "foreign.sqlite3");
  const db = new DatabaseSync(path); db.exec("CREATE TABLE user_data (value TEXT)"); db.close();
  const original = readFileSync(path);
  assert.throws(() => new CaptureStore(path), /not a supported/);
  assert.throws(() => CaptureStore.create(path), /EEXIST/);
  assert.deepEqual(readFileSync(path), original);
});

for (const fault of ["after-insert", "before-commit", "after-commit"]) {
  test(`SIGKILL at ${fault} recovers an all-or-nothing request and supports retry`, (t) => {
    const { store, database, directory } = setup(t); store.close();
    const path = join(directory, "request.json"); writeFileSync(path, JSON.stringify(request()));
    const child = spawnSync(process.execPath, [worker, database, path, fault], { encoding: "utf8", timeout: 10_000 });
    assert.equal(child.signal, "SIGKILL", child.stderr);
    const recovered = new CaptureStore(database);
    const committed = fault === "after-commit";
    assert.equal(recovered.snapshot().events.length, committed ? 1 : 0);
    assert.equal(recovered.snapshot().fragments.length, committed ? 1 : 0);
    assert.equal(recovered.append(request()).replayed, committed);
    assert.equal(recovered.snapshot().events.length, 1);
    assert.equal(recovered.snapshot().fragments[0].revision, 1);
    recovered.close();
    const db = new DatabaseSync(database);
    assert.equal(db.prepare("PRAGMA integrity_check").get().integrity_check, "ok"); db.close();
  });
}

test("interrupted undo rolls back the retraction together with the request and fragment", (t) => {
  const { store, database, directory } = setup(t);
  store.append(request()); store.close();
  const path = join(directory, "undo.json"); writeFileSync(path, JSON.stringify(undo()));
  const child = spawnSync(process.execPath, [worker, database, path, "after-insert"], { encoding: "utf8", timeout: 10_000 });
  assert.equal(child.signal, "SIGKILL", child.stderr);
  const recovered = new CaptureStore(database);
  assert.equal(recovered.snapshot().events.length, 1);
  assert.equal(recovered.snapshot().fragments[0].text, "сняли слот памяти");
  assert.equal(recovered.append(undo()).replayed, false);
  assert.equal(recovered.snapshot().events.length, 2);
  recovered.close();
});

test("CLI rejects invalid UTF-8 instead of silently replacing input bytes", (t) => {
  const { store, database, directory } = setup(t); store.close();
  const path = join(directory, "invalid.json");
  const valid = Buffer.from(JSON.stringify(request()));
  valid[valid.indexOf(Buffer.from("свод"))] = 0xff;
  writeFileSync(path, valid);
  const cli = fileURLToPath(new URL("./capture-cli.mjs", import.meta.url));
  const child = spawnSync(process.execPath, [cli, "append", database, path], { encoding: "utf8", timeout: 10_000 });
  assert.equal(child.status, 1);
  assert.match(child.stderr, /encoded data/);
  const recovered = new CaptureStore(database);
  assert.equal(recovered.snapshot().events.length, 0); recovered.close();
});

for (const identical of [false, true]) {
test(`concurrent processes serialize ${identical ? "identical delivery" : "distinct edits"}`, { timeout: 20_000 }, async (t) => {
  const { store, database, directory } = setup(t); store.close();
  const children = [request(), identical ? request() : request({ request_id: "rival", after: "сняли свитч памяти" })].map((r, i) => {
    const path = join(directory, `${i}.json`); writeFileSync(path, JSON.stringify(r));
    const child = fork(worker, [database, path], { stdio: ["ignore", "ignore", "pipe", "ipc"] });
    let result;
    const ready = new Promise((resolve, reject) => {
      child.once("error", reject);
      child.once("message", (m) => { assert.equal(m.ready, true); resolve(); });
    });
    const done = new Promise((resolve, reject) => {
      child.on("message", (m) => { if (m.ok !== undefined) result = m; });
      child.once("error", reject);
      child.once("exit", (code) => code === 0 && result ? resolve(result) : reject(new Error(`worker exit ${code}`)));
    });
    t.after(() => { if (child.exitCode === null) child.kill(); });
    return { child, ready, done };
  });
  await Promise.all(children.map((c) => c.ready));
  for (const c of children) c.child.send("start");
  const results = await Promise.all(children.map((c) => c.done));
  assert.equal(results.filter((r) => r.ok).length, identical ? 2 : 1);
  if (identical) assert.equal(results.filter((r) => r.receipt.replayed).length, 1);
  else assert.match(results.find((r) => !r.ok).error, /revision/);
  const recovered = new CaptureStore(database);
  assert.equal(recovered.snapshot().events.length, 1);
  assert.equal(recovered.snapshot().fragments[0].revision, 1);
  recovered.close();
});
}

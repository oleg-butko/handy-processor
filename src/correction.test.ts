import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { test, type TestContext } from "node:test";
import { CorrectionWorker } from "./correction-worker.js";
import { transformTranscriptionAsync } from "./transform.js";
import { TRANSFORMATION_RULE_IDS as ids } from "./transformation-rules.js";
import { TransformationLog } from "./transformation-log.js";
import { RuleSettings } from "./rule-settings.js";
import { EditorSessionRegistry } from "./editor-sessions.js";
import { createRequestHandler } from "./app.js";

const executable = resolve("rechprav/target/release/examples", process.platform === "win32" ? "correction_worker.exe" : "correction_worker");
function fixture(t: TestContext) {
  const directory = mkdtempSync(join(tmpdir(), "handy-correction-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  writeFileSync(join(directory, "dictionary.txt"), "свод\nсняли\nслот\nслота\nбез\nснятия\nправил\nпензе\nв\n");
  writeFileSync(join(directory, "dictionary.toml"), 'dictionary = "dictionary.txt"\n');
  const config = join(directory, "terms.toml");
  writeFileSync(config, readFileSync("rechprav/handy-terms.toml", "utf8").replace('"handy-dictionary.toml"', '"dictionary.toml"'));
  const worker = new CorrectionWorker({ executable, config });
  t.after(() => worker.close());
  return { directory, config, worker };
}

test("real worker: one decision for overlapping methods, scores, context and original-case protection", async (t) => {
  const { worker } = fixture(t);
  assert.equal(worker.status().state, "stopped");
  const off = await transformTranscriptionAsync("калиборд", new Set(), undefined, worker);
  assert.equal(off.text, "калиборд");
  assert.equal(worker.status().state, "stopped");
  const cases = [
    ["калиборд", "клипборд"], ["без снятия сваты", "без снятия слота"],
    ["сняли свод", "сняли слот"], ["сняли свод правил", "сняли свод правил"],
    ["долгождать", "долго ждать"], ["проковол колесо", "проколол колесо"],
    ["недозвода клиента", "недозвон до клиента"], ["проверь пэ эн зэ", "проверь ПНЗ"],
    ["в пензе", "в пензе"], ["КАЛИБОРД", "КАЛИБОРД"],
  ];
  for (const [input, expected] of cases) {
    const output = await worker.analyze(input!);
    assert.equal(output.text, expected, input);
    assert.equal(output.trace.complete, true);
    for (const decision of output.trace.decisions) for (const candidate of decision.candidates) {
      assert.ok(Math.abs(Object.values(candidate.parts).reduce((a, b) => a + b, 0) - candidate.score) < 1e-8);
    }
  }
  const formatted = await transformTranscriptionAsync("КАЛИБОРД.", new Set([ids.correction, ids.lowercase, ids.removeFinalPeriod]), undefined, worker);
  assert.equal(formatted.text, "калиборд"); // Lowercasing must not expose the protected token to correction.
  assert.deepEqual(formatted.appliedRules.map(r => r.id), [ids.correction, ids.lowercase, ids.removeFinalPeriod]);
});

test("real worker: conflict keeps input, changed weights require reload, incomplete search applies nothing", async (t) => {
  const { worker, config } = fixture(t);
  const original = readFileSync(config, "utf8");
  const extra = '\n[[sets.terms]]\nid="competitor"\ncanonical="свитч"\naliases=["свод"]\nboost=3.0\nknown_words="always"\ngeneration="exact"\n';
  writeFileSync(config, original + extra);
  const conflict = await worker.analyze("сняли свод");
  assert.equal(conflict.text, "сняли свод");
  assert.ok(conflict.trace.decisions.some(d => d.reason === "term_conflict"));
  writeFileSync(config, original);
  assert.equal((await worker.analyze("сняли свод")).text, "сняли свод");
  await worker.reload();
  const updated = await worker.analyze("сняли свод");
  assert.equal(updated.text, "сняли слот");
  assert.notEqual(updated.trace.profile, conflict.trace.profile);
  writeFileSync(config, original.replace("max_comparisons = 20000", "max_comparisons = 1"));
  await worker.reload();
  const limited = await transformTranscriptionAsync("калиборд.", new Set([ids.correction, ids.removeFinalPeriod]), undefined, worker);
  assert.equal(limited.text, "калиборд");
  assert.equal(limited.appliedRules[0]?.reason, "correction-incomplete");
});

test("HTTP toggle and persisted explanations survive SQLite reopening and profile changes", async (t) => {
  const { worker, directory, config } = fixture(t);
  const database = join(directory, "history.sqlite");
  let log = new TransformationLog(database);
  t.after(() => log.close());
  const settings = new RuleSettings(log);
  const handler = createRequestHandler({ correctionWorker: worker, ruleSettings: settings,
    transformationHistory: log, editorDraftStorage: log, editorSessions: new EditorSessionRegistry(),
    diagnosticLog: { path: "unused", async appendClientError() {} }, webUiAssets: { get: () => undefined } });
  const server = createServer((req, res) => { void handler(req, res); });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise<void>(resolve => server.close(() => resolve())));
  const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  const post = () => fetch(`${base}/v1/chat/completions`, { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ messages: [{ role: "user", content: "без снятия сваты" }] }) }).then(r => r.json());
  assert.equal((await post()).choices[0].message.content, "без снятия сваты");
  const toggle = (enabled: boolean) => fetch(`${base}/api/rules/${ids.correction}`, { method: "PATCH",
    headers: { "content-type": "application/json" }, body: JSON.stringify({ enabled }) });
  assert.equal((await toggle(true)).status, 200);
  assert.equal((await post()).choices[0].message.content, "без снятия слота");
  const snapshot = log.list(1, 1).items[0]!.appliedRules;
  assert.ok(snapshot?.[0]?.correction?.decisions.some(d => d.selected === "слота"));
  assert.equal((await toggle(false)).status, 200);
  assert.equal((await post()).choices[0].message.content, "без снятия сваты");
  assert.equal((await fetch(`${base}/api/correction/reload`, { method: "POST" })).status, 403);
  writeFileSync(config, readFileSync(config, "utf8").replaceAll("boost = 1.0", "boost = 2.0"));
  const reload = await fetch(`${base}/api/correction/reload`, { method: "POST", headers: { "content-type": "application/json", origin: base }, body: "{}" });
  assert.equal(reload.status, 200);
  assert.deepEqual(log.list(1, 3).items[1]!.appliedRules, snapshot);
  log.close(); log = new TransformationLog(database);
  assert.deepEqual(log.list(1, 3).items[1]!.appliedRules, snapshot);
  assert.equal(new RuleSettings(log).enabledRuleIds().has(ids.correction), false);
});

test("unavailable worker and oversized input preserve text and leave formatting available", async (t) => {
  const { directory } = fixture(t);
  const missing = new CorrectionWorker({ executable: join(directory, "absent"), config: "absent" });
  t.after(() => missing.close());
  for (const input of ["Калиборд.", "я".repeat(9000)]) {
    const result = await transformTranscriptionAsync(input, new Set([ids.correction, ids.lowercase]), undefined, missing);
    assert.equal(result.text, input.toLowerCase());
    assert.equal(result.appliedRules[0]?.reason, "correction-unavailable");
    assert.ok(result.appliedRules[0]?.error);
  }
});

test("bounded queue and timeout reject all pending work; a later request starts a fresh process", async (t) => {
  const { directory } = fixture(t);
  const fake = join(directory, "fake.mjs");
  writeFileSync(fake, `console.log(JSON.stringify({ready:true,protocol:1,profile:'test'})); process.stdin.resume();`);
  const worker = new CorrectionWorker({ executable: process.execPath, arguments: [fake], config: "unused", timeoutMs: 500, maxPending: 1 });
  t.after(() => worker.close());
  const first = worker.analyze("свод");
  const failed = assert.rejects(first, /время/);
  await assert.rejects(worker.analyze("ещё"), /Очередь/);
  await assert.rejects(worker.reload(), /Дождитесь/);
  await failed;
  assert.equal(worker.status().state, "stopped");
  writeFileSync(fake, `console.log(JSON.stringify({ready:true,protocol:1,profile:'test'}));
    let buffer=''; process.stdin.setEncoding('utf8'); process.stdin.on('data', chunk => {
      buffer+=chunk; let end; while((end=buffer.indexOf('\\n'))>=0) {
        const q=JSON.parse(buffer.slice(0,end)); buffer=buffer.slice(end+1);
        console.log(JSON.stringify({id:q.id,text:q.text,trace:{schemaVersion:1,profile:'test',complete:true,decisions:[],minScore:3,minMargin:1,omittedDecisions:0,omittedCandidates:0}}));
      }
    });`);
  assert.equal((await worker.analyze("свод")).text, "свод");
});

test("worker crash and malformed explanation cannot enter persisted history as a successful correction", async (t) => {
  const { directory } = fixture(t);
  const fake = join(directory, "broken.mjs");
  for (const action of ["process.exit(7)", "console.log(JSON.stringify({id:JSON.parse(line).id,text:'wrong',trace:{schemaVersion:1,profile:'test',complete:true,decisions:[{candidates:'broken'}]}}))"]) {
    writeFileSync(fake, `console.log(JSON.stringify({ready:true,protocol:1,profile:'test'}));
      process.stdin.setEncoding('utf8'); process.stdin.once('data', line => { ${action}; });`);
    const worker = new CorrectionWorker({ executable: process.execPath, arguments: [fake], config: "unused", timeoutMs: 1000 });
    try {
      const result = await transformTranscriptionAsync("исходник", new Set([ids.correction]), undefined, worker);
      assert.equal(result.text, "исходник");
      assert.equal(result.appliedRules[0]?.outcome, "skipped");
      assert.equal(result.appliedRules[0]?.correction, undefined);
      assert.equal(worker.status().state, "stopped");
    } finally { worker.close(); }
  }
});

test("invalid reload keeps the working profile and subsequent corrections available", async (t) => {
  const { worker, config } = fixture(t);
  const before = await worker.analyze("калиборд");
  writeFileSync(config, "invalid toml [");
  await assert.rejects(worker.reload());
  assert.equal(worker.status().state, "ready");
  assert.equal(worker.status().profile, before.trace.profile);
  assert.equal((await worker.analyze("калиборд")).text, "клипборд");
});

test("synchronous spawn errors do not leave later requests waiting forever", async () => {
  const worker = new CorrectionWorker({ executable: "", config: "unused", timeoutMs: 100 });
  try {
    await assert.rejects(worker.analyze("первый"));
    let timer: NodeJS.Timeout | undefined;
    const second = worker.analyze("второй").then(() => "resolved", () => "rejected");
    const result = await Promise.race([second, new Promise<string>(resolve => {
      timer = setTimeout(() => resolve("hung"), 250);
    })]);
    clearTimeout(timer);
    assert.equal(result, "rejected");
  } finally { worker.close(); }
});

test("valid short input may expand beyond 32768 characters without discarding all corrections", async (t) => {
  const { worker, config } = fixture(t);
  const canonical = "слот".repeat(16);
  writeFileSync(config, `baseline_config="dictionary.toml"\nenabled=true\n[[sets]]\nid="long"\n[[sets.terms]]\nid="long"\ncanonical="${canonical}"\naliases=["я"]\ngeneration="exact"\nknown_words="always"\nboost=1.0\n`);
  const text = Array(600).fill("я").join(" ");
  const result = await worker.analyze(text);
  assert.equal(result.text, Array(600).fill(canonical).join(" "));
  assert.ok(result.trace.omittedDecisions > 0);
});

test("reload isolates generations, keeps in-flight results, and bounds concurrent replacements", async (t) => {
  const { directory } = fixture(t);
  const fake = join(directory, "generations.mjs");
  const config = join(directory, "generation.json");
  writeFileSync(fake, `import {readFileSync} from 'node:fs';
    const config=JSON.parse(readFileSync(process.argv[2], 'utf8'));
    setTimeout(()=>console.log(JSON.stringify({ready:true,protocol:1,profile:config.profile})),config.delay);
    let buffer=''; process.stdin.setEncoding('utf8'); process.stdin.on('data',chunk=>{
      buffer+=chunk; let end; while((end=buffer.indexOf('\\n'))>=0){
        const q=JSON.parse(buffer.slice(0,end)); buffer=buffer.slice(end+1);
        setTimeout(()=>console.log(JSON.stringify({id:q.id,text:q.text,trace:{schemaVersion:1,profile:config.profile,complete:true,decisions:[],minScore:3,minMargin:1,omittedDecisions:0,omittedCandidates:0}})),q.text==='slow'?500:0);
      }
    });`);
  writeFileSync(config, JSON.stringify({ profile: "old", delay: 0 }));
  const worker = new CorrectionWorker({ executable: process.execPath, arguments: [fake], config, timeoutMs: 1000, maxPending: 1 });
  t.after(() => worker.close());
  assert.equal((await worker.analyze("warm")).trace.profile, "old");
  writeFileSync(config, JSON.stringify({ profile: "new", delay: 100 }));
  const reload = worker.reload();
  await assert.rejects(worker.reload(), /Дождитесь/);
  const oldRequest = worker.analyze("slow");
  await reload;
  assert.equal(worker.status().profile, "new");
  await assert.rejects(worker.analyze("over capacity"), /Очередь/);
  await assert.rejects(worker.reload(), /Дождитесь/);
  assert.equal((await oldRequest).trace.profile, "old");
  assert.equal((await worker.analyze("fresh")).trace.profile, "new");
  writeFileSync(config, JSON.stringify({ profile: "stalled", delay: 10000 }));
  await assert.rejects(worker.reload(), /время загрузки/);
  assert.equal((await worker.analyze("still fresh")).trace.profile, "new");
  const closing = worker.reload();
  worker.close();
  await assert.rejects(closing, /остановлен/);
  await assert.rejects(worker.analyze("closed"), /остановлен/);
});

// Verify a copied runtime outside the checkout, using the native worker.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { correctionRuntimeFiles, copyCorrectionRuntime } from "../../scripts/correction-runtime.mjs";

const root = resolve(import.meta.dirname, "../..");
const directory = mkdtempSync(join(tmpdir(), "handy-runtime-"));
try {
  const copied = copyCorrectionRuntime(correctionRuntimeFiles(root), directory);
  const cases = [
    ["калиборд", "клипборд"],
    ["без снятия сваты", "без снятия слота"],
    ["сняли свод правил", "сняли свод правил"],
    ["проверь пэ эн зэ", "проверь ПНЗ"],
  ];
  const run = spawnSync(copied[0], [join(directory, "rechprav/handy-terms.toml")], {
    cwd: directory, encoding: "utf8", timeout: 30_000, maxBuffer: 1024 * 1024,
    input: cases.map(([text], id) => JSON.stringify({ id, text })).join("\n") + "\n",
  });
  assert.ifError(run.error);
  assert.equal(run.status, 0, run.stderr);
  const [ready, ...results] = run.stdout.trim().split("\n").map(line => JSON.parse(line));
  assert.equal(ready.ready, true);
  assert.equal(results.length, cases.length);
  for (const [id, result] of results.entries()) {
    assert.equal(result.id, id);
    assert.equal(result.trace.complete, true);
    assert.equal(result.text, cases[id][1]);
  }
  console.log(JSON.stringify({ platform: process.platform, profile: ready.profile,
    copied: copied.map(path => path.slice(directory.length + 1)),
    cases: cases.map(([input, expected], id) => ({ input, expected, actual: results[id].text })),
  }, null, 2));
} finally {
  rmSync(directory, { recursive: true, force: true });
}

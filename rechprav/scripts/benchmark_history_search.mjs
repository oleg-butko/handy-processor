// Measure the synchronous search on a technical 50k-row history, never user data.
import { DatabaseSync } from "node:sqlite";
import { performance } from "node:perf_hooks";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TransformationLog } from "../../dist/transformation-log.js";

const directory = mkdtempSync(join(tmpdir(), "handy-history-review-"));
try {
  const path = join(directory, "history.sqlite");
  new TransformationLog(path).close();
  const database = new DatabaseSync(path);
  const insert = database.prepare("INSERT INTO transformations(original_text,transformed_text,rules_snapshot,source) VALUES(?,?,?,?)");
  const plain = JSON.stringify({ pipelineVersion: 2, applied: [
    { id: "case.lowercase", version: 1, outcome: "unchanged", changed: false },
  ] });
  const changed = JSON.stringify({ pipelineVersion: 2, applied: [
    { id: "recognition.terms-and-phrases", version: 1, outcome: "changed", changed: true,
      correction: { technicalPadding: "x".repeat(800) } },
  ] });
  database.exec("BEGIN");
  for (let i = 0; i < 50_000; i++) {
    insert.run(`запись номер ${i} про схему и результаты`,
      i % 20 === 0 ? `запись номер ${i} со словом слот` : `обычный результат номер ${i}`,
      i % 20 === 0 ? changed : plain, "external");
  }
  database.exec("COMMIT");
  database.close();

  const log = new TransformationLog(path);
  const timings = {};
  for (const [name, filter] of [
    ["text", { query: "слот" }],
    ["correction", { correctionOnly: true }],
    ["combined", { query: "слот", correctionOnly: true }],
    ["phrase", { query: "схему и результаты", searchIn: "original" }],
  ]) {
    const started = performance.now();
    const page = log.list(1, 10, filter);
    timings[name] = { milliseconds: Math.round(performance.now() - started), matches: page.total };
  }
  log.close();
  // Recreate the previous SQL callback on the same data, count and first page.
  const legacy = new DatabaseSync(path);
  legacy.function("legacy_contains", { deterministic: true }, (value, query) =>
    typeof value === "string" && typeof query === "string" &&
    value.replace(/\s+/gu, " ").toLocaleLowerCase("ru-RU").includes(query) ? 1 : 0);
  const legacyTimings = {};
  for (const [name, extra] of [["text", ""], ["combined", ` AND EXISTS (
    SELECT 1 FROM json_each(transformations.rules_snapshot, '$.applied') AS rule
    WHERE json_extract(rule.value, '$.id') = 'recognition.terms-and-phrases'
      AND json_extract(rule.value, '$.outcome') = 'changed'
      AND json_extract(rule.value, '$.changed') = 1
  )`]]) {
    const where = "(legacy_contains(original_text, ?) = 1 OR legacy_contains(transformed_text, ?) = 1)" + extra;
    const started = performance.now();
    const count = legacy.prepare(`SELECT COUNT(*) AS total FROM transformations WHERE ${where}`).get("слот", "слот").total;
    legacy.prepare(`SELECT id, original_text, transformed_text, created_at, rules_snapshot, source, context_snapshot
      FROM transformations WHERE ${where} ORDER BY id DESC LIMIT ? OFFSET ?`).all("слот", "слот", 10, 0);
    legacyTimings[name] = { milliseconds: Math.round(performance.now() - started), matches: count };
    if (count !== timings[name].matches) throw new Error(`Search results changed for ${name}`);
  }
  legacy.close();
  console.log(JSON.stringify({ rows: 50_000, optimized: timings, legacy: legacyTimings }, null, 2));
} finally {
  rmSync(directory, { recursive: true, force: true });
}

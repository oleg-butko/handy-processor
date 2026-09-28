import assert from "node:assert/strict";
import test from "node:test";
import { summarizeAnnotations } from "./analysis.js";
import { buildIndex } from "./search.js";

test("confirmed pairs and processor outcomes are counted separately", () => {
  const records = [
    { id: 1, originalText: "свод", transformedText: "слот" },
    { id: 2, originalText: "свод", transformedText: "свод" },
    { id: 3, originalText: "слот", transformedText: "свод" },
    { id: 4, originalText: "свод", transformedText: "ввод" },
    { id: 5, originalText: "неясно", transformedText: "неясно" },
  ];
  const annotations = new Map([
    [1, { status: "reviewed", gold: "слот", originKind: "synthetic" }],
    [2, { status: "reviewed", gold: "слот", originKind: "real-asr" }],
    [3, { status: "reviewed", gold: "слот", originKind: "unknown" }],
    [4, { status: "reviewed", gold: "слот", originKind: "synthetic" }],
  ]);
  const result = summarizeAnnotations(buildIndex(records), annotations);
  assert.equal(result.totals.corrected, 3);
  assert.equal(result.totals.exactFix, 1);
  assert.equal(result.totals.missed, 1);
  assert.equal(result.totals.otherResult, 1);
  assert.equal(result.totals.damagedCorrect, 1);
  assert.equal(result.totals.realAsr, 1);
  assert.equal(result.totals.synthetic, 2);
  assert.equal(result.pairs[0].count, 3);
  assert.equal(result.pairs[0].realAsr, 1);
  assert.equal(result.pairs[0].synthetic, 2);
  assert.deepEqual(result.pairs[0].ids, [1, 2, 4]);
});

test("only the 30 most frequent reviewed pairs are ranked", () => {
  const records = Array.from({ length: 100 }, (_, i) => ({ id: i + 1, originalText: `ошибка ${i}`, transformedText: `ошибка ${i}` }));
  const annotations = new Map(records.map((record) => [record.id, { status: "reviewed", gold: `правильно ${record.id}`, originKind: "synthetic" }]));
  const result = summarizeAnnotations(buildIndex(records), annotations);
  assert.equal(result.pairCount, 100);
  assert.equal(result.pairs.length, 30);
  assert.ok(result.pairs.every((pair, i, all) => i === 0 || all[i - 1].source.localeCompare(pair.source, "ru") <= 0));
});

test("top pair ranking matches a full sort when frequencies differ", () => {
  const records = [];
  const annotations = new Map();
  const counts = new Map();
  for (let group = 0; group < 80; group += 1) {
    const count = (group * 17) % 9 + 1;
    const source = `ошибка ${group}`;
    counts.set(source, count);
    for (let n = 0; n < count; n += 1) {
      const id = records.length + 1;
      records.push({ id, originalText: source, transformedText: source });
      annotations.set(id, { status: "reviewed", gold: `правка ${group}`, originKind: "synthetic" });
    }
  }
  const actual = summarizeAnnotations(buildIndex(records), annotations).pairs.map((pair) => pair.source);
  const expected = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], "ru"))
    .slice(0, 30).map(([source]) => source);
  assert.deepEqual(actual, expected);
});

test("pair keys cannot collide when imported text contains control characters", () => {
  const records = [
    { id: 1, originalText: "а\u0000б", transformedText: "а\u0000б" },
    { id: 2, originalText: "а", transformedText: "а" },
  ];
  const annotations = new Map([
    [1, { status: "reviewed", gold: "в", originKind: "synthetic" }],
    [2, { status: "reviewed", gold: "б\u0000в", originKind: "synthetic" }],
  ]);
  const result = summarizeAnnotations(buildIndex(records), annotations);
  assert.equal(result.pairCount, 2);
});

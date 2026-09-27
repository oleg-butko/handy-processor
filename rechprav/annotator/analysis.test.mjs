import assert from "node:assert/strict";
import test from "node:test";
import { summarizeAnnotations } from "./analysis.js";

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
  const result = summarizeAnnotations(records, annotations);
  assert.equal(result.totals.corrected, 3);
  assert.equal(result.totals.exactFix, 1);
  assert.equal(result.totals.missed, 1);
  assert.equal(result.totals.otherResult, 1);
  assert.equal(result.totals.damagedCorrect, 1);
  assert.equal(result.totals.realAsr, 1);
  assert.equal(result.totals.synthetic, 2);
  assert.equal(result.pairs[0].count, 3);
  assert.deepEqual(result.pairs[0].ids, [1, 2, 4]);
});

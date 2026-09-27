import assert from "node:assert/strict";
import test from "node:test";
import { buildIndex, editDistance, search, words } from "./search.js";

const records = [
  { id: 1, originalText: "сняли свод", transformedText: "сняли слот", source: "external", appliedRules: [{ id: "recognition.terms-and-phrases", outcome: "changed", changed: true }] },
  { id: 2, originalText: "сняли свод", transformedText: "сняли свод", source: "external", appliedRules: null },
  { id: 3, originalText: "сняли слот", transformedText: "сняли слот", source: "voice-editor", appliedRules: null },
  { id: 4, originalText: "проверили ввод", transformedText: "проверили ввод", source: "external", appliedRules: null },
];

test("Unicode words and adjacent phrase matching", () => {
  assert.deepEqual(words("СНЯЛИ, свод!"), ["сняли", "свод"]);
  const index = buildIndex(records);
  const exact = search(index, { query: "сняли свод", distance: 0 });
  assert.equal(exact.total, 2);
  assert.equal(exact.groups[0].phrase, "сняли свод");
  assert.equal(exact.groups[0].ids.length, 2);
  assert.equal(search(index, { query: "свод сняли", distance: 0 }).total, 0);
});

test("two edits find a term confusion, with filters and separate variant counts", () => {
  assert.equal(editDistance("свод", "слот", 2), 2);
  assert.equal(editDistance("аб", "ба", 1), 1);
  const index = buildIndex(records);
  const fuzzy = search(index, { query: "сняли свод", distance: 2 });
  assert.equal(fuzzy.total, 3);
  assert.deepEqual(fuzzy.groups.map((group) => [group.phrase, group.ids.length]), [["сняли свод", 2], ["сняли слот", 1]]);
  assert.equal(search(index, { query: "сняли свод", distance: 2, changedOnly: true }).total, 1);
  assert.equal(search(index, { query: "сняли свод", distance: 2, source: "voice-editor" }).total, 1);
  assert.equal(search(index, { query: "слот", mode: "result", distance: 0 }).total, 2);
});

test("empty query lists repeated originals and short broad fuzzy searches are refused", () => {
  const index = buildIndex(records);
  const all = search(index);
  assert.equal(all.total, 4);
  assert.equal(all.groups[0].phrase, "сняли свод");
  assert.equal(all.groups[0].ids.length, 2);
  assert.throws(() => search(index, { query: "пнз", distance: 2 }), /более длинные слова/u);
});

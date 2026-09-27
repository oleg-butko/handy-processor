import assert from "node:assert/strict";
import test from "node:test";
import { defaultAnnotation, parseJsonl, serializeProject } from "./project.js";

const history = { id: 1, originalText: "свод", transformedText: "слот", source: "external" };

test("history import never promotes processor output to gold", () => {
  const parsed = parseJsonl(JSON.stringify(history), false);
  assert.equal(parsed.records.length, 1);
  assert.equal(parsed.annotations.size, 0);
  assert.equal(defaultAnnotation().gold, null);
});

test("legacy projects migrate to unconfirmed provenance and no manual group", () => {
  const line = JSON.stringify({ schema_version: 1, history, annotation: { status: "reviewed", gold: "слот", note: "проверено" } });
  const parsed = parseJsonl(line, true);
  assert.deepEqual(parsed.annotations.get(1), { status: "reviewed", gold: "слот", note: "проверено", originKind: "unknown", groupId: "" });
  const saved = serializeProject(parsed.records, parsed.annotations);
  assert.equal(JSON.parse(saved).schema_version, 2);
  assert.deepEqual(parseJsonl(saved, true).annotations.get(1), parsed.annotations.get(1));
});

test("invalid or incomplete projects cannot silently replace loaded work", () => {
  assert.throws(() => parseJsonl("\n", false), /не содержит записей/u);
  assert.throws(() => parseJsonl(`${JSON.stringify(history)}\n${JSON.stringify(history)}`, false), /повтор ID/u);
  const bad = { schema_version: 2, history, annotation: { status: "reviewed", gold: "слот", note: "", groupId: "", originKind: "guess" } };
  assert.throws(() => parseJsonl(JSON.stringify(bad), true), /неверная разметка/u);
});

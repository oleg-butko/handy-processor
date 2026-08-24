import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { detectSentencePosition } from "./transformation-context.js";

describe("detectSentencePosition", () => {
  it("recognizes the start of an empty or decorated document", () => {
    assert.equal(detectSentencePosition("", 0), "start");
    assert.equal(detectSentencePosition("  «", 3), "start");
  });

  it("recognizes sentence terminators before whitespace and quotes", () => {
    assert.equal(detectSentencePosition("Первое.   ", 10), "start");
    assert.equal(detectSentencePosition("Он спросил: «Да?»  ", 18), "start");
    assert.equal(detectSentencePosition("Готово!\n", 8), "start");
    assert.equal(detectSentencePosition("Пауза… ", 7), "start");
  });

  it("recognizes a position in the middle of a sentence", () => {
    assert.equal(detectSentencePosition("Первое предложение", 7), "middle");
    assert.equal(detectSentencePosition("Без точки   ", 12), "middle");
  });

  it("uses the start of a selection and rejects invalid offsets", () => {
    assert.equal(detectSentencePosition("Начало. Замена", 8), "start");
    assert.equal(detectSentencePosition("Текст", -1), "unknown");
    assert.equal(detectSentencePosition("Текст", 6), "unknown");
    assert.equal(detectSentencePosition("Текст", 1.5), "unknown");
  });
});

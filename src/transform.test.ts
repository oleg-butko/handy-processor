import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { transformTranscription } from "./transform.js";
import { TRANSFORMATION_RULE_IDS } from "./transformation-rules.js";

function transform(text: string): string {
  return transformTranscription(text).text;
}

describe("transformTranscription", () => {
  it("lowercases text and removes its final period", () => {
    assert.equal(transform("HELLO World."), "hello world");
  });

  it("keeps periods that are not at the end of the input", () => {
    assert.equal(
      transform("FIRST SENTENCE. SECOND SENTENCE."),
      "first sentence. second sentence",
    );
  });

  it("preserves trailing whitespace", () => {
    assert.equal(transform("HELLO.  \n"), "hello  \n");
  });

  it("does not remove other final punctuation", () => {
    assert.equal(transform("IS THIS OK?"), "is this ok?");
  });

  it("normalizes simple Russian numerals as part of the transformation", () => {
    assert.equal(
      transform("МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА."),
      "мне нужно 22 файла",
    );
  });

  it("normalizes inflected numbers with scales", () => {
    assert.equal(
      transform("НЕ ХВАТАЕТ ДВУХ ТЫСЯЧ ДВАДЦАТИ ДВУХ ФАЙЛОВ."),
      "не хватает 2022 файлов",
    );
  });

  it("corrects a conservative numeral typo", () => {
    assert.equal(
      transform("МНЕ НУЖНО ДВАЦАТЬ ДВА ФАЙЛА."),
      "мне нужно 22 файла",
    );
  });

  it("can apply each rule independently", () => {
    assert.equal(
      transformTranscription(
        "МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.",
        new Set([TRANSFORMATION_RULE_IDS.russianNumerals]),
      ).text,
      "МНЕ НУЖНО 22 ФАЙЛА.",
    );
    assert.equal(
      transformTranscription(
        "HELLO World.",
        new Set([TRANSFORMATION_RULE_IDS.removeFinalPeriod]),
      ).text,
      "HELLO World",
    );
    assert.equal(
      transformTranscription(
        "HELLO World.",
        new Set([TRANSFORMATION_RULE_IDS.lowercase]),
      ).text,
      "hello world.",
    );
  });

  it("returns the rules that ran and whether they changed the text", () => {
    const result = transformTranscription(
      "already lowercase",
      new Set([
        TRANSFORMATION_RULE_IDS.lowercase,
        TRANSFORMATION_RULE_IDS.removeFinalPeriod,
      ]),
    );

    assert.deepEqual(result, {
      text: "already lowercase",
      appliedRules: [
        {
          id: TRANSFORMATION_RULE_IDS.lowercase,
          version: 1,
          outcome: "unchanged",
          changed: false,
        },
        {
          id: TRANSFORMATION_RULE_IDS.removeFinalPeriod,
          version: 1,
          outcome: "unchanged",
          changed: false,
        },
      ],
    });
  });

  it("leaves text unchanged when every rule is disabled", () => {
    const result = transformTranscription("НЕ МЕНЯЙ МЕНЯ.", new Set());
    assert.deepEqual(result, { text: "НЕ МЕНЯЙ МЕНЯ.", appliedRules: [] });
  });
});

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  TRANSFORMATION_SOURCES,
  type SentencePosition,
  type VoiceEditorTransformationContext,
} from "./transformation-context.js";
import { transformTranscription } from "./transform.js";
import { TRANSFORMATION_RULE_IDS } from "./transformation-rules.js";

function transform(text: string): string {
  return transformTranscription(text).text;
}

function editorContext(
  sentencePosition: SentencePosition,
): VoiceEditorTransformationContext {
  return {
    source: TRANSFORMATION_SOURCES.voiceEditor,
    editor: {
      sessionId: "test-session",
      documentRevision: 1,
      selectionStart: 5,
      selectionEnd: 5,
      sentencePosition,
    },
  };
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

  it("keeps commas by default and removes all of them when enabled", () => {
    assert.equal(transform("ПЕРВЫЙ, ВТОРОЙ, ТРЕТИЙ."), "первый, второй, третий");
    const result = transformTranscription(
      "Первый, второй, третий.",
      new Set([
        TRANSFORMATION_RULE_IDS.removeAllCommas,
        TRANSFORMATION_RULE_IDS.removeFinalPeriod,
      ]),
    );
    assert.equal(result.text, "Первый второй третий");
    assert.deepEqual(result.appliedRules.map((rule) => rule.id), [
      TRANSFORMATION_RULE_IDS.removeFinalPeriod,
      TRANSFORMATION_RULE_IDS.removeAllCommas,
    ]);
    assert.equal(
      transformTranscription(
        "Начало, продолжение!",
        new Set([TRANSFORMATION_RULE_IDS.removeAllCommas]),
        editorContext("start"),
      ).text,
      "Начало продолжение!",
    );
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
    assert.equal(
      transformTranscription(
        "One, two, three.",
        new Set([TRANSFORMATION_RULE_IDS.removeAllCommas]),
      ).text,
      "One two three.",
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

  it("lowercases voice editor input only inside a sentence", () => {
    const result = transformTranscription(
      "НОВОЕ СЛОВО.",
      undefined,
      editorContext("middle"),
    );

    assert.equal(result.text, "новое слово");
    assert.equal(
      result.appliedRules.some(
        (rule) =>
          rule.id ===
            TRANSFORMATION_RULE_IDS.editorLowercaseInsideSentence &&
          rule.outcome === "changed",
      ),
      true,
    );
    assert.equal(
      result.appliedRules.some(
        (rule) => rule.id === TRANSFORMATION_RULE_IDS.lowercase,
      ),
      false,
    );
  });

  it("preserves voice editor casing at the start of a sentence", () => {
    const result = transformTranscription(
      "НОВОЕ СЛОВО.",
      undefined,
      editorContext("start"),
    );

    assert.equal(result.text, "НОВОЕ СЛОВО");
    assert.deepEqual(
      result.appliedRules.find(
        (rule) =>
          rule.id === TRANSFORMATION_RULE_IDS.editorLowercaseInsideSentence,
      ),
      {
        id: TRANSFORMATION_RULE_IDS.editorLowercaseInsideSentence,
        version: 1,
        outcome: "skipped",
        changed: false,
        reason: "sentence-start",
      },
    );
  });

  it("skips contextual casing when editor context is unknown", () => {
    const result = transformTranscription(
      "НОВОЕ СЛОВО",
      undefined,
      editorContext("unknown"),
    );
    assert.equal(result.text, "НОВОЕ СЛОВО");
    assert.equal(
      result.appliedRules.find(
        (rule) =>
          rule.id === TRANSFORMATION_RULE_IDS.editorLowercaseInsideSentence,
      )?.reason,
      "context-unknown",
    );
  });
});

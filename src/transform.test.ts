import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { transformTranscription } from "./transform.js";

describe("transformTranscription", () => {
  it("lowercases text and removes its final period", () => {
    assert.equal(transformTranscription("HELLO World."), "hello world");
  });

  it("keeps periods that are not at the end of the input", () => {
    assert.equal(
      transformTranscription("FIRST SENTENCE. SECOND SENTENCE."),
      "first sentence. second sentence",
    );
  });

  it("preserves trailing whitespace", () => {
    assert.equal(transformTranscription("HELLO.  \n"), "hello  \n");
  });

  it("does not remove other final punctuation", () => {
    assert.equal(transformTranscription("IS THIS OK?"), "is this ok?");
  });

  it("normalizes simple Russian numerals as part of the transformation", () => {
    assert.equal(
      transformTranscription("МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА."),
      "мне нужно 22 файла",
    );
  });

  it("normalizes inflected numbers with scales", () => {
    assert.equal(
      transformTranscription("НЕ ХВАТАЕТ ДВУХ ТЫСЯЧ ДВАДЦАТИ ДВУХ ФАЙЛОВ."),
      "не хватает 2022 файлов",
    );
  });

  it("corrects a conservative numeral typo", () => {
    assert.equal(
      transformTranscription("МНЕ НУЖНО ДВАЦАТЬ ДВА ФАЙЛА."),
      "мне нужно 22 файла",
    );
  });
});

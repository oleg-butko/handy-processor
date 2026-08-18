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
});

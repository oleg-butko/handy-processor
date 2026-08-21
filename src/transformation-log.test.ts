import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { TransformationLog } from "./transformation-log.js";

describe("TransformationLog", () => {
  it("returns newest transformations first with pagination", () => {
    const log = new TransformationLog(":memory:");
    try {
      log.add("FIRST", "first");
      log.add("SECOND", "second");
      log.add("THIRD", "third");

      const firstPage = log.list(1, 2);
      assert.equal(firstPage.total, 3);
      assert.deepEqual(
        firstPage.items.map((item) => item.originalText),
        ["THIRD", "SECOND"],
      );

      const secondPage = log.list(2, 2);
      assert.deepEqual(
        secondPage.items.map((item) => item.originalText),
        ["FIRST"],
      );
    } finally {
      log.close();
    }
  });
});

import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import { TRANSFORMATION_RULE_IDS } from "./transformation-rules.js";
import { TransformationLog } from "./transformation-log.js";

const lowercaseApplied = [
  {
    id: TRANSFORMATION_RULE_IDS.lowercase,
    version: 1,
    outcome: "changed",
    changed: true,
  },
] as const;

describe("TransformationLog", () => {
  it("returns newest transformations first with pagination", () => {
    const log = new TransformationLog(":memory:");
    try {
      log.add("FIRST", "first", lowercaseApplied);
      log.add("SECOND", "second", lowercaseApplied);
      log.add("THIRD", "third", lowercaseApplied);

      const firstPage = log.list(1, 2);
      assert.equal(firstPage.total, 3);
      assert.deepEqual(
        firstPage.items.map((item) => item.originalText),
        ["THIRD", "SECOND"],
      );
      assert.deepEqual(firstPage.items[0]?.appliedRules, lowercaseApplied);

      const secondPage = log.list(2, 2);
      assert.deepEqual(
        secondPage.items.map((item) => item.originalText),
        ["FIRST"],
      );
    } finally {
      log.close();
    }
  });

  it("migrates version 1 history and marks old rule data as unknown", () => {
    const directory = mkdtempSync(join(tmpdir(), "handy-processor-log-"));
    const databasePath = join(directory, "history.sqlite3");
    const legacyDatabase = new DatabaseSync(databasePath);
    legacyDatabase.exec(`
      CREATE TABLE transformations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        original_text TEXT NOT NULL,
        transformed_text TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      INSERT INTO transformations (original_text, transformed_text)
      VALUES ('OLD', 'old');
      PRAGMA user_version = 1;
    `);
    legacyDatabase.close();

    const log = new TransformationLog(databasePath);
    try {
      assert.equal(log.list(1, 10).items[0]?.appliedRules, null);
      log.add("NEW", "new", lowercaseApplied);
      assert.deepEqual(log.list(1, 10).items[0]?.appliedRules, lowercaseApplied);
    } finally {
      log.close();
      rmSync(directory, { recursive: true, force: true });
    }
  });
});

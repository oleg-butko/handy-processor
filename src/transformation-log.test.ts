import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import {
  EXTERNAL_TRANSFORMATION_CONTEXT,
  TRANSFORMATION_SOURCES,
  type VoiceEditorTransformationContext,
} from "./transformation-context.js";
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
      log.add("FIRST", "first", lowercaseApplied, EXTERNAL_TRANSFORMATION_CONTEXT);
      log.add("SECOND", "second", lowercaseApplied, EXTERNAL_TRANSFORMATION_CONTEXT);
      log.add("THIRD", "third", lowercaseApplied, EXTERNAL_TRANSFORMATION_CONTEXT);

      const firstPage = log.list(1, 2);
      assert.equal(firstPage.total, 3);
      assert.deepEqual(
        firstPage.items.map((item) => item.originalText),
        ["THIRD", "SECOND"],
      );
      assert.deepEqual(firstPage.items[0]?.appliedRules, lowercaseApplied);
      assert.equal(firstPage.items[0]?.source, TRANSFORMATION_SOURCES.external);
      assert.equal(firstPage.items[0]?.context, null);

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
      log.add("NEW", "new", lowercaseApplied, EXTERNAL_TRANSFORMATION_CONTEXT);
      assert.deepEqual(log.list(1, 10).items[0]?.appliedRules, lowercaseApplied);
    } finally {
      log.close();
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("stores a compact voice editor context snapshot", () => {
    const log = new TransformationLog(":memory:");
    const context: VoiceEditorTransformationContext = {
      source: TRANSFORMATION_SOURCES.voiceEditor,
      editor: {
        sessionId: "session-1",
        documentRevision: 7,
        selectionStart: 12,
        selectionEnd: 12,
        sentencePosition: "middle",
      },
    };

    try {
      log.add("НОВЫЙ", "новый", lowercaseApplied, context);
      const item = log.list(1, 10).items[0];
      assert.equal(item?.source, TRANSFORMATION_SOURCES.voiceEditor);
      assert.deepEqual(item?.context, {
        schemaVersion: 1,
        editor: { sentencePosition: "middle" },
      });
    } finally {
      log.close();
    }
  });

  it("reads version 2 rule snapshots after migrating the database", () => {
    const directory = mkdtempSync(join(tmpdir(), "handy-processor-log-v2-"));
    const databasePath = join(directory, "history.sqlite3");
    const legacyDatabase = new DatabaseSync(databasePath);
    legacyDatabase.exec(`
      CREATE TABLE transformations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        original_text TEXT NOT NULL,
        transformed_text TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        ),
        rules_snapshot TEXT
      ) STRICT;
      CREATE TABLE rule_settings (
        rule_id TEXT PRIMARY KEY,
        enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
        updated_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      INSERT INTO transformations (
        original_text,
        transformed_text,
        rules_snapshot
      ) VALUES (
        'OLD',
        'old',
        '{"pipelineVersion":1,"applied":[{"id":"case.lowercase","version":1,"changed":true}]}'
      );
      PRAGMA user_version = 2;
    `);
    legacyDatabase.close();

    const log = new TransformationLog(databasePath);
    try {
      const item = log.list(1, 10).items[0];
      assert.equal(item?.source, TRANSFORMATION_SOURCES.external);
      assert.deepEqual(item?.appliedRules, lowercaseApplied);
    } finally {
      log.close();
      rmSync(directory, { recursive: true, force: true });
    }
  });
});

import { DatabaseSync, type StatementSync } from "node:sqlite";
import type { StoredRuleSetting } from "./rule-settings.js";
import type {
  AppliedTransformationRule,
  TransformationRuleId,
} from "./transformation-rules.js";

export interface TransformationRecord {
  readonly id: number;
  readonly originalText: string;
  readonly transformedText: string;
  readonly createdAt: string;
  readonly appliedRules: readonly AppliedTransformationRule[] | null;
}

export interface TransformationPage {
  readonly items: readonly TransformationRecord[];
  readonly total: number;
}

interface TransformationRow {
  readonly id: number;
  readonly original_text: string;
  readonly transformed_text: string;
  readonly created_at: string;
  readonly rules_snapshot: string | null;
}

interface RuleSettingRow {
  readonly rule_id: string;
  readonly enabled: number;
}

interface StoredRuleSnapshot {
  readonly pipelineVersion: 1;
  readonly applied: readonly AppliedTransformationRule[];
}

const SCHEMA_VERSION = 2;

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseRuleSnapshot(
  value: string | null,
): readonly AppliedTransformationRule[] | null {
  if (value === null) {
    return null;
  }

  const snapshot: unknown = JSON.parse(value);
  if (
    !isObject(snapshot) ||
    snapshot.pipelineVersion !== 1 ||
    !Array.isArray(snapshot.applied)
  ) {
    throw new Error("Transformation contains an invalid rule snapshot");
  }

  for (const rule of snapshot.applied) {
    if (
      !isObject(rule) ||
      typeof rule.id !== "string" ||
      !Number.isSafeInteger(rule.version) ||
      typeof rule.changed !== "boolean"
    ) {
      throw new Error("Transformation contains an invalid applied rule");
    }
  }

  return snapshot.applied as unknown as readonly AppliedTransformationRule[];
}

function serializeRuleSnapshot(
  appliedRules: readonly AppliedTransformationRule[],
): string {
  const snapshot: StoredRuleSnapshot = {
    pipelineVersion: 1,
    applied: appliedRules,
  };
  return JSON.stringify(snapshot);
}

function migrate(database: DatabaseSync): void {
  const versionRow = database.prepare("PRAGMA user_version").get() as
    | { user_version: number }
    | undefined;
  const version = versionRow?.user_version ?? 0;

  if (version > SCHEMA_VERSION) {
    throw new Error(
      `Database schema version ${version} is newer than supported version ${SCHEMA_VERSION}`,
    );
  }

  if (version < 1) {
    database.exec(`
      BEGIN;
      CREATE TABLE IF NOT EXISTS transformations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        original_text TEXT NOT NULL,
        transformed_text TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      PRAGMA user_version = 1;
      COMMIT;
    `);
  }

  if (version < 2) {
    database.exec(`
      BEGIN;
      ALTER TABLE transformations ADD COLUMN rules_snapshot TEXT;
      CREATE TABLE rule_settings (
        rule_id TEXT PRIMARY KEY,
        enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
        updated_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      PRAGMA user_version = 2;
      COMMIT;
    `);
  }
}

export class TransformationLog {
  readonly #database: DatabaseSync;
  readonly #insert: StatementSync;
  readonly #count: StatementSync;
  readonly #list: StatementSync;
  readonly #listRuleSettings: StatementSync;
  readonly #setRuleEnabled: StatementSync;

  constructor(databasePath: string) {
    this.#database = new DatabaseSync(databasePath, { timeout: 5_000 });
    this.#database.exec(`
      PRAGMA journal_mode = DELETE;
      PRAGMA synchronous = NORMAL;
    `);
    migrate(this.#database);

    this.#insert = this.#database.prepare(`
      INSERT INTO transformations (
        original_text,
        transformed_text,
        rules_snapshot
      ) VALUES (?, ?, ?)
    `);
    this.#count = this.#database.prepare(
      "SELECT COUNT(*) AS total FROM transformations",
    );
    this.#list = this.#database.prepare(`
      SELECT id, original_text, transformed_text, created_at, rules_snapshot
      FROM transformations
      ORDER BY id DESC
      LIMIT ? OFFSET ?
    `);
    this.#listRuleSettings = this.#database.prepare(`
      SELECT rule_id, enabled
      FROM rule_settings
    `);
    this.#setRuleEnabled = this.#database.prepare(`
      INSERT INTO rule_settings (rule_id, enabled)
      VALUES (?, ?)
      ON CONFLICT (rule_id) DO UPDATE SET
        enabled = excluded.enabled,
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    `);
  }

  add(
    originalText: string,
    transformedText: string,
    appliedRules: readonly AppliedTransformationRule[],
  ): void {
    this.#insert.run(
      originalText,
      transformedText,
      serializeRuleSnapshot(appliedRules),
    );
  }

  list(page: number, pageSize: number): TransformationPage {
    const countRow = this.#count.get() as { total: number } | undefined;
    const rows = this.#list.all(pageSize, (page - 1) * pageSize) as unknown as
      TransformationRow[];

    return {
      total: countRow?.total ?? 0,
      items: rows.map((row) => ({
        id: row.id,
        originalText: row.original_text,
        transformedText: row.transformed_text,
        createdAt: row.created_at,
        appliedRules: parseRuleSnapshot(row.rules_snapshot),
      })),
    };
  }

  listRuleSettings(): readonly StoredRuleSetting[] {
    const rows = this.#listRuleSettings.all() as unknown as RuleSettingRow[];
    return rows.map((row) => ({
      ruleId: row.rule_id,
      enabled: row.enabled === 1,
    }));
  }

  setRuleEnabled(ruleId: TransformationRuleId, enabled: boolean): void {
    this.#setRuleEnabled.run(ruleId, enabled ? 1 : 0);
  }

  close(): void {
    this.#database.close();
  }
}

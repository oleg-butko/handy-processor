import { DatabaseSync, type StatementSync } from "node:sqlite";
import type { StoredRuleSetting } from "./rule-settings.js";
import {
  TRANSFORMATION_SOURCES,
  toStoredTransformationContext,
  type StoredTransformationContext,
  type TransformationContext,
  type TransformationSource,
} from "./transformation-context.js";
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
  readonly source: TransformationSource;
  readonly context: StoredTransformationContext | null;
}

export interface TransformationPage {
  readonly items: readonly TransformationRecord[];
  readonly total: number;
}

export interface TransformationTextPair {
  readonly originalText: string;
  readonly transformedText: string;
}

export const DEFAULT_TRANSFORMATION_HISTORY_LIMIT = 50_000;
export const MAX_TRANSFORMATION_HISTORY_LIMIT = 1_000_000;

export interface EditorDraft {
  readonly content: string;
  readonly state: unknown | null;
  readonly currentNoteId: number | null;
  readonly revision: number;
  readonly updatedAt: string | null;
}

export interface EditorNoteSummary {
  readonly id: number;
  readonly name: string;
  readonly updatedAt: string;
}

export interface EditorNote extends EditorNoteSummary {
  readonly content: string;
  readonly state: unknown | null;
  readonly revision: number;
  readonly createdAt: string;
}

interface TransformationRow {
  readonly id: number;
  readonly original_text: string;
  readonly transformed_text: string;
  readonly created_at: string;
  readonly rules_snapshot: string | null;
  readonly source: string;
  readonly context_snapshot: string | null;
}

interface TransformationTextPairRow {
  readonly original_text: string;
  readonly transformed_text: string;
}

interface RuleSettingRow {
  readonly rule_id: string;
  readonly enabled: number;
}

interface EditorDraftRow {
  readonly content: string;
  readonly state_snapshot: string | null;
  readonly current_note_id: number | null;
  readonly revision: number;
  readonly updated_at: string;
}

interface EditorNoteRow {
  readonly id: number;
  readonly name: string;
  readonly content: string;
  readonly state_snapshot: string | null;
  readonly revision: number;
  readonly created_at: string;
  readonly updated_at: string;
}

interface StoredRuleSnapshot {
  readonly pipelineVersion: 2;
  readonly applied: readonly AppliedTransformationRule[];
}

const SCHEMA_VERSION = 6;
const TRANSFORMATION_HISTORY_LIMIT_KEY = "transformation_history_limit";

function validateTransformationHistoryLimit(value: number): void {
  if (
    !Number.isSafeInteger(value) ||
    value < 1 ||
    value > MAX_TRANSFORMATION_HISTORY_LIMIT
  ) {
    throw new RangeError(
      `Transformation history limit must be an integer from 1 to ${MAX_TRANSFORMATION_HISTORY_LIMIT}`,
    );
  }
}

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
  if (!isObject(snapshot) || !Array.isArray(snapshot.applied)) {
    throw new Error("Transformation contains an invalid rule snapshot");
  }

  if (snapshot.pipelineVersion === 1) {
    return snapshot.applied.map((rule) => {
      if (
        !isObject(rule) ||
        typeof rule.id !== "string" ||
        !Number.isSafeInteger(rule.version) ||
        typeof rule.changed !== "boolean"
      ) {
        throw new Error("Transformation contains an invalid applied rule");
      }

      return {
        id: rule.id as TransformationRuleId,
        version: rule.version as number,
        outcome: rule.changed ? "changed" : "unchanged",
        changed: rule.changed,
      };
    });
  }

  if (snapshot.pipelineVersion !== 2) {
    throw new Error("Transformation contains an invalid rule snapshot");
  }

  for (const rule of snapshot.applied) {
    if (
      !isObject(rule) ||
      typeof rule.id !== "string" ||
      !Number.isSafeInteger(rule.version) ||
      !["changed", "unchanged", "skipped"].includes(
        String(rule.outcome),
      ) ||
      (rule.reason !== undefined && typeof rule.reason !== "string") ||
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
    pipelineVersion: 2,
    applied: appliedRules,
  };
  return JSON.stringify(snapshot);
}

function parseSource(value: string): TransformationSource {
  if (
    value !== TRANSFORMATION_SOURCES.external &&
    value !== TRANSFORMATION_SOURCES.voiceEditor
  ) {
    throw new Error("Transformation contains an invalid source");
  }
  return value;
}

function parseContextSnapshot(
  value: string | null,
): StoredTransformationContext | null {
  if (value === null) {
    return null;
  }

  const snapshot: unknown = JSON.parse(value);
  if (
    !isObject(snapshot) ||
    snapshot.schemaVersion !== 1 ||
    !isObject(snapshot.editor) ||
    !["start", "middle", "unknown"].includes(
      String(snapshot.editor.sentencePosition),
    )
  ) {
    throw new Error("Transformation contains an invalid context snapshot");
  }

  return snapshot as unknown as StoredTransformationContext;
}

function parseEditorState(value: string | null): unknown | null {
  return value === null ? null : (JSON.parse(value) as unknown);
}

function serializeEditorState(value: unknown | null): string | null {
  return value === null ? null : JSON.stringify(value);
}

function toEditorNote(row: EditorNoteRow): EditorNote {
  return {
    id: row.id,
    name: row.name,
    content: row.content,
    state: parseEditorState(row.state_snapshot),
    revision: row.revision,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
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

  if (version < 3) {
    database.exec(`
      BEGIN;
      ALTER TABLE transformations ADD COLUMN source TEXT NOT NULL
        DEFAULT 'external'
        CHECK (source IN ('external', 'voice-editor'));
      ALTER TABLE transformations ADD COLUMN context_snapshot TEXT;
      PRAGMA user_version = 3;
      COMMIT;
    `);
  }

  if (version < 4) {
    database.exec(`
      BEGIN;
      CREATE TABLE editor_draft (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        content TEXT NOT NULL,
        revision INTEGER NOT NULL CHECK (revision >= 1),
        updated_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      PRAGMA user_version = 4;
      COMMIT;
    `);
  }


  if (version < 5) {
    database.exec(`
      BEGIN;
      CREATE TABLE editor_notes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL CHECK (
          length(trim(name)) BETWEEN 1 AND 100
        ),
        content TEXT NOT NULL,
        state_snapshot TEXT,
        revision INTEGER NOT NULL CHECK (revision >= 1),
        created_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        ),
        updated_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      ALTER TABLE editor_draft ADD COLUMN state_snapshot TEXT;
      ALTER TABLE editor_draft ADD COLUMN current_note_id INTEGER;
      PRAGMA user_version = 5;
      COMMIT;
    `);
  }

  if (version < 6) {
    database.exec(`
      BEGIN;
      CREATE TABLE app_settings (
        setting_key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (
          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
      ) STRICT;
      PRAGMA user_version = 6;
      COMMIT;
    `);
  }
}

export class TransformationLog {
  readonly #database: DatabaseSync;
  readonly #insert: StatementSync;
  readonly #count: StatementSync;
  readonly #list: StatementSync;
  readonly #listTextPairs: StatementSync;
  readonly #getAppSetting: StatementSync;
  readonly #setAppSetting: StatementSync;
  readonly #pruneTransformations: StatementSync;
  readonly #listRuleSettings: StatementSync;
  readonly #setRuleEnabled: StatementSync;
  readonly #getEditorDraft: StatementSync;
  readonly #saveEditorDraft: StatementSync;
  readonly #listEditorNotes: StatementSync;
  readonly #getEditorNote: StatementSync;
  readonly #createEditorNote: StatementSync;
  readonly #saveEditorNote: StatementSync;
  readonly #renameEditorNote: StatementSync;
  readonly #deleteEditorNote: StatementSync;
  readonly #clearDeletedDraftNote: StatementSync;

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
        rules_snapshot,
        source,
        context_snapshot
      ) VALUES (?, ?, ?, ?, ?)
    `);
    this.#count = this.#database.prepare(
      "SELECT COUNT(*) AS total FROM transformations",
    );
    this.#list = this.#database.prepare(`
      SELECT id, original_text, transformed_text, created_at, rules_snapshot,
        source, context_snapshot
      FROM transformations
      ORDER BY id DESC
      LIMIT ? OFFSET ?
    `);
    this.#listTextPairs = this.#database.prepare(`
      SELECT original_text, transformed_text
      FROM transformations
      ORDER BY id DESC
    `);
    this.#getAppSetting = this.#database.prepare(`
      SELECT value
      FROM app_settings
      WHERE setting_key = ?
    `);
    this.#setAppSetting = this.#database.prepare(`
      INSERT INTO app_settings (setting_key, value)
      VALUES (?, ?)
      ON CONFLICT (setting_key) DO UPDATE SET
        value = excluded.value,
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    `);
    this.#pruneTransformations = this.#database.prepare(`
      DELETE FROM transformations
      WHERE id <= COALESCE((
        SELECT id
        FROM transformations
        ORDER BY id DESC
        LIMIT 1 OFFSET ?
      ), 0)
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
    this.#getEditorDraft = this.#database.prepare(`
      SELECT content, state_snapshot, current_note_id, revision, updated_at
      FROM editor_draft
      WHERE id = 1
    `);
    this.#saveEditorDraft = this.#database.prepare(`
      INSERT INTO editor_draft (
        id, content, state_snapshot, current_note_id, revision
      ) VALUES (1, ?, ?, ?, 1)
      ON CONFLICT (id) DO UPDATE SET
        content = excluded.content,
        state_snapshot = excluded.state_snapshot,
        current_note_id = excluded.current_note_id,
        revision = editor_draft.revision + 1,
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      RETURNING content, state_snapshot, current_note_id, revision, updated_at
    `);
    this.#listEditorNotes = this.#database.prepare(`
      SELECT id, name, updated_at
      FROM editor_notes
      ORDER BY updated_at DESC, id DESC
    `);
    this.#getEditorNote = this.#database.prepare(`
      SELECT id, name, content, state_snapshot, revision, created_at, updated_at
      FROM editor_notes
      WHERE id = ?
    `);
    this.#createEditorNote = this.#database.prepare(`
      INSERT INTO editor_notes (name, content, state_snapshot, revision)
      VALUES (?, ?, ?, 1)
      RETURNING id, name, content, state_snapshot, revision, created_at, updated_at
    `);
    this.#saveEditorNote = this.#database.prepare(`
      UPDATE editor_notes SET
        content = ?,
        state_snapshot = ?,
        revision = revision + 1,
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      WHERE id = ?
      RETURNING id, name, content, state_snapshot, revision, created_at, updated_at
    `);
    this.#renameEditorNote = this.#database.prepare(`
      UPDATE editor_notes SET
        name = ?,
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      WHERE id = ?
      RETURNING id, name, content, state_snapshot, revision, created_at, updated_at
    `);
    this.#deleteEditorNote = this.#database.prepare(`
      DELETE FROM editor_notes
      WHERE id = ?
    `);
    this.#clearDeletedDraftNote = this.#database.prepare(`
      UPDATE editor_draft SET current_note_id = NULL
      WHERE current_note_id = ?
    `);
  }

  add(
    originalText: string,
    transformedText: string,
    appliedRules: readonly AppliedTransformationRule[],
    context: TransformationContext,
  ): void {
    const storedContext = toStoredTransformationContext(context);
    this.#database.exec("BEGIN IMMEDIATE");
    try {
      this.#insert.run(
        originalText,
        transformedText,
        serializeRuleSnapshot(appliedRules),
        context.source,
        storedContext === null ? null : JSON.stringify(storedContext),
      );
      this.#pruneTransformations.run(this.getTransformationHistoryLimit());
      this.#database.exec("COMMIT");
    } catch (error) {
      this.#database.exec("ROLLBACK");
      throw error;
    }
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
        source: parseSource(row.source),
        context: parseContextSnapshot(row.context_snapshot),
      })),
    };
  }

  *iterateTextPairs(): IterableIterator<TransformationTextPair> {
    const rows = this.#listTextPairs.iterate() as Iterable<TransformationTextPairRow>;
    for (const row of rows) {
      yield {
        originalText: row.original_text,
        transformedText: row.transformed_text,
      };
    }
  }

  getTransformationHistoryLimit(): number {
    const row = this.#getAppSetting.get(TRANSFORMATION_HISTORY_LIMIT_KEY) as
      | { value: string }
      | undefined;
    if (row === undefined) {
      return DEFAULT_TRANSFORMATION_HISTORY_LIMIT;
    }

    const value = Number(row.value);
    validateTransformationHistoryLimit(value);
    return value;
  }

  setTransformationHistoryLimit(value: number): void {
    validateTransformationHistoryLimit(value);
    this.#database.exec("BEGIN IMMEDIATE");
    try {
      this.#setAppSetting.run(TRANSFORMATION_HISTORY_LIMIT_KEY, String(value));
      this.#pruneTransformations.run(value);
      this.#database.exec("COMMIT");
    } catch (error) {
      this.#database.exec("ROLLBACK");
      throw error;
    }
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

  loadEditorDraft(): EditorDraft {
    const row = this.#getEditorDraft.get() as EditorDraftRow | undefined;
    return row === undefined
      ? {
          content: "",
          state: null,
          currentNoteId: null,
          revision: 0,
          updatedAt: null,
        }
      : {
          content: row.content,
          state: parseEditorState(row.state_snapshot),
          currentNoteId: row.current_note_id,
          revision: row.revision,
          updatedAt: row.updated_at,
        };
  }

  saveEditorDraft(
    content: string,
    state: unknown | null,
    currentNoteId: number | null,
  ): EditorDraft {
    const row = this.#saveEditorDraft.get(
      content,
      serializeEditorState(state),
      currentNoteId,
    ) as
      | EditorDraftRow
      | undefined;
    if (row === undefined) {
      throw new Error("Failed to save editor draft");
    }
    return {
      content: row.content,
      state: parseEditorState(row.state_snapshot),
      currentNoteId: row.current_note_id,
      revision: row.revision,
      updatedAt: row.updated_at,
    };
  }

  listEditorNotes(): readonly EditorNoteSummary[] {
    const rows = this.#listEditorNotes.all() as unknown as Array<
      Pick<EditorNoteRow, "id" | "name" | "updated_at">
    >;
    return rows.map((row) => ({
      id: row.id,
      name: row.name,
      updatedAt: row.updated_at,
    }));
  }

  loadEditorNote(id: number): EditorNote | null {
    const row = this.#getEditorNote.get(id) as EditorNoteRow | undefined;
    return row === undefined ? null : toEditorNote(row);
  }

  createEditorNote(
    name: string,
    content: string,
    state: unknown | null,
  ): EditorNote {
    const row = this.#createEditorNote.get(
      name,
      content,
      serializeEditorState(state),
    ) as EditorNoteRow | undefined;
    if (row === undefined) {
      throw new Error("Failed to create editor note");
    }
    return toEditorNote(row);
  }

  saveEditorNote(
    id: number,
    content: string,
    state: unknown | null,
  ): EditorNote | null {
    const row = this.#saveEditorNote.get(
      content,
      serializeEditorState(state),
      id,
    ) as EditorNoteRow | undefined;
    return row === undefined ? null : toEditorNote(row);
  }

  renameEditorNote(id: number, name: string): EditorNote | null {
    const row = this.#renameEditorNote.get(name, id) as
      | EditorNoteRow
      | undefined;
    return row === undefined ? null : toEditorNote(row);
  }

  deleteEditorNote(id: number): boolean {
    this.#database.exec("BEGIN IMMEDIATE");
    try {
      const result = this.#deleteEditorNote.run(id);
      if (result.changes === 0) {
        this.#database.exec("ROLLBACK");
        return false;
      }
      this.#clearDeletedDraftNote.run(id);
      this.#database.exec("COMMIT");
      return true;
    } catch (error) {
      this.#database.exec("ROLLBACK");
      throw error;
    }
  }

  close(): void {
    this.#database.close();
  }
}

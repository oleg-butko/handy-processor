import { DatabaseSync, type StatementSync } from "node:sqlite";

export interface TransformationRecord {
  readonly id: number;
  readonly originalText: string;
  readonly transformedText: string;
  readonly createdAt: string;
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
}

const SCHEMA_VERSION = 1;

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

  if (version === 0) {
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
}

export class TransformationLog {
  readonly #database: DatabaseSync;
  readonly #insert: StatementSync;
  readonly #count: StatementSync;
  readonly #list: StatementSync;

  constructor(databasePath: string) {
    this.#database = new DatabaseSync(databasePath, { timeout: 5_000 });
    this.#database.exec(`
      PRAGMA journal_mode = DELETE;
      PRAGMA synchronous = NORMAL;
    `);
    migrate(this.#database);

    this.#insert = this.#database.prepare(`
      INSERT INTO transformations (original_text, transformed_text)
      VALUES (?, ?)
    `);
    this.#count = this.#database.prepare(
      "SELECT COUNT(*) AS total FROM transformations",
    );
    this.#list = this.#database.prepare(`
      SELECT id, original_text, transformed_text, created_at
      FROM transformations
      ORDER BY id DESC
      LIMIT ? OFFSET ?
    `);
  }

  add(originalText: string, transformedText: string): void {
    this.#insert.run(originalText, transformedText);
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
      })),
    };
  }

  close(): void {
    this.#database.close();
  }
}

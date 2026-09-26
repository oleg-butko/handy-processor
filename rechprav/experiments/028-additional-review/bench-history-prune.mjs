import { DatabaseSync } from "node:sqlite";

const LIMIT = 50_000;
const ITERATIONS = 1_000;

function measure(mode) {
  const db = new DatabaseSync(":memory:");
  try {
    db.exec("CREATE TABLE transformations (id INTEGER PRIMARY KEY AUTOINCREMENT, text TEXT); BEGIN");
    const insert = db.prepare("INSERT INTO transformations (text) VALUES ('сняли слот')");
    for (let index = 0; index < LIMIT; index++) insert.run();
    db.exec("COMMIT");

    const count = db.prepare("SELECT COUNT(*) AS total FROM transformations");
    const oldPrune = db.prepare(`DELETE FROM transformations WHERE id <= COALESCE((
      SELECT id FROM transformations ORDER BY id DESC LIMIT 1 OFFSET ?), 0)`);
    const newPrune = db.prepare(`DELETE FROM transformations WHERE id IN (
      SELECT id FROM transformations ORDER BY id ASC LIMIT ?)`);
    const start = performance.now();
    for (let index = 0; index < ITERATIONS; index++) {
      db.exec("BEGIN");
      insert.run();
      if (mode === "old") {
        oldPrune.run(LIMIT);
      } else {
        const excess = count.get().total - LIMIT;
        if (excess > 0) newPrune.run(excess);
      }
      db.exec("COMMIT");
    }
    return { ms: Number((performance.now() - start).toFixed(2)), finalCount: count.get().total };
  } finally {
    db.close();
  }
}

console.log(JSON.stringify({ node: process.version, limit: LIMIT, iterations: ITERATIONS,
  old: measure("old"), new: measure("new") }, null, 2));

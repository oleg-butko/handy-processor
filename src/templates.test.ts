import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { it } from "node:test";
import { filterTemplates, parseTemplates, templateMatches, templatePrefixes } from "./template-model.js";
import { TemplateFile } from "./templates.js";
import { TransformationLog } from "./transformation-log.js";

it("parses BOM and CRLF, preserving source line numbers and text", () => {
  const result = parseTemplates("\uFEFFПервый > Второй > Текст  с пробелами\r\n\r\nневерно\r\nа > > в\r\nа > б > в > г");
  assert.deepEqual(result.items, [{ line: 1, levels: ["Первый", "Второй", "Текст  с пробелами"] }]);
  assert.deepEqual(result.diagnostics.map((item) => item.line), [3, 4, 5]);
});

it("matches every prefix at word starts across only selected levels", () => {
  const { items } = parseTemplates("Сложные задачи > другие варианты > готовый ответ\nслон > дружба > Ёлка, слово\nмасло > подряд > ничего");
  assert.deepEqual(filterTemplates(items, "СЛ.др.", [0, 1]).map((row) => row.line), [1, 2]);
  assert.equal(filterTemplates(items, "сл.др", [2]).length, 0);
  assert.deepEqual(filterTemplates(items, "ел.сл", [2]).map((row) => row.line), [2]);
  assert.equal(filterTemplates(items, "", [2]).length, 3);
  assert.deepEqual(templateMatches("Ёлка, слово; слон", templatePrefixes("ел.сл.сло")), [
    { from: 0, to: 2 }, { from: 6, to: 9 }, { from: 13, to: 16 },
  ]);
  assert.equal(filterTemplates(items, "[.*", [0, 1, 2]).length, 0);
});

it("retains the last file on read failure and persists validated selection", async () => {
  const directory = await mkdtemp(join(tmpdir(), "handy-templates-"));
  const database = join(directory, "settings.sqlite3");
  let storage = new TransformationLog(database);
  try {
    const source = new TemplateFile(directory, storage);
    const defaultPath = join(directory, "templates.txt");
    assert.equal((await source.load()).items.length, 0);
    await writeFile(defaultPath, "а > б > исходный");
    assert.equal((await source.load()).items[0]?.levels[2], "исходный");
    await writeFile(defaultPath, "частичная запись");
    const stale = await source.load();
    assert.equal(stale.stale, true);
    assert.equal(stale.items[0]?.levels[2], "исходный");
    await assert.rejects(source.select(join(directory, "missing.txt")));
    assert.equal(storage.getTemplatesPath(), null);
    const chosen = join(directory, "chosen.txt");
    await writeFile(chosen, "а > б > выбранный");
    await source.select(chosen);
    storage.close();
    storage = new TransformationLog(database);
    assert.equal((await new TemplateFile(directory, storage).load()).items[0]?.levels[2], "выбранный");
    await writeFile(chosen, Buffer.from([0xff, 0xfe]));
    await assert.rejects(source.select(chosen));
    await writeFile(chosen, "x".repeat(2 * 1024 * 1024 + 1));
    await assert.rejects(source.select(chosen), /2 МБ/u);
    await assert.rejects(source.select("relative.txt"), /абсолютный/u);
  } finally {
    storage.close();
    await rm(directory, { recursive: true, force: true });
  }
});

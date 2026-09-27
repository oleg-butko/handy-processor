import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

const root = path.dirname(fileURLToPath(import.meta.url));

test("history import, fuzzy variants, annotation and project restore", async () => {
  const server = await createServer({ root, server: { host: "127.0.0.1", port: 0 } });
  await server.listen();
  const address = server.httpServer.address();
  const browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined),
    headless: true,
    chromiumSandbox: true,
  });
  const directory = await mkdtemp(path.join(tmpdir(), "rechprav-annotator-test-"));
  try {
    const page = await browser.newPage({ acceptDownloads: true });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(`http://127.0.0.1:${address.port}/`);
    await page.locator("#history-file").setInputFiles(path.join(root, "demo-history.jsonl"));
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent?.includes("5 записей"));
    assert.match(await page.locator("#groups").textContent(), /сняли свод.*2 записей/u);

    await page.locator("#query").fill("сняли свод");
    await page.locator("#distance").selectOption("2");
    await page.locator("#search-form button[type=submit]").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent?.includes("3 найдено в 2 вариантах"));
    assert.match(await page.locator("#groups").textContent(), /сняли слот/u);

    await page.locator("#gold").fill("сняли слот");
    await page.locator("#mark-edited").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent?.includes("1 ошибок подтверждено"));
    await page.locator("#origin-kind").selectOption("synthetic");
    await page.locator("#group-id").fill("slot-case");
    page.on("dialog", (dialog) => dialog.accept());
    await page.locator("#apply-group").click();
    assert.match(await page.locator("#analysis-summary").textContent(), /1 проверено: 1 ошибок/u);
    await page.locator("#status-filter").selectOption("reviewed");
    await page.locator("#origin-filter").selectOption("synthetic");
    await page.locator("#search-form button[type=submit]").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent?.includes("1 найдено"));
    await page.locator("#status-filter").selectOption("all");
    await page.locator("#origin-filter").selectOption("all");
    await page.locator("#search-form button[type=submit]").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent?.includes("3 найдено"));
    const downloadPromise = page.waitForEvent("download");
    await page.locator("#save-project").click();
    const download = await downloadPromise;
    const saved = path.join(directory, download.suggestedFilename());
    await download.saveAs(saved);
    const rows = (await readFile(saved, "utf8")).trim().split("\n").map(JSON.parse);
    assert.equal(rows.length, 5);
    assert.equal(rows[0].schema_version, 2);
    assert.equal(rows[0].annotation.gold, "сняли слот");
    assert.equal(rows[0].annotation.originKind, "synthetic");
    assert.equal(rows[0].annotation.groupId, "slot-case");
    assert.equal(rows[1].annotation.gold, null);
    assert.equal(rows[1].annotation.groupId, "slot-case");

    await page.locator("#project-file").setInputFiles(saved);
    await page.waitForFunction(() => document.querySelector("#file-name")?.textContent?.includes("rechprav-annotation-"));
    assert.match(await page.locator("#summary").textContent(), /1 проверено/u);
    assert.equal(await page.locator("#gold").inputValue(), "сняли слот");
    assert.equal(await page.locator("#group-id").inputValue(), "slot-case");

    await page.evaluate(() => {
      const original = File.prototype.text;
      File.prototype.text = function () {
        if (this.name !== "slow.jsonl") return original.call(this);
        return new Promise((resolve, reject) => setTimeout(() => original.call(this).then(resolve, reject), 300));
      };
    });
    const record = (id) => JSON.stringify({ id, originalText: "пример", transformedText: "пример", source: "external" });
    await page.locator("#history-file").setInputFiles({ name: "slow.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(record(10)) });
    await page.locator("#history-file").setInputFiles({ name: "fast.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(record(11)) });
    await page.waitForFunction(() => document.querySelector("#file-name")?.textContent?.includes("fast.jsonl"));
    await page.waitForTimeout(400);
    assert.match(await page.locator("#file-name").textContent(), /fast\.jsonl/u);

    await page.locator("#history-file").setInputFiles({ name: "empty.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from("\n") });
    await page.getByText("JSONL-файл не содержит записей").waitFor();
    assert.match(await page.locator("#file-name").textContent(), /fast\.jsonl/u);
    assert.deepEqual(errors, []);
  } finally {
    await browser.close();
    await server.close();
    await rm(directory, { recursive: true, force: true });
  }
});

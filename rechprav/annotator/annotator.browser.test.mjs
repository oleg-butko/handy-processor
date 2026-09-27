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
    const downloadPromise = page.waitForEvent("download");
    await page.locator("#save-project").click();
    const download = await downloadPromise;
    const saved = path.join(directory, download.suggestedFilename());
    await download.saveAs(saved);
    const rows = (await readFile(saved, "utf8")).trim().split("\n").map(JSON.parse);
    assert.equal(rows.length, 5);
    assert.equal(rows[0].annotation.gold, "сняли слот");

    await page.locator("#project-file").setInputFiles(saved);
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent?.includes("1 проверено"));
    assert.equal(await page.locator("#gold").inputValue(), "сняли слот");
    assert.deepEqual(errors, []);
  } finally {
    await browser.close();
    await server.close();
    await rm(directory, { recursive: true, force: true });
  }
});

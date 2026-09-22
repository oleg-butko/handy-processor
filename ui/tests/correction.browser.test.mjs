import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import { test } from 'node:test';
import { chromium } from 'playwright';
import { createRequestHandler } from '../../dist/app.js';
import { CorrectionWorker } from '../../dist/correction-worker.js';
import { RuleSettings } from '../../dist/rule-settings.js';
import { TransformationLog } from '../../dist/transformation-log.js';
import { EditorSessionRegistry } from '../../dist/editor-sessions.js';
import { loadWebUiAssets } from '../../dist/web-ui-assets.js';

test('real correction toggle, persisted candidate explanations, reload and mobile layout', async t => {
  const directory = mkdtempSync(join(tmpdir(), 'handy-correction-browser-'));
  const log = new TransformationLog(join(directory, 'history.sqlite'));
  writeFileSync(join(directory, 'dictionary.txt'), 'свод\nсняли\nслот\nслота\nбез\nснятия\nправил\n');
  writeFileSync(join(directory, 'dictionary.toml'), 'dictionary = "dictionary.txt"\n');
  writeFileSync(join(directory, 'terms.toml'), readFileSync('rechprav/handy-terms.toml', 'utf8').replace('"handy-dictionary.toml"', '"dictionary.toml"'));
  const worker = new CorrectionWorker({ executable: resolve('rechprav/target/release/examples', process.platform === 'win32' ? 'correction_worker.exe' : 'correction_worker'), config: join(directory, 'terms.toml') });
  const handler = createRequestHandler({ correctionWorker: worker, ruleSettings: new RuleSettings(log),
    transformationHistory: log, editorDraftStorage: log, editorSessions: new EditorSessionRegistry(),
    diagnosticLog: { path: 'unused', async appendClientError() {} }, webUiAssets: loadWebUiAssets(resolve('web-dist')) });
  const server = createServer((req, res) => { void handler(req, res); });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    worker.close();
    await new Promise(resolve => { server.closeAllConnections(); server.close(resolve); });
    log.close(); rmSync(directory, { recursive: true, force: true });
  });
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH ?? (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined), chromiumSandbox: true });
  t.after(() => browser.close());
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const base = `http://127.0.0.1:${server.address().port}`;
  const submit = async text => {
    const response = await fetch(`${base}/v1/chat/completions`, { method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ messages: [{ role: 'user', content: text }] }) });
    assert.equal(response.status, 200);
    return (await response.json()).choices[0].message.content;
  };
  await page.goto(base);
  await page.getByRole('tab', { name: 'Список правил', exact: true }).click();
  const toggle = page.getByRole('checkbox', { name: 'Включить правило «Коррекция терминов и фраз»' });
  assert.equal(await toggle.isChecked(), false);
  assert.equal(await submit('без снятия сваты'), 'без снятия сваты');
  await toggle.check();
  await page.getByRole('checkbox', { name: 'Выключить правило «Коррекция терминов и фраз»' }).waitFor();
  assert.equal(await submit('без снятия сваты'), 'без снятия слота');
  assert.equal(await submit('сняли свод правил'), 'сняли свод правил');
  await page.getByRole('tab', { name: 'История', exact: true }).click();
  const details = page.locator('.correction-details');
  await details.first().waitFor();
  assert.equal(await details.count(), 2);
  await details.first().locator(':scope > summary').click();
  const rejected = details.first().locator('.correction-decision').filter({ hasText: '«свод»' }).first();
  await rejected.locator(':scope > summary').click();
  assert.match(await rejected.textContent(), /Сработало контекстное исключение/);
  await details.nth(1).locator(':scope > summary').click();
  const chosen = details.nth(1).locator('.correction-decision').first();
  await chosen.locator(':scope > summary').click();
  assert.match(await chosen.textContent(), /«сваты» → «слота»/);
  await chosen.getByText('Из чего сложилась оценка', { exact: true }).first().click();
  assert.match(await chosen.textContent(), /Вес термина/);
  assert.match(await chosen.textContent(), /Бонус контекста/);
  assert.match(await chosen.textContent(), /без снятия/);
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
  assert.equal(await details.nth(1).evaluate(element => element.getBoundingClientRect().right <= innerWidth), true);
  assert.equal(await details.nth(1).locator('.correction-candidates-scroll').first().evaluate(element =>
    element.clientWidth < element.scrollWidth && element.getBoundingClientRect().right <= innerWidth), true);
  if (process.env.CORRECTION_SCREENSHOT) await page.screenshot({ path: process.env.CORRECTION_SCREENSHOT, fullPage: true });
  await page.getByRole('tab', { name: 'Список правил', exact: true }).click();
  await page.locator('.correction-settings > summary').click();
  await page.getByRole('button', { name: 'Загрузить профиль заново' }).click();
  await page.getByText('Профиль загружен', { exact: true }).waitFor();
  await page.getByRole('checkbox', { name: 'Выключить правило «Коррекция терминов и фраз»' }).uncheck();
  await page.getByRole('checkbox', { name: 'Включить правило «Коррекция терминов и фраз»' }).waitFor();
  assert.equal(await submit('без снятия сваты'), 'без снятия сваты');
  assert.deepEqual(errors, []);
});

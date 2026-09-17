import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { test } from 'node:test';
import { chromium } from 'playwright';

// Run after build:ui. Uses an isolated HTTP fixture, never the user's database.
test('templates filter, alphabet, hidden third-level copy and native activation', async (t) => {
  const items = [
    { line: 1, levels: ['Сложные задачи', 'Другие варианты', 'Готовый ответ'] },
    { line: 2, levels: ['Общение', 'Приветствие', 'Добрый день!'] },
  ];
  const server = createServer(async (req, res) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;
    const data = pathname === '/api/templates' ? { path: '/test/templates.txt', items, diagnostics: [], error: null, stale: false }
      : pathname === '/api/editor/draft' ? { content: '', state: null, currentNoteId: null, revision: 0 }
      : pathname === '/api/transformations' ? { items: [], pagination: { total: 0, totalPages: 0 } }
      : pathname === '/api/settings/history' ? { maxEntries: 50000 }
      : { items: [] };
    if (pathname.startsWith('/api/')) {
      res.setHeader('content-type', 'application/json'); res.end(JSON.stringify(data)); return;
    }
    try {
      const file = pathname === '/' ? 'index.html' : pathname.slice(1);
      const content = await readFile(new URL(`../../web-dist/${file}`, import.meta.url));
      res.setHeader('content-type', file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : 'text/html');
      res.end(content);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  const executablePath = process.env.CHROMIUM_PATH ?? (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);
  const browser = await chromium.launch({ executablePath, chromiumSandbox: true });
  t.after(() => browser.close());
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    window.__copied = '';
    window.__TAURI__ = {
      core: { invoke: async (command, args) => {
        if (command === 'templates_desktop_state') return { supported: true, settings: { enabled: true, key: 'Win', intervalMs: 200 }, error: null, canPaste: true, activation: null };
        if (command === 'copy_template') window.__copied = args.text;
      } },
      event: { listen: async (_, handler) => { window.__activate = handler; return () => {}; } },
    };
  });
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  assert.deepEqual(await page.getByRole('tab').allTextContents(), ['Голосовой редактор', 'История', 'Список правил', 'Шаблоны', 'Настройки']);
  await page.getByRole('tab', { name: 'Шаблоны', exact: true }).click();
  await page.locator('.template-row').first().waitFor();
  const filter = page.getByRole('textbox', { name: 'Фильтр', exact: true });
  await filter.fill('сл.др');
  assert.equal(await page.locator('.template-row').count(), 1);
  assert.deepEqual(await page.locator('mark').allTextContents(), ['Сл', 'Др']);
  await page.getByRole('button', { name: 'Уровень 3', exact: true }).click();
  assert.equal(await page.locator('[data-level="3"]').count(), 0);
  await page.getByRole('button', { name: 'Скопировать строку 1', exact: true }).click();
  await page.waitForFunction(() => window.__copied === 'Готовый ответ');
  await page.getByRole('button', { name: 'Уровень 2', exact: true }).click();
  assert.equal(await page.locator('.template-row').count(), 0);
  await page.getByRole('button', { name: 'Очистить', exact: true }).click();
  for (const letter of ['с', 'л']) await page.getByRole('button', { name: `Буква ${letter}`, exact: true }).click();
  assert.equal(await filter.inputValue(), 'сл');
  assert.equal(await page.locator('.template-row').count(), 1);
  await page.getByRole('tab', { name: 'История', exact: true }).click();
  await page.evaluate(() => window.__activate({ payload: 1 }));
  await page.waitForFunction(() => document.querySelector('[role="tab"][aria-selected="true"]')?.textContent === 'Шаблоны');
  assert.equal(await filter.inputValue(), 'сл');
  assert.deepEqual(errors, []);
});

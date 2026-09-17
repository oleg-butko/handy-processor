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
  const diagnostics = [];
  let predefinedWords = ['Сложные', 'Другие'];
  const server = createServer(async (req, res) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;
    if (pathname === '/api/diagnostics/client-error') {
      let body = '';
      for await (const chunk of req) body += chunk;
      diagnostics.push(JSON.parse(body));
      res.writeHead(201, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ logPath: '/test/handy-processor.log' }));
      return;
    }
    const data = pathname === '/api/templates' ? { path: '/test/templates.txt', items, predefinedWords, predefinedWordsError: null, diagnostics: [], error: null, stale: false }
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
    window.__pasteCalls = [];
    window.__settings = { enabled: true, key: 'Win', intervalMs: 200 };
    const state = () => ({ supported: true, settings: window.__settings, error: null, canPaste: true, activation: null });
    window.__TAURI__ = {
      core: { invoke: async (command, args) => {
        if (command === 'templates_desktop_state') return state();
        if (command === 'record_template_key') return 'ControlRight';
        if (command === 'set_template_hotkey') { window.__settings = args.settings; return state(); }
        if (command === 'ack_templates_activation') window.__acked = args.id;
        if (command === 'copy_template') window.__copied = args.text;
        if (command === 'paste_template') {
          window.__pasteCalls.push(args.text);
          return new Promise(resolve => { window.__finishPaste = () => resolve({ pasted: false, message: 'Скопировано; вставьте вручную. Фокус изменился' }); });
        }
      } },
      event: { listen: async (_, handler) => { window.__activate = handler; return () => {}; } },
    };
  });
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  assert.deepEqual(await page.getByRole('tab').allTextContents(), ['Голосовой редактор', 'История', 'Список правил', 'Шаблоны', 'Настройки']);
  await page.getByRole('tab', { name: 'Шаблоны', exact: true }).click();
  await page.locator('.template-row').first().waitFor();
  const filter = page.getByRole('textbox', { name: 'Фильтр', exact: true });
  await t.test('compact hotkey settings and predefined words with refresh', async () => {
    assert.equal(await page.getByRole('heading', { name: 'Шаблоны', exact: true }).count(), 0);
    const interval = page.getByRole('textbox', { name: 'Интервал двойного нажатия', exact: true });
    assert.equal(await interval.isVisible(), false);
    await page.locator('.templates-hotkey summary').click();
    assert.equal(await interval.isVisible(), true);
    await page.locator('.templates-hotkey summary').click();
    assert.equal(await interval.isVisible(), false);
    const words = page.getByRole('group', { name: 'Готовые слова', exact: true });
    await words.getByRole('button', { name: 'Сложные', exact: true }).click();
    assert.equal(await filter.inputValue(), 'Сложные.');
    assert.equal(await page.locator('.template-row').count(), 1);
    await words.getByRole('button', { name: 'Другие', exact: true }).click();
    assert.equal(await filter.inputValue(), 'Сложные.Другие.');
    assert.equal(await page.locator('.template-row').count(), 1);
    await filter.fill('сл');
    await words.getByRole('button', { name: 'Другие', exact: true }).click();
    assert.equal(await filter.inputValue(), 'сл.Другие.');
    predefinedWords = ['Общение'];
    await page.getByRole('button', { name: 'Обновить', exact: true }).click();
    await words.getByRole('button', { name: 'Общение', exact: true }).waitFor();
    assert.equal(await words.getByRole('button', { name: 'Другие', exact: true }).count(), 0);
    await filter.fill('');
    await words.getByRole('button', { name: 'Общение', exact: true }).click();
    assert.equal(await filter.inputValue(), 'Общение.');
    assert.equal(await page.locator('.template-row').count(), 1);
    assert.match(await page.locator('.template-row').textContent(), /Добрый день!/);
    await page.getByRole('button', { name: 'Очистить', exact: true }).click();
  });
  await filter.fill('сл.др');
  assert.equal(await page.locator('.template-row').count(), 1);
  assert.deepEqual(await page.locator('mark').allTextContents(), ['Сл', 'Др']);
  await page.getByRole('button', { name: 'Уровень 3', exact: true }).click();
  assert.equal(await page.locator('[data-level="3"]').count(), 0);
  await page.getByRole('button', { name: 'Скопировать строку 1', exact: true }).click();
  await page.waitForFunction(() => window.__copied === 'Готовый ответ');
  const paste = page.getByRole('button', { name: 'Вставить строку 1', exact: true });
  await paste.click();
  await page.waitForFunction(() => window.__pasteCalls.length === 1);
  assert.equal(await paste.isDisabled(), true);
  assert.deepEqual(await page.evaluate(() => window.__pasteCalls), ['Готовый ответ']);
  await page.evaluate(() => window.__finishPaste());
  await page.getByText('Скопировано; вставьте вручную. Фокус изменился', { exact: true }).waitFor();
  await page.locator('.templates-hotkey summary').click();
  await page.getByRole('button', { name: 'Win × 2', exact: true }).click();
  await page.getByRole('button', { name: 'Правый Ctrl × 2', exact: true }).waitFor();
  await page.getByRole('textbox', { name: 'Интервал двойного нажатия', exact: true }).fill('350');
  await page.getByRole('button', { name: 'Сохранить', exact: true }).click();
  await page.waitForFunction(() => window.__settings.key === 'ControlRight' && window.__settings.intervalMs === 350);
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
  await page.waitForFunction(() => window.__acked === 1);
  if (process.env.TEMPLATES_SCREENSHOT) {
    await page.locator('.templates-hotkey summary').click();
    await page.screenshot({ path: process.env.TEMPLATES_SCREENSHOT, fullPage: true });
    await page.locator('.templates-hotkey summary').click();
  }
  await t.test('delayed native state cannot reset edited or saved hotkey settings', async () => {
    const interval = page.getByRole('textbox', { name: 'Интервал двойного нажатия', exact: true });
    for (const save of [false, true]) {
      await page.evaluate(() => {
        const invoke = window.__TAURI__.core.invoke;
        window.__restoreInvoke = () => { window.__TAURI__.core.invoke = invoke; };
        window.__finishState = null;
        window.__TAURI__.core.invoke = async (command, args) => {
          const result = await invoke(command, args);
          if (command === 'templates_desktop_state') return new Promise(resolve => { window.__finishState = () => resolve(result); });
          return result;
        };
      });
      try {
        await page.getByRole('tab', { name: 'История', exact: true }).click();
        await page.getByRole('tab', { name: 'Шаблоны', exact: true }).click();
        await page.waitForFunction(() => typeof window.__finishState === 'function');
        await interval.fill(save ? '550' : '450');
        if (save) {
          await page.getByRole('button', { name: 'Сохранить', exact: true }).click();
          await page.waitForFunction(() => window.__settings.intervalMs === 550);
        }
        await page.evaluate(() => window.__finishState());
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.equal(await interval.inputValue(), save ? '550' : '450');
        if (save) assert.match(await page.locator('.templates-hotkey summary').textContent(), /550 мс/);
      } finally {
        await page.evaluate(() => { window.__finishState?.(); window.__restoreInvoke(); });
      }
    }
  });
  await t.test('recording stops when the window loses focus or unloads', async () => {
    await page.evaluate(() => {
      const invoke = window.__TAURI__.core.invoke;
      window.__cancelledRecordings = 0;
      window.__TAURI__.core.invoke = (command, args) => {
        if (command === 'record_template_key') return new Promise((_, reject) => { window.__rejectRecording = reject; });
        if (command === 'cancel_template_recording') {
          window.__cancelledRecordings++;
          window.__rejectRecording?.('Запись отменена');
          return Promise.resolve();
        }
        return invoke(command, args);
      };
    });
    for (const event of ['blur', 'pagehide']) {
      await page.getByRole('button', { name: 'Правый Ctrl × 2', exact: true }).click();
      await page.getByRole('button', { name: 'Отмена записи', exact: true }).waitFor();
      const previous = await page.evaluate(() => window.__cancelledRecordings);
      await page.evaluate(event => window.dispatchEvent(new Event(event)), event);
      await page.waitForFunction(previous => window.__cancelledRecordings > previous, previous, { timeout: 2000 });
      await page.getByRole('button', { name: 'Правый Ctrl × 2', exact: true }).waitFor();
    }
  });
  await t.test('changing the source cannot be overwritten by an older tab reload', async () => {
    const fresh = { path: '/test/new.txt', items: [{ line: 1, levels: ['Новая категория', 'Новая тема', 'Новый ответ'] }], diagnostics: [], error: null, stale: false };
    const pendingReads = [];
    await page.route('**/api/templates', async route => {
      if (route.request().method() === 'PUT') await route.fulfill({ json: fresh });
      else pendingReads.push(route);
    });
    await page.evaluate(() => {
      const invoke = window.__TAURI__.core.invoke;
      window.__stateReads = 0;
      window.__TAURI__.core.invoke = (command, args) => {
        if (command === 'choose_templates_file') return new Promise(resolve => { window.__chooseFile = () => resolve('/test/new.txt'); });
        if (command === 'templates_desktop_state') window.__stateReads++;
        return invoke(command, args);
      };
    });
    await page.getByRole('button', { name: 'Очистить', exact: true }).click();
    await page.getByRole('button', { name: 'Выбрать файл', exact: true }).click();
    await page.waitForFunction(() => typeof window.__chooseFile === 'function');
    await page.getByRole('tab', { name: 'История', exact: true }).click();
    await page.getByRole('tab', { name: 'Шаблоны', exact: true }).click();
    await page.waitForFunction(() => window.__stateReads > 0);
    await page.evaluate(() => window.__chooseFile());
    await page.getByText('/test/new.txt', { exact: true }).waitFor();
    const completed = pendingReads.map(route => page.waitForEvent('requestfinished', request => request === route.request()));
    await Promise.all(pendingReads.map(route => route.fulfill({ json: { path: '/test/templates.txt', items, diagnostics: [], error: null, stale: false } })));
    await Promise.all(completed);
    // Wait for the React render caused by the now-finished stale responses.
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.locator('.templates-path').textContent(), '/test/new.txt');
    await page.getByRole('button', { name: 'Скопировать строку 1', exact: true }).click();
    await page.waitForFunction(() => window.__copied === 'Новый ответ');
    await page.unroute('**/api/templates');
  });
  await t.test('native ACL errors are logged and show the log path', async () => {
    await page.evaluate(() => {
      const invoke = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (command, args) => {
        if (command === 'templates_desktop_state' || command === 'save_history_export') {
          return Promise.reject(`Command ${command} not allowed by ACL`);
        }
        return invoke(command, args);
      };
    });
    await page.getByRole('tab', { name: 'История', exact: true }).click();
    await page.getByRole('tab', { name: 'Шаблоны', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: 'templates_desktop_state not allowed by ACL' }).filter({ hasText: '/test/handy-processor.log' }).waitFor();
    const entry = diagnostics.find(entry => entry.message.includes('templates_desktop_state not allowed by ACL'));
    assert.equal(entry?.event, 'templates');
    assert.equal(entry?.launcherBridgeDetected, true);
    assert.equal(entry?.origin, `http://127.0.0.1:${server.address().port}`);
    assert.equal('args' in entry, false);
    await page.getByRole('tab', { name: 'Настройки', exact: true }).click();
    await page.getByRole('button', { name: 'Скачать текстовый файл', exact: true }).click();
    await page.getByText('Не удалось экспортировать историю. Подробности записаны в /test/handy-processor.log', { exact: true }).waitFor();
    assert.equal(diagnostics.at(-1)?.event, 'history-export');
    assert.match(diagnostics.at(-1)?.message, /save_history_export not allowed by ACL/);
    await page.route('**/api/diagnostics/client-error', route => route.fulfill({ status: 500, body: 'Log unavailable' }));
    await page.getByRole('tab', { name: 'Шаблоны', exact: true }).click();
    const error = page.getByRole('alert').filter({ hasText: 'templates_desktop_state not allowed by ACL' });
    await error.waitFor();
    assert.equal((await error.textContent()).includes('Подробности записаны'), false);
    await page.unroute('**/api/diagnostics/client-error');
  });
  assert.deepEqual(errors, []);
});

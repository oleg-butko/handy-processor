# Web UI и локальное хранилище

## Выбранная архитектура

Интерфейс сделан как отдельная SPA на React, Mantine и Vite. Next.js здесь не
нужен: сервер уже предоставляет HTTP API, SSR и серверные React-компоненты не
дают преимуществ локальной странице истории.

```text
Handy
  |
  | POST /v1/chat/completions
  v
Node HTTP server -> transformer -> SQLite
       |                          |
       | GET /api/*              |
       v                          v
React + Mantine + CodeMirror SPA
       ├───────────────► editor context in memory
       └───────────────► history, draft, notes and rule settings
```

Backend, хранилище и UI разделены контрактами. Следующие страницы смогут
добавлять собственные `/api/*` endpoints, не меняя OpenAI-compatible API для
Handy.

## Файлы рядом с executable

При запуске SEA runtime-каталогом считается директория самого executable:

```text
handy-processor.exe
config.json
handy-processor.sqlite3
```

`config.json` содержит порт:

```json
{
  "port": 11434
}
```

Если файла нет, сервер создаёт его с портом `11434`. SQLite-база также
создаётся автоматически. Директория executable поэтому должна быть доступна
для записи.

При обычном запуске через Node.js runtime-каталогом является текущая рабочая
директория. Для тестов его можно переопределить переменной
`HANDY_PROCESSOR_RUNTIME_DIR`.

## SQLite

Сервер использует встроенный модуль `node:sqlite`, поэтому для SEA не нужен
нативный npm-модуль или отдельная DLL. Таблица `transformations` хранит:

- исходный текст из последнего `user`-сообщения;
- результат трансформации;
- UTC-время создания;
- автоинкрементный идентификатор.
- JSON-снимок правил, запущенных для этой трансформации.
- источник `external` или `voice-editor`;
- компактный JSON-снимок контекста редактора.

Таблица `rule_settings` хранит пользовательские переключатели правил. Значения
применяются без перезапуска сервера. Каталог, порядок, описания и значения по
умолчанию остаются в реестре TypeScript, поэтому добавление правила не требует
изменения схемы БД.

Таблица `editor_draft` хранит автоматически сохраняемое рабочее состояние:
текст, снимок CodeMirror с undo/redo, выбранную заметку и монотонную ревизию.
Таблица `editor_notes` хранит именованные заметки, их текст, снимок undo/redo,
время создания, время изменения и отдельную ревизию. Таблица `app_settings`
хранит расширяемые серверные настройки, сейчас — максимальное количество
записей истории.

Запись создаётся только для успешно обработанного запроса
`POST /v1/chat/completions`.
Схема имеет версию через SQLite `PRAGMA user_version`, чтобы следующие функции
могли добавлять последовательные миграции без пересоздания пользовательской
базы.

## API истории

```http
GET /api/transformations?page=1&pageSize=10
```

- `page` начинается с `1`;
- `pageSize` может быть от `1` до `100`;
- записи сортируются от новых к старым.

Ответ:

```json
{
  "items": [
    {
      "id": 42,
      "originalText": "МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.",
      "transformedText": "мне нужно 22 файла",
      "createdAt": "2026-08-21T10:54:12.883Z",
      "source": "external",
      "context": null,
      "appliedRules": [
        {
          "id": "case.lowercase",
          "version": 1,
          "outcome": "changed",
          "changed": true
        },
        {
          "id": "numbers.ru-cardinals-to-digits",
          "version": 1,
          "outcome": "changed",
          "changed": true
        },
        {
          "id": "punctuation.remove-final-period",
          "version": 1,
          "outcome": "changed",
          "changed": true
        }
      ]
    }
  ],
  "pagination": {
    "page": 1,
    "pageSize": 10,
    "total": 42,
    "totalPages": 5
  }
}
```

Для записей, созданных до появления правил, `appliedRules` равен `null`.
Старые записи после миграции получают источник `external` и `context: null`.

## Лимит и экспорт истории

По умолчанию SQLite хранит последние 50 000 преобразований. Значение можно
изменить на вкладке **Настройки** в диапазоне от 1 до 1 000 000:

```http
GET /api/settings/history
PATCH /api/settings/history
Content-Type: application/json

{ "maxEntries": 50000 }
```

Уменьшение лимита сразу и без возможности восстановления удаляет самые старые
записи. После каждого нового преобразования сервер снова проверяет лимит.

Экспериментальный текстовый экспорт доступен через:

```http
GET /api/transformations/export
```

Записи идут от новых к старым. Экспорт содержит только исходный и
преобразованный текст. Внутри записи используется строка
`--- РЕЗУЛЬТАТ ---`, между записями — `==============================`.
Контекст, время и снимок правил в файл не попадают. Если пользовательский текст
сам содержит эти строки, первая версия формата их не экранирует.

Способ сохранения зависит от оболочки интерфейса:

- в обычном браузере используется стандартная загрузка браузера;
- в окне Tauri WebView интерфейс вызывает native-команду launcher, и файл
  сохраняется в portable-каталог рядом с `HandyProcessor.exe`, `config.json` и
  SQLite-базой.

Launcher получает экспорт с локального HTTP endpoint и сам записывает его на
диск. Если файл с сегодняшней датой уже существует, к имени добавляется
суффикс `-2`, `-3` и так далее; существующий экспорт не перезаписывается.

## API правил

```http
GET /api/rules
PATCH /api/rules/:ruleId
```

`GET` возвращает метаданные, пример и текущее состояние каждого правила.
`PATCH` принимает JSON вида `{ "enabled": false }`. Подробнее см.
[описание правил трансформации](transformation-rules.md).

## API голосового редактора

```http
GET /api/editor/draft
PUT /api/editor/draft
GET /api/editor/notes
POST /api/editor/notes
GET /api/editor/notes/:noteId
PUT /api/editor/notes/:noteId
PATCH /api/editor/notes/:noteId
DELETE /api/editor/notes/:noteId
PUT /api/editor/sessions/:sessionId
DELETE /api/editor/sessions/:sessionId
```

Рабочий документ и именованные заметки вместе с undo/redo хранятся в SQLite.
Активные контекстные сессии и позиция курсора хранятся только в памяти и
автоматически истекают. Подробный контракт и пользовательский сценарий описаны в
[документации голосового редактора](voice-editor.md).

## Разработка UI

Обычный запуск пересобирает UI перед стартом backend:

```bash
npm run dev
```

Отдельные команды:

```bash
npm run build:ui
npm run dev:ui
npm run typecheck:ui
```

Production build создаёт `web-dist`. Перед SEA-сборкой скрипт
`scripts/create-sea-config.mjs` находит все файлы в этой директории и добавляет
их в сгенерированную конфигурацию assets. Поэтому новые chunks, изображения и
шрифты будут автоматически встроены в executable. Во время работы рядом с EXE
нужны только изменяемые `config.json` и SQLite-база; папка `web-dist` не нужна.

## Безопасность

Логина и пароля нет. Сервер всегда слушает только `127.0.0.1`; порт меняется
через конфиг. Это намеренное ограничение: без аутентификации его не следует
публиковать в локальную сеть или интернет.

## Источники

- [Node.js: встроенный SQLite](https://nodejs.org/api/sqlite.html)
- [Node.js: SEA assets](https://nodejs.org/api/single-executable-applications.html#assets)
- [Mantine: Vite рекомендуется для SPA](https://mantine.dev/getting-started/?g=vite)
- [Vite: production build](https://vite.dev/guide/build.html)

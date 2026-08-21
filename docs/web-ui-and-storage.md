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
       | GET /api/transformations|
       v                          v
React + Mantine SPA <-------- paginated history
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
      "createdAt": "2026-08-21T10:54:12.883Z"
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

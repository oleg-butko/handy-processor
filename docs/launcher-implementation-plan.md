# План опционального Tauri-лаунчера

## Цель

Добавить компактный Windows desktop shell, не лишая сервер возможности
работать самостоятельно:

```text
standalone:
  handy-processor-server.exe -> localhost API + browser UI

desktop:
  HandyProcessor.exe -> tray + WebView2 window
                    -> managed handy-processor-server.exe
```

Лаунчер отвечает только за native UI и жизненный цикл дочернего процесса.
Трансформация, OpenAI-compatible API, React/Mantine UI, конфигурация и SQLite
остаются в Node SEA.

## Структура репозитория

Существующие файлы сервера остаются на месте. Лаунчер добавляется как
изолированная часть того же репозитория:

```text
handy-processor/
├── src/                         # Node server
├── ui/                          # React/Mantine UI
├── scripts/                     # общие build scripts
├── launcher/
│   ├── README.md
│   └── src-tauri/
│       ├── Cargo.toml
│       ├── build.rs
│       ├── tauri.conf.json
│       ├── capabilities/
│       ├── icons/
│       ├── binaries/            # generated sidecars, ignored by Git
│       └── src/
└── docs/
```

## Runtime-контракт

### Обнаружение сервера

Лаунчер читает тот же `config.json`, вычисляет localhost URL и вызывает
`GET /health`. Ответ содержит стабильную идентичность сервиса и версию:

```json
{
  "status": "ok",
  "service": "handy-processor",
  "version": "0.1.0",
  "pid": 1234
}
```

Если совместимый сервер уже работает, лаунчер подключается к нему и не создаёт
второй процесс. Сервер в этом случае считается внешним и не завершается вместе
с лаунчером.

### Managed-процесс

Если сервер отсутствует, лаунчер запускает соседний SEA с аргументом
`--managed`, скрытым console window и явно заданным
`HANDY_PROCESSOR_RUNTIME_DIR`.

В managed-режиме сервер:

- принимает строку `shutdown` через stdin;
- корректно закрывает HTTP listener и SQLite;
- также завершается при EOF stdin, чтобы не оставаться orphan-процессом после
  аварийного закрытия лаунчера;
- сохраняет прежнее поведение без `--managed`.

При штатном выходе лаунчер сначала запрашивает graceful shutdown, ждёт
ограниченное время и только затем применяет принудительное завершение.

### Окно и tray

- Tauri window создаётся скрытым и загружает UI с localhost.
- Левый клик по tray выполняет `unminimize`, `show`, `set_focus`.
- Правый клик открывает native menu.
- Закрытие окна и, на Windows, сворачивание скрывают его в tray.
- Только пункт `Выход` завершает лаунчер.
- Single-instance предотвращает несколько tray icon и повторный запуск
  показывает существующее окно.

Начальное меню:

- Открыть Handy Processor;
- Открыть в браузере;
- Скопировать API URL;
- Открыть папку данных;
- Перезапустить управляемый сервер;
- Выход.

## Артефакты

Portable-каталог должен содержать минимальный набор:

```text
HandyProcessor.exe
handy-processor-server.exe
config.json
handy-processor.sqlite3          # появляется после первого запуска
```

React assets остаются встроенными в SEA. Иконки лаунчера по возможности
компилируются в Tauri executable. Staging-копии sidecar и build output не
коммитятся.

## Этапы и коммиты

- [x] Этап 0 — сохранить архитектуру и план.
  Коммит: `docs: plan optional Tauri launcher`.
- [x] Этап 1 — добавить идентифицируемый health response, управляемый stdin
  lifecycle и тесты, не меняя standalone-поведение.
  Коммит: `feat(server): add managed launcher lifecycle`.
- [x] Этап 2 — создать Tauri scaffold, staging sidecar и воспроизводимые build
  commands.
  Коммит: `build(launcher): scaffold Tauri sidecar app`.
- [x] Этап 3 — реализовать supervisor, localhost readiness, tray, окно,
  single-instance и корректное завершение.
  Коммит: `feat(launcher): manage server from system tray`.
- [ ] Этап 4 — добавить portable packaging, Windows-инструкцию и итоговые
  проверки.
  Коммит: `docs(launcher): add Windows build and usage guide`.

## Проверки

Для сервера:

- unit/integration tests на Node.js 24 и 26;
- standalone startup остаётся рабочим;
- `--managed` завершается по `shutdown` и EOF;
- повторный shutdown идемпотентен;
- SQLite закрывается до завершения процесса.

Для лаунчера:

- `cargo fmt --check`, `cargo check`, `cargo test`;
- сборка Tauri с staged sidecar;
- ручная Windows-проверка отсутствия console window;
- запуск при свободном порте и подключение к уже работающему серверу;
- close/minimize-to-tray, повторный запуск и tray Exit;
- восстановление после раннего завершения sidecar.

## Риски и ограничения

- Native Windows и WebView2 поведение окончательно проверяется в Windows build
  environment; Debian-проверка покрывает Rust logic, но не Win32 UX.
- Порт может быть занят посторонним процессом. Лаунчер обязан проверять поля
  `service` и `version`, а не только HTTP 200.
- Tauri сам по себе не предоставляет полный supervisor для долгоживущего
  sidecar. Владение процессом, readiness, crash state и shutdown timeout должны
  быть явным состоянием лаунчера.
- При установленной сборке директория executable может быть read-only.
  Portable release сохраняет данные рядом с EXE; installer в будущем должен
  явно выбирать writable data directory.

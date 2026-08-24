# Сборка Windows EXE через SEA в Node.js 26

Инструкция по сборке самостоятельного server EXE без обязательного
Tauri-лаунчера.

Актуально для `handy-processor` на 21 августа 2026 года. Проверенная версия
Node.js — `26.7.0`.

## Результат

Node.js 26 может самостоятельно создать автономный EXE командой
`node --build-sea`. В отличие от Node.js 24, не нужны:

- отдельный SEA blob в релизном процессе;
- копирование `node.exe` вручную;
- npm-пакет `postject`;
- sentinel fuse;
- удаление исходной подписи перед инъекцией.

Для ежедневной разработки SEA не нужен: сервер по-прежнему удобнее запускать
через `npm run dev`. SEA следует собирать как релизный артефакт.

React/Mantine UI встраивается в EXE как SEA assets. Изменяемые `config.json` и
`handy-processor.sqlite3` находятся рядом с EXE; база создаётся при первом
запуске.

Для фоновой работы доступен опциональный Tauri tray launcher. Полная сборка
двух EXE и их поведение описаны в [Windows tray launcher](windows-launcher.md).

Node.js 26 сейчас имеет статус Current, а не LTS. Для внутреннего применения и
тестирования это практичный вариант. Перед публичным production-релизом нужно
либо принять использование Current-ветки, либо дождаться перехода Node.js 26 в
LTS.

## Использование Node.js 26 без изменения NVM default

В текущем Debian-окружении установлены обе версии:

```text
default: 24 -> v24.19.0
Node 26:  v26.7.0
```

Запустить одну команду через Node.js 26, не меняя default:

```bash
. /home/user1/.nvm/nvm.sh
nvm exec 26 node --version
nvm exec 26 npm test
```

Переключить только текущую shell-сессию:

```bash
. /home/user1/.nvm/nvm.sh
nvm use 26
node --version
```

Вернуться к default:

```bash
nvm use default
```

Команда `nvm use 26` не меняет alias `default`. Менять default можно только
явно через `nvm alias default 26`, и для этой сборки это не требуется.

Для удобства проекта можно создать `.nvmrc` со строкой `26.7.0`. Тогда внутри
репозитория достаточно выполнить `nvm use`; глобальный default останется 24.

## Рекомендуемый вариант: сборка непосредственно в Windows

### Требования

- Windows x64;
- PowerShell;
- Node.js `26.7.0` x64 или другая зафиксированная версия Node.js 26;
- npm;
- необязательно: Windows SDK с `signtool.exe` для подписи.

Проверить активную версию:

```powershell
node --version
npm --version
node -p "process.arch"
node --help | Select-String "build-sea"
```

Ожидаются `v26.x`, `x64` и параметр `--build-sea`.

Если используется nvm-windows, перед сборкой можно переключиться на нужную
версию:

```powershell
nvm use 26.7.0
```

nvm-windows и POSIX NVM — разные проекты. У nvm-windows обычно нет аналога
`nvm exec`, поэтому после сборки при необходимости нужно выполнить
`nvm use 24.19.0`.

### 1. Установить зависимости

Для чистой установки существующих зависимостей:

```powershell
npm ci
```

React, Mantine, Vite и esbuild уже зафиксированы в `package-lock.json`.
`postject` для Node.js 26 не нужен.

### 2. Собрать приложение в один ESM-файл

Собрать React UI и единый ESM backend bundle:

```powershell
npm run bundle:sea:node26

node --check sea-build/handy-processor.mjs
```

Несмотря на поддержку ESM, bundling всё ещё необходим. SEA содержит один
основной скрипт; локальные модули и npm-пакеты не загружаются из файловой
системы автоматически. esbuild включает backend-код и сгенерированный словарь
в итоговый `.mjs`, оставляя встроенные `node:`-модули, включая `node:sqlite`,
частью runtime. Vite отдельно создаёт UI assets в `web-dist`.

### 3. Создать конфигурацию Node.js 26 SEA

Конфигурация `sea-config.node26.json` уже находится в корне проекта:

```json
{
  "main": "sea-build/handy-processor.mjs",
  "mainFormat": "module",
  "output": "sea-build/handy-processor.exe",
  "disableExperimentalSEAWarning": true,
  "useSnapshot": false,
  "useCodeCache": false,
  "execArgvExtension": "none"
}
```


Причины выбранных значений:

- `mainFormat: "module"` соответствует ESM bundle;
- `useSnapshot: false` обязателен для ESM main и упрощает cross-build;
- `useCodeCache: false` сохраняет совместимость с cross-build и динамическими
  импортами;
- `execArgvExtension: "none"` запрещает внешнему `NODE_OPTIONS` изменять
  параметры встроенного Node.js runtime. Порт читается из соседнего
  `config.json` и от `NODE_OPTIONS` не зависит.

`npm run prepare:sea:node26` создаёт
`sea-build/sea-config.node26.generated.json` и автоматически добавляет в него
все файлы из `web-dist` как SEA assets. Поэтому конфигурацию не нужно вручную
обновлять при добавлении UI chunks, изображений или шрифтов.

### 4. Создать готовый EXE одной командой

```powershell
npm run build:sea:node26
```

Результат:

```text
sea-build\handy-processor.exe
sea-build\config.json
```

Команда собирает UI и backend, создаёт EXE и копирует рядом стартовый конфиг.
`handy-processor.sqlite3` появится там же при первом запуске. Папка должна быть
доступна для записи.

### 5. Проверить EXE

```powershell
$serverProcess = Start-Process `
  -FilePath ".\sea-build\handy-processor.exe" `
  -PassThru

try {
  Start-Sleep -Seconds 1

  Invoke-RestMethod "http://127.0.0.1:11434/health"
  Invoke-WebRequest "http://127.0.0.1:11434/"

  $body = @{
    model = "handy-basic-transformer"
    messages = @(
      @{ role = "user"; content = "МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА." }
    )
    stream = $false
  } | ConvertTo-Json -Depth 4

  $response = Invoke-RestMethod `
    -Uri "http://127.0.0.1:11434/v1/chat/completions" `
    -Method Post `
    -ContentType "application/json" `
    -Body $body

  $response.choices[0].message.content
} finally {
  Stop-Process -Id $serverProcess.Id
}
```

Ожидаемый результат:

```text
мне нужно 22 файла
```

Историю можно проверить через UI по адресу `http://localhost:11434/` или API:

```powershell
Invoke-RestMethod "http://127.0.0.1:11434/api/transformations?page=1&pageSize=10"
```

После локальной проверки EXE нужно проверить на минимальной поддерживаемой
версии Windows без установленной Node.js.

### 6. Настроить Handy

При запущенном EXE:

- Provider: `Custom`;
- Base URL: `http://127.0.0.1:11434/v1`;
- API Key: пусто;
- Model: `handy-basic-transformer`;
- prompt: ровно `${output}`.

Другой порт задаётся в конфиге рядом с EXE:

```powershell
'{"port":12434}' | Set-Content .\sea-build\config.json -Encoding ascii
.\sea-build\handy-processor.exe
```

В этом случае Base URL в Handy должен быть
`http://127.0.0.1:12434/v1`.

### 7. Подписать релизный EXE

Подписание выполняется после `node --build-sea`:

```powershell
signtool sign `
  /f path\to\certificate.pfx `
  /fd SHA256 `
  /tr https://your-rfc3161-timestamp-server.example `
  /td SHA256 `
  sea-build/handy-processor.exe

signtool verify /pa /v sea-build/handy-processor.exe
```

Пароль PFX нельзя сохранять в репозитории. Неподписанный EXE работает, но
SmartScreen или антивирус могут показать предупреждение.

## npm workflow

Команды сборки уже добавлены в `package.json`.

В Windows с активной Node.js 26:

```powershell
npm run build:sea:node26
```

Собрать server SEA вместе с tray launcher в готовый portable-каталог:

```powershell
npm run install:launcher
npm run build:windows:portable
```

В Debian без изменения NVM default:

```bash
. /home/user1/.nvm/nvm.sh
nvm exec 26 npm run build:sea:node26
```

Для ежедневной разработки всё равно использовать:

```bash
npm run dev
npm test
```

## Cross-build Windows EXE из Debian

Node.js 26 упрощает и cross-build: поле `executable` указывает Node binary,
которая станет основой результата.

Порядок действий:

1. Через Node.js 26 собрать UI и единый `.mjs` командой
   `npm run bundle:sea:node26`.
2. Скачать официальный Windows x64 archive той же **точной** версии, например
   `26.7.0`.
3. Проверить SHA-256 архива по официальному `SHASUMS256.txt`.
4. Распаковать `node.exe` в build-каталог.
5. Указать этот файл в `executable`.
6. Запустить Linux Node.js той же версии с `--build-sea`.
7. Перенести результат на Windows, протестировать и подписать.

Создать конфигурацию с Windows `node.exe` и всеми UI assets:

```bash
node scripts/create-sea-config.mjs sea-build/node-v26.7.0-win-x64/node.exe
```

Сборка без переключения default:

```bash
. /home/user1/.nvm/nvm.sh
nvm exec 26 node --build-sea sea-build/sea-config.node26.generated.json
node scripts/copy-sea-runtime-files.mjs
```

Версия Linux Node.js, выполняющей команду, должна совпадать с версией целевой
Windows `node.exe`. Для cross-platform SEA `useSnapshot` и `useCodeCache`
должны оставаться выключенными.

Этот путь официально предусмотрен конфигурацией SEA, но итоговый PE-файл всё
равно необходимо проверять в Windows. Для регулярных публичных релизов Windows
VM или Windows CI runner остаётся более простым и надёжным вариантом.

## Что было проверено локально

Через установленную Node.js `26.7.0` выполнено:

```bash
nvm exec 26 npm run build:sea:node26
./sea-build/handy-processor.exe
```

Результат проверки:

- успешно создан автономный ESM Linux x64 SEA размером 142 МБ;
- executable прочитал `config.json` и создал SQLite рядом с собой;
- встроенные React/Mantine HTML, JavaScript и CSS доступны из браузера;
- `GET /health` вернул `{"status":"ok"}`;
- запрос преобразовал `МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.` в
  `мне нужно 22 файла`;
- `/api/transformations` вернул сохранённую запись;
- NVM default после сборки остался `24.19.0`.

Это подтверждает совместимость кода сервера с нативным `--build-sea`. Windows
EXE нужно отдельно собрать или cross-build-нуть и проверить в Windows.

## Источники

- [Node.js 26: Single executable applications](https://nodejs.org/api/single-executable-applications.html)
- [Node.js releases: статусы 24 LTS и 26 Current](https://nodejs.org/en/about/previous-releases)
- [esbuild: bundling API](https://esbuild.github.io/api/#bundle)
- [Microsoft SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)

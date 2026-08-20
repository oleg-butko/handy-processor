# Сборка Windows EXE через SEA в Node.js 26

Актуально для `handy-processor` на 18 августа 2026 года. Проверенная версия
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

Один раз добавить esbuild как прямую dev-зависимость:

```powershell
npm install --save-dev esbuild
```

После этого следует закоммитить изменения `package.json` и
`package-lock.json`. `postject` для Node.js 26 не нужен.

### 2. Собрать приложение в один ESM-файл

Проект уже использует ESM, а Node.js 26 SEA поддерживает
`"mainFormat": "module"`, поэтому можно сохранить этот формат:

```powershell
npx esbuild src/server.ts `
  --bundle `
  --platform=node `
  --target=node26 `
  --format=esm `
  --outfile=sea-build/handy-processor.mjs

node --check sea-build/handy-processor.mjs
```

Несмотря на поддержку ESM, bundling всё ещё необходим. SEA содержит один
основной скрипт; локальные модули и npm-пакеты не загружаются из файловой
системы автоматически. esbuild включает `app.ts` и `transform.ts` в итоговый
`.mjs`, оставляя встроенные `node:`-модули частью runtime.

Если предпочтительнее максимально консервативная сборка, можно оставить
CommonJS:

```powershell
npx esbuild src/server.ts `
  --bundle `
  --platform=node `
  --target=node26 `
  --format=cjs `
  --outfile=sea-build/handy-processor.cjs
```

Тогда в конфигурации ниже нужно указать `.cjs` и
`"mainFormat": "commonjs"`.

### 3. Создать конфигурацию Node.js 26 SEA

Создать в корне проекта `sea-config.node26.json`:

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
  параметры встроенного Node.js runtime. На `HOST` и `PORT` это не влияет:
  приложение продолжает читать их как обычные переменные окружения.

### 4. Создать готовый EXE одной командой

```powershell
node --build-sea sea-config.node26.json
```

Результат:

```text
sea-build\handy-processor.exe
```

Эта команда заменяет генерацию blob, копирование `node.exe`, удаление старой
подписи и запуск `postject` из инструкции для Node.js 24.

### 5. Проверить EXE

```powershell
$serverProcess = Start-Process `
  -FilePath ".\sea-build\handy-processor.exe" `
  -PassThru

try {
  Start-Sleep -Seconds 1

  Invoke-RestMethod "http://127.0.0.1:11434/health"

  $body = @{
    model = "handy-basic-transformer"
    messages = @(
      @{ role = "user"; content = "NODE 26 SEA TEST." }
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
node 26 sea test
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

Другой порт можно задать перед запуском:

```powershell
$env:PORT = "12434"
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

## Удобный npm workflow

После успешной ручной проверки можно добавить в `package.json`:

```json
{
  "scripts": {
    "bundle:sea:node26": "esbuild src/server.ts --bundle --platform=node --target=node26 --format=esm --outfile=sea-build/handy-processor.mjs",
    "build:sea:node26": "npm run bundle:sea:node26 && node --build-sea sea-config.node26.json"
  }
}
```

В Windows с активной Node.js 26:

```powershell
npm run build:sea:node26
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

1. Через Node.js 26 собрать единый `.mjs` или `.cjs`.
2. Скачать официальный Windows x64 archive той же **точной** версии, например
   `26.7.0`.
3. Проверить SHA-256 архива по официальному `SHASUMS256.txt`.
4. Распаковать `node.exe` в build-каталог.
5. Указать этот файл в `executable`.
6. Запустить Linux Node.js той же версии с `--build-sea`.
7. Перенести результат на Windows, протестировать и подписать.

Пример отдельной конфигурации `sea-config.node26.windows.json`:

```json
{
  "main": "sea-build/handy-processor.mjs",
  "mainFormat": "module",
  "executable": "sea-build/node-v26.7.0-win-x64/node.exe",
  "output": "sea-build/handy-processor.exe",
  "disableExperimentalSEAWarning": true,
  "useSnapshot": false,
  "useCodeCache": false,
  "execArgvExtension": "none"
}
```

Сборка без переключения default:

```bash
. /home/user1/.nvm/nvm.sh
nvm exec 26 node --build-sea sea-config.node26.windows.json
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
nvm exec 26 node --build-sea /tmp/handy-processor-sea-node26.json
nvm exec 26 node --build-sea /tmp/handy-processor-sea-node26-esm.json
```

Результат проверки:

- успешно созданы CommonJS- и ESM-варианты автономного Linux x64 SEA;
- каждый SEA имеет размер 142 МБ и запускается напрямую как executable;
- `GET /health` вернул `{"status":"ok"}`;
- запрос к ESM SEA преобразовал `NODE 26 ESM SEA TEST.` в
  `node 26 esm sea test`;
- NVM default после сборки остался `24.19.0`.

Это подтверждает совместимость кода сервера с нативным `--build-sea`. Windows
EXE нужно отдельно собрать или cross-build-нуть и проверить в Windows.

## Источники

- [Node.js 26: Single executable applications](https://nodejs.org/api/single-executable-applications.html)
- [Node.js releases: статусы 24 LTS и 26 Current](https://nodejs.org/en/about/previous-releases)
- [esbuild: bundling API](https://esbuild.github.io/api/#bundle)
- [Microsoft SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)

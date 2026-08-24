# Сборка Windows EXE через Node.js SEA

Историческая инструкция для Node.js 24.

Актуально для проекта `handy-processor` на 18 августа 2026 года.

> Эта инструкция сохранена как историческая для Node.js 24 и не включает
> текущие React UI assets, `config.json` и SQLite. Для актуальной сборки
> используйте [инструкцию Node.js 26](windows-sea-node26.md).

## Краткий вывод

Сервер можно упаковать в один `handy-processor.exe` с помощью штатного
механизма Node.js Single Executable Applications (SEA). На целевой машине не
понадобятся Node.js, `node_modules` или исходники проекта.

Текущий сервер хорошо подходит для SEA:

- в рантайме он использует только встроенные модули Node.js;
- у него нет нативных `.node`-аддонов и внешних файлов;
- конфигурация задаётся через `HOST` и `PORT`, а не через файл рядом с
  программой.

Node.js 24 поддерживает в SEA только один встроенный CommonJS-скрипт. Поэтому
`src/server.ts` вместе с импортами сначала нужно собрать в один `.cjs`-файл.
Локальная проверка такого bundle прошла успешно: esbuild создал файл размером
около 4,7 КБ, сервер запустился и преобразовал `SEA TEST.` в `sea test`.

## Рекомендуемый вариант: собирать на Windows

Это наиболее надёжный путь: Windows-версия Node.js создаёт blob, собственная
`node.exe` становится основой итогового EXE, а готовый файл сразу можно
запустить и при необходимости подписать.

### Требования

- Windows x64;
- PowerShell;
- Node.js 24 LTS x64. Проверенная версия проекта — `24.19.0`;
- npm;
- необязательно: Windows SDK с `signtool.exe` для удаления старой подписи и
  подписания готового файла.

Проверить окружение:

```powershell
node --version
npm --version
node -p "process.arch"
```

Ожидается Node.js `v24.x` и архитектура `x64`.

### 1. Установить инструменты сборки

Из корня проекта:

```powershell
npm ci
npm install --save-dev esbuild postject
```

`esbuild` нужно сохранить как прямую dev-зависимость, даже если он уже
транзитивно установлен через другой пакет. Изменения `package.json` и
`package-lock.json` после этой команды следует закоммитить, чтобы сборка была
воспроизводимой. После этого в следующих чистых сборках достаточно `npm ci`.

### 2. Собрать один CommonJS-файл

```powershell
New-Item -ItemType Directory -Force sea-build | Out-Null

npx esbuild src/server.ts `
  --bundle `
  --platform=node `
  --target=node24 `
  --format=cjs `
  --outfile=sea-build/handy-processor.cjs

node --check sea-build/handy-processor.cjs
```

Флаг `--bundle` включает `app.ts` и `transform.ts` в тот же файл. Встроенные
модули с префиксом `node:` остаются частью Node.js и не требуют отдельных
файлов.

### 3. Создать SEA-конфигурацию

Создать в корне проекта файл `sea-config.json`:

```json
{
  "main": "sea-build/handy-processor.cjs",
  "output": "sea-build/handy-processor.blob",
  "disableExperimentalSEAWarning": true,
  "useSnapshot": false,
  "useCodeCache": false
}
```

`useSnapshot` и `useCodeCache` намеренно выключены. Для этого небольшого
сервера они не нужны, а их отключение делает процедуру совместимой и с
cross-build между Linux и Windows.

### 4. Создать blob и основу EXE

```powershell
node --experimental-sea-config sea-config.json

node -e "require('node:fs').copyFileSync(process.execPath, 'sea-build/handy-processor.exe')"
```

Важно: версия Node.js, создавшая blob, должна точно совпадать с версией
`node.exe`, в которую он будет внедрён. При рекомендуемой Windows-сборке обе
команды используют одну установленную Node.js, поэтому условие выполняется
автоматически.

### 5. Удалить исходную подпись Node.js, если доступен SignTool

После изменения подписанного `node.exe` старая подпись в любом случае станет
недействительной. Удалить её до внедрения blob:

```powershell
signtool remove /s sea-build/handy-processor.exe
```

Этот шаг необязателен. Если `signtool` не установлен, `postject` может вывести
предупреждение о подписи; его можно проигнорировать для локальной сборки.

### 6. Внедрить SEA blob

```powershell
npx postject `
  sea-build/handy-processor.exe `
  NODE_SEA_BLOB `
  sea-build/handy-processor.blob `
  --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2
```

После этой команды `sea-build/handy-processor.exe` является автономным
сервером.

### 7. Проверить готовый EXE

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
      @{ role = "user"; content = "SEA TEST." }
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

Ожидаемый результат последней команды:

```text
sea test
```

Также нужно проверить EXE на той минимальной версии Windows, которую планируется
поддерживать. Наличие Node.js на тестовой машине для запуска EXE не требуется.

### 8. Настроить Handy

Пока EXE запущен, настройки Handy остаются такими же, как при запуске через
`npm`:

- Provider: `Custom`;
- Base URL: `http://127.0.0.1:11434/v1`;
- API Key: пусто;
- Model: `handy-basic-transformer`;
- содержимое prompt: ровно `${output}`.

Если нужен другой порт:

```powershell
$env:PORT = "12434"
.\sea-build\handy-processor.exe
```

Тогда в Handy нужно указать `http://127.0.0.1:12434/v1`.

## Подписание для распространения

Неподписанный EXE работает, но Windows SmartScreen или антивирус могут показать
предупреждение. Для распространения файл рекомендуется подписать **после**
внедрения SEA blob:

```powershell
signtool sign `
  /f path\to\certificate.pfx `
  /fd SHA256 `
  /tr https://your-rfc3161-timestamp-server.example `
  /td SHA256 `
  sea-build/handy-processor.exe

signtool verify /pa /v sea-build/handy-processor.exe
```

URL timestamp-сервера и способ доступа к сертификату зависят от поставщика
сертификата. Пароль PFX не следует записывать в репозиторий или командный файл.

## Можно ли собрать Windows EXE из Debian

Технически — да. Официальная документация Node.js отдельно допускает
cross-platform SEA при выполнении двух условий:

1. Linux Node.js, создающая blob, и Windows `node.exe`, в которую он внедряется,
   имеют **одинаковую точную версию**;
2. `useSnapshot` и `useCodeCache` равны `false`.

`postject` умеет внедрять ресурсы в Windows PE-файлы, поэтому общий процесс на
Debian выглядит так:

1. собрать единый `.cjs` через esbuild;
2. создать blob установленной Linux Node.js;
3. скачать официальный Windows x64 archive **той же версии Node.js** и
   проверить его SHA-256 по официальному `SHASUMS256.txt`;
4. скопировать из архива `node.exe` как `handy-processor.exe`;
5. выполнить ту же команду `postject` над этим PE-файлом;
6. перенести EXE на Windows, протестировать и подписать там.

Для текущего окружения парой будет Linux Node.js `24.19.0` и Windows x64
`node.exe` из официального релиза `24.19.0`.

Cross-build менее удобен: без Wine результат нельзя полноценно запустить в
Debian, а штатный `signtool` доступен на Windows. Поэтому для регулярных
релизов лучше использовать Windows VM или CI runner с Windows.

## Node.js 25.5 и новее

Начиная с Node.js 25.5 появился `node --build-sea sea-config.json`, который сам
создаёт итоговый executable без ручного копирования Node.js и `postject`.
Основная инструкция выше намеренно использует Node.js 24 LTS и старый процесс:
эта версия уже установлена и проверена в проекте. При переходе на более новую
LTS процедуру можно упростить, но Windows EXE всё равно следует собирать и
тестировать Windows-версией Node.js.

Отдельная пошаговая инструкция для нативного `--build-sea` находится в
[`windows-sea-node26.md`](windows-sea-node26.md).

## Ограничения и особенности

- SEA в Node.js 24 имеет статус active development.
- Итоговый файл включает весь Node.js runtime, поэтому он будет значительно
  больше исходного bundle — обычно сравним по размеру с `node.exe`.
- Это консольное приложение. Закрытие окна завершает сервер.
- При уже занятом порте `11434` сервер не запустится.
- EXE не устанавливает и не запускает себя автоматически. Автозапуск через
  Task Scheduler или Windows Service — отдельная задача.
- Сервер по умолчанию слушает только `127.0.0.1`, поэтому не доступен с других
  компьютеров. Не следует задавать `HOST=0.0.0.0` без отдельной аутентификации
  и правил firewall.

## Источники

- [Node.js 24: Single executable applications](https://nodejs.org/download/release/v24.18.0/docs/api/single-executable-applications.html)
- [Node.js 25.5.0: появление `--build-sea`](https://nodejs.org/en/blog/release/v25.5.0)
- [nodejs/postject](https://github.com/nodejs/postject)
- [esbuild: bundling API](https://esbuild.github.io/api/#bundle)
- [Microsoft SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)

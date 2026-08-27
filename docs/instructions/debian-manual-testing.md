# Ручное тестирование в Debian

Эта инструкция проверяет сервер, React/Mantine UI, SQLite и полный цикл
трансформации текста. Для обычной проверки в Debian launcher не нужен:
браузер открывает тот же интерфейс, который на Windows показывает Tauri.

Tray launcher остаётся отдельной опциональной оболочкой. В Debian его можно
собрать и проверить нативно; специфичное для WebView2 и Windows поведение всё
равно необходимо проверять по инструкции
[Windows tray launcher и portable release](windows-launcher.md).

## Требования

- Node.js не ниже `24.8` и npm;
- `curl` для проверки HTTP API;
- Python 3 только для форматирования JSON (часть `| python3 -m json.tool`
  можно убрать);
- любой современный браузер.

Для сборки launcher дополнительно нужны Rust и системные зависимости Tauri:

```bash
sudo apt-get update
sudo apt-get install -y \
  pkg-config \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Если Node.js установлен через NVM, в начале нового терминала загрузите NVM и
проверьте активную версию:

```bash
. "$HOME/.nvm/nvm.sh"
nvm use 24
node --version
npm --version
```

Для запуска TypeScript-сервера подходят Node.js 24 и 26. Node.js 26 обязателен
только для актуальной SEA-сборки.

## Первичная установка

Из корня репозитория:

```bash
cd ~/handy-processor
npm ci
```

`npm ci` нужно повторять после изменения `package-lock.json` или при создании
чистого рабочего каталога.

## Автоматические проверки

Перед ручной проверкой рекомендуется выполнить:

```bash
npm run typecheck
npm test
```

`npm test` самостоятельно собирает UI и сервер, запускает Node.js-тесты и
проверки упаковки. Успешный результат не должен содержать упавших тестов.

## Сборка и запуск сервера

Для повторной ручной проверки достаточно:

```bash
npm run build
export HANDY_PROCESSOR_RUNTIME_DIR="$(mktemp -d /tmp/handy-processor-manual.XXXXXX)"
npm start
```

`npm run build` создаёт:

- `web-dist/` — production-сборку React UI;
- `dist/` — скомпилированный TypeScript-сервер.

Runtime-каталог в `/tmp` отделяет тестовые `config.json` и SQLite-базу от
файлов проекта. При запуске сервер сам создаст в нём:

```text
config.json
handy-processor.sqlite3
```

В терминале должно появиться примерно следующее:

```text
Handy Processor is running.
Web UI: http://localhost:11434/
Handy API: http://127.0.0.1:11434/v1
```

Не закрывайте этот терминал до завершения ручной проверки. Остановить сервер
можно сочетанием `Ctrl+C`.

## Быстрая проверка HTTP API

Откройте второй терминал и выполните:

```bash
curl --fail --silent --show-error \
  http://127.0.0.1:11434/health | python3 -m json.tool
```

Ожидаются HTTP 200, `"status": "ok"` и
`"service": "handy-processor"`.

Проверьте трансформацию:

```bash
curl --fail --silent --show-error \
  --request POST \
  --header 'Content-Type: application/json' \
  --data '{"model":"handy-basic-transformer","messages":[{"role":"user","content":"МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА."}],"stream":false}' \
  http://127.0.0.1:11434/v1/chat/completions \
  | python3 -m json.tool
```

В `choices[0].message.content` ожидается:

```text
мне нужно 22 файла
```

Проверьте появление записи в истории:

```bash
curl --fail --silent --show-error \
  'http://127.0.0.1:11434/api/transformations?page=1&pageSize=10' \
  | python3 -m json.tool
```

## Проверка интерфейса

Откройте <http://127.0.0.1:11434/>. Если в VM настроено открытие ссылок, это
можно сделать из второго терминала:

```bash
xdg-open http://127.0.0.1:11434/
```

Рекомендуемый чек-лист:

1. **Голосовой редактор**
   - текст вводится и редактируется;
   - `Ctrl+Z`, `Ctrl+Y` и кнопки отмены/повтора работают;
   - фокус меняет статус контекста без сдвига интерфейса;
   - после небольшой паузы отображается статус **Сохранено**.
2. **История**
   - тестовый запрос виден первым;
   - исходный и преобразованный текст показаны рядом;
   - видны применённые правила;
   - карточки остаются компактными при ширине окна примерно 800–1000 px.
3. **Список правил**
   - каждое правило можно выключить и снова включить;
   - следующий API-запрос учитывает новое состояние;
   - старая запись истории не изменяется.
4. **Настройки**
   - светлая, тёмная и системная темы переключаются сразу;
   - отдельно меняется размер текста основного интерфейса;
   - меняется размер шрифта рабочего текста редактора;
   - после обновления страницы выбранные параметры сохраняются.

## Проверка сохранения черновика

1. Введите текст в редакторе и дождитесь статуса **Сохранено**.
2. Остановите сервер сочетанием `Ctrl+C`.
3. В том же терминале, не меняя `HANDY_PROCESSOR_RUNTIME_DIR`, запустите:

   ```bash
   npm start
   ```

4. Обновите страницу. Текст должен восстановиться, а undo-история — начаться
   заново.

Путь к тестовой базе сервер печатает при запуске. Не удаляйте runtime-каталог,
пока проверяете восстановление данных.

## Режим разработки

Для работы над сервером с автоматическим перезапуском используется:

```bash
export HANDY_PROCESSOR_RUNTIME_DIR="$(mktemp -d /tmp/handy-processor-dev.XXXXXX)"
npm run dev
```

Эта команда один раз собирает UI и затем следит за изменениями серверного
TypeScript. После изменения React или CSS нужно отдельно повторить
`npm run build:ui` либо перезапустить `npm run dev`.

`npm run dev:ui` запускает только Vite dev server. Сейчас в его конфигурации
нет proxy к backend, поэтому для полноценной ручной проверки API удобнее
использовать `npm run build` и `npm start`.

## Сборка и проверка launcher в Debian x64

Для SEA-сборки переключитесь на Node.js 26, затем установите npm-зависимости
launcher и выполните одну сборочную команду:

```bash
. "$HOME/.nvm/nvm.sh"
nvm use 26
npm run install:launcher
npm run build:linux:launcher
```

Результат находится в одном каталоге:

```text
launcher/src-tauri/target/release/HandyProcessor
launcher/src-tauri/target/release/handy-processor-server
launcher/src-tauri/target/release/config.json
```

Запуск из графической сессии Debian:

```bash
npm run start:linux:launcher
```

Можно также отдельно выполнить проверки:

```bash
npm run test:launcher
npm run check:launcher
```

В Debian проверяются WebKitGTK-окно, tray, меню и lifecycle sidecar. Отсутствие
Windows console window и поведение WebView2 здесь проверить нельзя; для этого
используйте Windows portable build.

## Проверка с Handy

Handy на Windows должен обращаться к Base URL:

```text
http://127.0.0.1:11434/v1
```

Для Debian smoke test сам Handy не требуется: приведённый выше POST-запрос
имеет тот же OpenAI-compatible формат и проходит через тот же pipeline.

Если порт изменён в созданном `config.json`, замените `11434` во всех URL на
настроенное значение.

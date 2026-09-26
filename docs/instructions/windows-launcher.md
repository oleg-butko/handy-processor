# Windows tray launcher и portable release

Актуально для `handy-processor` на 23 августа 2026 года. Portable-сборка,
WebView2-интерфейс, system tray и управление server sidecar проверены на
Windows.

## Что получается

Лаунчер опционален. Один и тот же сервер можно запускать двумя способами:

```text
standalone:
  handy-processor-server.exe

desktop:
  HandyProcessor.exe -> tray + WebView2 window
                    -> handy-processor-server.exe --managed
```

Portable-каталог содержит launcher, сервер, настройки, пример шаблонов и
комплект коррекции:

```text
HandyProcessor.exe
handy-processor-server.exe
config.json
templates.example.txt
rechprav/handy-terms.toml
rechprav/handy-dictionary.toml
rechprav/DICTIONARY-NOTICE.md
rechprav/target/release/examples/correction_worker.exe
rechprav/artifacts/opencorpora-wordfreq/dictionary.rpt
```

`handy-processor.sqlite3` создаётся после первого запроса. React/Mantine UI,
Node.js runtime и SQLite driver встроены в server EXE; отдельные DLL,
`web-dist`, Node.js и Python на пользовательской машине не нужны.

Для вкладки [«Шаблоны»](../templates.md) создайте собственный `templates.txt`
рядом с EXE или выберите другой файл в интерфейсе. Windows-лаунчер по умолчанию
открывает эту вкладку двойным Win (200 мс); одиночный Win при включённой
привязке не открывает «Пуск». Настройки находятся справа сверху во вкладке.

## Требования к Windows build-машине

- Windows x64;
- Node.js 26 x64 (проверено с `26.7.0`) и npm;
- stable Rust toolchain `x86_64-pc-windows-msvc`;
- Visual Studio 2022 Build Tools с workload **Desktop development with C++**;
- WebView2 Runtime для запуска UI (обычно уже присутствует в Windows 10/11).

Проверка окружения:

```powershell
node --version
node -p "process.arch"
rustc --version
rustc --print host-tuple
cargo --version
```

Ожидаются Node `v26.x`, архитектура `x64` и Rust target
`x86_64-pc-windows-msvc`.

## Сборка portable-каталога

Из корня репозитория:

```powershell
npm ci
npm run install:launcher
cargo fetch --manifest-path rechprav/Cargo.toml --locked
npm run build:windows:portable
```

Последняя команда последовательно:

1. собирает Rust worker коррекции, React UI и Node.js 26 SEA;
2. копирует SEA под target-specific sidecar-именем Tauri;
3. собирает release launcher без installer bundle;
4. проверяет сигнатуру `MZ` всех трёх EXE, валидность `config.json` и наличие ресурсов коррекции;
5. формирует `release\handy-processor-windows-x64`, включая worker, профили и словарь.

`cargo fetch` нужен однократно для кеша зависимостей worker; сама его сборка
использует `--locked --offline`. Полный словарь не хранится в Git: новый ZIP
из `npm run archive:sources` уже включает его, а при Git checkout ресурс нужно
подготовить или скопировать отдельно. Повторная упаковка сохраняет существующие
TOML-настройки коррекции в release-каталоге. Подробнее, включая исправление
`correction_worker.exe ENOENT`: [коррекция в Handy Processor](../rechprav/handy-integration.md#windows-portable-и-ошибка-enoent).

SQLite-базу из рабочего или старого release-каталога build script не копирует
и не удаляет.

### Версия в заголовке окна

Заголовок WebView2-окна имеет вид `Handy Processor v0.123`. Первая часть
задаётся вручную в корневом файле `version-major.txt`. Вторая часть во время
обычной сборки вычисляется командой `git rev-list --count HEAD`, поэтому она
увеличивается с каждым коммитом в основной истории репозитория. Скрипт создания
ZIP сохраняет вычисленное значение в `version-build.txt` внутри архива.

При сборке распакованного архива Git не требуется: `build.rs` автоматически
читает `version-build.txt`. Переменная окружения нужна только для исходников,
в которых нет ни `.git`, ни этого файла, либо для явного переопределения:

```powershell
$env:HANDY_PROCESSOR_BUILD_NUMBER = "123"
npm run build:windows:portable
```

Это display-версия launcher, а не версия API-контракта server из `/health`.

## Проверки до передачи артефакта

```powershell
npm test
npm run test:packaging
npm run test:launcher
npm run check:launcher
```

Затем запустите:

```powershell
.\release\handy-processor-windows-x64\HandyProcessor.exe
```

Лаунчер стартует скрытым. Ожидаемое поведение:

- в notification area появляется иконка Handy Processor;
- левый или двойной клик показывает окно с localhost UI;
- заголовок окна содержит актуальную display-версию launcher;
- закрытие и сворачивание окна скрывают его и убирают из taskbar;
- после перезапуска восстанавливаются последние размер и положение окна;
- правый клик открывает native menu;
- повторный запуск EXE не создаёт второй tray icon, а показывает первое окно;
- экспорт истории из вкладки **Настройки** сохраняет `.txt` или `.jsonl` в этот же
  portable-каталог и показывает полный путь в интерфейсе;
- пункт **Выход** корректно останавливает только тот сервер, который запустил
  сам launcher.

Правый клик предоставляет команды открытия окна и браузера, копирования API
URL, открытия папки данных, перезапуска managed server и выхода.

Проверьте API отдельно:

```powershell
Invoke-RestMethod "http://127.0.0.1:11434/health"
Invoke-WebRequest "http://127.0.0.1:11434/"
```

## Standalone и уже запущенный сервер

Server EXE в portable-каталоге остаётся полностью самостоятельным:

```powershell
.\release\handy-processor-windows-x64\handy-processor-server.exe
```

Если затем запустить `HandyProcessor.exe`, launcher проверит `/health` и
подключится к совместимому процессу. Такой процесс считается внешним: launcher
не завершает и не перезапускает его. Если порт занят другим сервисом или другой
версией Handy Processor, появляется native error dialog.

Managed server запускается с Windows-флагом `CREATE_NO_WINDOW`, поэтому его
console window не появляется в taskbar. При штатном выходе launcher отправляет
команду `shutdown`, ждёт до четырёх секунд и только затем применяет forced kill.
EOF управляющего pipe также завершает server после аварийного закрытия
launcher.

## Конфиг и данные

Оба EXE используют `config.json` рядом с собой:

```json
{
  "port": 11434
}
```

Менять порт следует при полностью остановленных launcher и server. Для Handy
Base URL должен совпадать с конфигом, например
`http://127.0.0.1:11434/v1`.

Portable-каталог должен находиться в доступном для записи месте, поскольку там
создаются SQLite-база и файлы экспорта истории. Повторный экспорт в один день
не перезаписывает существующий файл: launcher добавляет к имени `-2`, `-3` и
так далее. При ошибке экспорта рядом создаётся `handy-processor.log`. При
отправке отчёта достаточно приложить последнюю JSON-строку из этого файла: она
содержит техническую причину, но не тексты преобразований. Текущий release не
следует помещать в `Program Files`.
NSIS target оставлен в Tauri-конфигурации для будущего installer-варианта, но
installer потребует переноса изменяемых данных в `%LOCALAPPDATA%`.

## Подписание и распространение

Подписывать нужно оба финальных файла после формирования portable-каталога:

```powershell
signtool sign /f path\to\certificate.pfx /fd SHA256 `
  /tr https://your-rfc3161-timestamp-server.example /td SHA256 `
  .\release\handy-processor-windows-x64\HandyProcessor.exe

signtool sign /f path\to\certificate.pfx /fd SHA256 `
  /tr https://your-rfc3161-timestamp-server.example /td SHA256 `
  .\release\handy-processor-windows-x64\handy-processor-server.exe
```

После подписи повторно проверьте запуск на чистой Windows без установленного
Node.js. Portable-каталог можно упаковать стандартной командой
`Compress-Archive`.

## Почему launcher находится в этом репозитории

`launcher/` имеет собственные npm и Cargo lock-файлы и не вмешивается в
серверную сборку, но общий репозиторий удерживает атомарный контракт версий
`/health`, имён sidecar и portable layout. Это уменьшает риск собрать launcher
и server несовместимых версий. Отдельный репозиторий имеет смысл только если у
компонентов появятся независимые релизные циклы.

## Источники

- [Tauri: System Tray](https://v2.tauri.app/learn/system-tray/)
- [Tauri: Embedding External Binaries](https://v2.tauri.app/develop/sidecar/)
- [Tauri: Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)
- [Tauri: Window State](https://v2.tauri.app/plugin/window-state/)
- [Microsoft: Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)

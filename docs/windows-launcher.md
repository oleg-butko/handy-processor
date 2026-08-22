# Windows tray launcher и portable release

Актуально для `handy-processor` на 21 августа 2026 года.

## Что получается

Лаунчер опционален. Один и тот же сервер можно запускать двумя способами:

```text
standalone:
  handy-processor-server.exe

desktop:
  HandyProcessor.exe -> tray + WebView2 window
                    -> handy-processor-server.exe --managed
```

Portable-каталог содержит только три поставляемых файла:

```text
HandyProcessor.exe
handy-processor-server.exe
config.json
```

`handy-processor.sqlite3` создаётся после первого запроса. React/Mantine UI,
Node.js runtime и SQLite driver встроены в server EXE; отдельные DLL,
`web-dist`, Node.js и Python на пользовательской машине не нужны.

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
npm run build:windows:portable
```

Последняя команда последовательно:

1. собирает React UI и Node.js 26 SEA;
2. копирует SEA под target-specific sidecar-именем Tauri;
3. собирает release launcher без installer bundle;
4. проверяет сигнатуру `MZ` обоих EXE и валидность `config.json`;
5. формирует `release\handy-processor-windows-x64`.

SQLite-базу из рабочего или старого release-каталога build script не копирует
и не удаляет.

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
- закрытие и сворачивание окна скрывают его и убирают из taskbar;
- правый клик открывает native menu;
- повторный запуск EXE не создаёт второй tray icon, а показывает первое окно;
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
создаётся SQLite-база. Текущий release не следует помещать в `Program Files`.
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
- [Microsoft: Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)

# 024: комплект коррекции в Windows portable

2026-09-23, ветка `fix/windows-correction-runtime`.

Пользователь сообщил о `spawn .../correction_worker.exe ENOENT` после
`npm run build:windows:portable`. Причина: команда обходила `build:correction`,
а упаковщик копировал только launcher, SEA-сервер, config и пример шаблонов.
Исходный ZIP также не содержал исходников/ресурсов `rechprav`.

Исправления:

- Windows portable workflow сначала собирает нативный worker.
- Упаковщик проверяет Windows-сигнатуру worker и наличие его комплекта,
  затем копирует binary, два TOML, полный dictionary.rpt и атрибуцию словаря
  в те пути, которые использует сервер. Проверки выполняются до изменения release.
- При повторной упаковке существующие TOML в release сохраняются.
- ZIP исходников включает Rust-код, Cargo.lock, профили и платформонезависимый
  словарь. Сборочные каталоги, Python-окружение и приватные корпуса не включаются;
  Python-кеши отфильтрованы. Ресурс dictionary.rpt остаётся вне Git.
- Инструкция дополнена зависимостями Cargo, составом каталога и способом
  исправить уже собранный portable без пересборки сервера и лаунчера.

Проверки:

| Команда | Результат |
|---|---|
| `npm run test:packaging` | [4/4](packaging-tests.txt): состав комплекта, сохранение настроек, отсутствующие ресурсы и ELF вместо Windows-worker |
| `npm run build:correction` | [Нативная Linux-сборка прошла](worker-build.txt) |
| `node rechprav/scripts/check_runtime_bundle.mjs` | [4/4 примера](relocated-runtime.json): реальный worker с полным словарём запущен из временного каталога вне checkout |
| `npm run archive:sources` | [Вывод создания ZIP](archive-build.txt) |
| Проверка созданного ZIP существующим unpacker и сравнение SHA256 обязательных файлов | [Результат](archive-check.json) |

Сигнатуры Windows EXE в unit-тестах — технические заглушки для проверки упаковки.
Исполнение нативного Windows PE, полный Windows SEA/Tauri build и нажатие reload
в Windows здесь не проверены: среда Debian. Исправление готово для следующей
Windows-сборки. Алгоритм и веса не менялись, оценка качества на корпусах не
повторялась. Рабочие серверы и БД не менялись.

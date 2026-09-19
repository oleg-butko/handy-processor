# 000 — первичное обследование

Дата: 2026-09-19. Тип: проверка предпосылок, не benchmark алгоритмов.

Цель: выяснить, что уже есть в проекте и что необходимо до сравнения корректоров.
Проверены `src/transformation-rules.ts`, `src/russian-numerals.ts`,
`src/transformation-log.ts`, `src/app.ts`, README, package.json и исходный план.

## Наблюдения

1. Сервер Node.js/TypeScript применяет локальные правила синхронно. Rust относится
   к launcher; самостоятельный package Rechprav ещё предстоит создать.
2. Числительные исправляются ограниченным fuzzy-поиском в своём словаре.
   Прямое расширение этого словаря не решает общую задачу ASR.
3. Текстовый экспорт записывает source/result с разделителями. Result — результат
   автоматических правил, а не проверенная расшифровка. Для корпуса нужен gold.
4. `rustc` и `cargo` не найдены в PATH и `/home/user1/.cargo/bin`.
   `rustc --version` дал exit 127; установка не выполнялась.
5. В просмотренном дереве исходников нет размеченного ASR-корпуса.
   Другие пользовательские каталоги и содержимое живой SQLite не исследовались.

Версии, машина, Git SHA и хеш исходного плана: [environment.json](environment.json).

## Воспроизведение проверки

Из корня проекта:

```bash
git status --short
git rev-parse HEAD
rg --files -g '!node_modules' -g '!package-lock.json'
rg -n 'original_text|transformed_text|iterateTextPairs' src/transformation-log.ts
rg -n 'sendTransformationExport|EXPORT_RESULT_SEPARATOR' src/app.ts
command -v rustc
command -v cargo
node --version
python3 --version
lscpu
free -m
sha256sum docs/rechprav/first-plan.md
```

Отсутствие команды в PATH не доказывает отсутствие toolchain во всей файловой
системе; дополнительно проверен обычный путь `~/.cargo/bin`. Это достаточное
основание включить подготовку среды в следующий этап.

## Решение

Начать с схемы данных, метрик и KEEP, затем tokenizer/formatter и baseline.
Не выбирать алгоритм до реальных измерений. Проблемы запуска и доступа к
документации не являются отрицательными результатами сравнения алгоритмов.

Precision, recall, WER, FCR, latency и RAM корректоров: **не измерялись**.
Установка зависимостей, запуск сервера и изменение текущих правил не выполнялись.

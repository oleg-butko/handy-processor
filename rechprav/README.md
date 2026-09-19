# Rechprav

Локальный Rust-инструмент для исследования коррекции ошибок русского ASR.
Реализован фундамент: подготовка и проверка разметки, токенизация, просмотр
и контрольный алгоритм KEEP. Эта версия **ещё не исправляет слова**.

- [Поэтапная реализация](../docs/rechprav/implementation-plan.md)
- [Протокол экспериментов](../docs/rechprav/experiment-protocol.md)
- [История](../docs/rechprav/history.md)
- [Сводка результатов](../docs/rechprav/RESULTS.md)
- [Первичное обследование](experiments/000-inventory/report.md)
- [Формат корпуса и инструкция по разметке](../docs/rechprav/corpus-format.md)

## Сборка и проверки

Из корня `handy-processor`. В текущей VM Rust доступен через `~/.local/bin/`:

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline
/home/user1/.local/bin/cargo test --manifest-path rechprav/Cargo.toml --locked --offline
/home/user1/.local/bin/cargo clippy --manifest-path rechprav/Cargo.toml --locked --offline --all-targets -- -D warnings
/home/user1/.local/bin/cargo fmt --manifest-path rechprav/Cargo.toml -- --check
rechprav/target/release/rechprav --help
```

На другой машине используйте свой `cargo`. Перед первой offline-сборкой
зависимости из Cargo.lock нужно однократно загрузить через `cargo fetch --locked`
с тем же `--manifest-path`. Сам бинарник не использует сеть и внешние сервисы.

## Попробовать на технических примерах

```bash
mkdir -p rechprav/artifacts
rechprav/target/release/rechprav inspect 'драйвир, CUDA и C++'
rechprav/target/release/rechprav validate rechprav/data/fixtures/corpus.jsonl
rechprav/target/release/rechprav review rechprav/data/fixtures/review-input.txt --output-dir rechprav/artifacts/review-001
rechprav/target/release/rechprav benchmark rechprav/data/fixtures/corpus.jsonl --output-dir rechprav/artifacts/keep-001
rechprav/target/release/rechprav prepare rechprav/data/fixtures/review-input.txt --output rechprav/artifacts/pending-001.jsonl --group technical-pilot --kind synthetic
```

Каждый output file/directory должен быть новым; при повторном запуске выберите
другое имя. Существующие файлы, каталоги и ссылки не перезаписываются.
Ошибочный корпус проверяется до создания директории benchmark.

`review` создаёт:

- `review.txt`: исходные строки с маркерами `-` для lexical tokens;
- `corrected.txt`: точную копию входных байтов в режиме KEEP;
- `debug.jsonl`: исходник, окончания строк, байтовые диапазоны, токены, решения и переносы;
- `manifest.json`: параметры запуска, хеш входа и встроенные при компиляции хеши исходников.

`--width` принимает 32–130 (по умолчанию 130); ограничиваются и отображаемые
колонки, и число Unicode scalar values. Заголовок `[000001.2]` означает вторую
визуальную часть первой входной строки. `↳` означает продолжение одного длинного
токена/участка, а не новую позицию слова. В debug диапазон такого участка повторяется
и снабжён `continuation_part`. Управляющие символы экранируются только в review.
Обычный перенос жадный между lexical units; предпочтение границ предложений
и отображение замен другой длины будут добавлены вместе с корректором.

NFC и е/ё нормализуются только в поисковых ключах. `--strict-yo` сохраняет
различие е/ё при сравнении. Пунктуация и whitespace не занимают позиции маркеров;
числа, URL и технические атомы занимают одну позицию и защищены.
Это детерминированные эвристики токенизации, не морфологический анализ.

`benchmark` пока поддерживает только KEEP: сохраняет `metrics.json`,
`predictions.jsonl`, `manifest.json`. Precision без предложений равен null,
FCR равен нулю при наличии правильных слов, WER до/после одинаков.
Поле времени означает стоимость проверки и вычисления метрик, **не latency корректора**.
Синтетические и реальные данные, а также разные splits нельзя смешивать в одном прогоне.
Все файлы `data/fixtures/` — искусственные технические примеры, не история Handy.

## Организация

`src/` — библиотека и CLI; `tests/` — проверки поведения, безопасности вывода и метрик;
`data/fixtures/` — технические примеры; `experiments/` — сохранённые прогоны.
Опыты не меняют сервер HandyProcessor до этапа интеграции.

Словари и модели собираются заранее в `artifacts/`; runtime работает без сети.
Приватные данные и построчные отчёты помещаются в ignored private-каталоги.
В отслеживаемых экспериментах сохраняются воспроизводимые команды, конфиги,
хеши ресурсов, агрегированные результаты и разрешённые примеры.

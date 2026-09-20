# Rechprav

Локальный Rust-инструмент для исследования коррекции ошибок русского ASR.
Реализованы подготовка/проверка разметки, токенизация, просмотр, KEEP и первый
экспериментальный корректор: неизвестное слово заменяется, только если полный
поиск нашёл ровно один словарный вариант на расстоянии Levenshtein 1.
Словарь в примерах маленький и синтетический; качество на реальном ASR не проверено.

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

Для исправлений указать конфиг; без него используется KEEP:

```bash
rechprav/target/release/rechprav inspect 'драйвир, маско и капер' --config rechprav/configs/baseline.toml
rechprav/target/release/rechprav correct rechprav/data/fixtures/baseline/review-input.txt --config rechprav/configs/baseline.toml --output-dir rechprav/artifacts/correct-002
rechprav/target/release/rechprav benchmark rechprav/data/fixtures/baseline/corpus.jsonl --config rechprav/configs/baseline.toml --output-dir rechprav/artifacts/baseline-002
python3 rechprav/scripts/run_baseline_experiment.py rechprav/artifacts/comparison-002
```

`correct` — псевдоним `review`. `--engine baseline` требует `--config`;
`--engine keep` не принимает конфиг. Скрипт сравнения запускает KEEP, baseline
и отдельные отключения domain/ignore/защиты заглавных слов на одном техническом
корпусе. Он проверяет, что бинарник собран из текущих исходников.

## Словари и политика B1

Пути в TOML считаются относительно самого конфига. `dictionary.txt` содержит
русские словоформы по одной на строку; пустые строки и строки `#` пропускаются.
Файл проверяется и индексируется один раз при запуске. Это не морфологический
анализатор: отсутствие слова означает неизвестность, а не доказанную ошибку.

`domain.txt` и `ignore.txt` защищают точные нормализованные lexical tokens.
Термин в domain не даёт права автоматически заменять похожее русское слово:
`куда → CUDA` этот baseline не выполняет. Главный словарь сейчас содержит только
русские буквы; иностранные имена/термины помещать в domain.

```bash
rechprav/target/release/rechprav build-dict rechprav/data/fixtures/baseline/dictionary.txt --output rechprav/artifacts/dictionary-002.json
```

`build-dict` создаёт проверенный нормализованный JSON-словарь с метаданными.
Можно указать его в `dictionary` конфига. Он ещё не является сохранённым на диск
trie: дерево строится один раз при загрузке. Е/ё-режим конфига и JSON обязан совпадать.

Все параметры B1 находятся в `[policy]`: distance=1, минимальная/максимальная
длина слова, максимальное число кандидатов/посещённых узлов, защита titlecase
и uppercase. Слова ≤3 символов, смешанный регистр, URL, пути, dotted identifiers,
backtick-code, латиница и числа защищены. По умолчанию также защищены все слова
с заглавной буквы, включая начало предложения; это снижает recall.
При неполном поиске, двух кандидатах или отсутствии кандидатов выбирается KEEP.
Перестановка соседних букв считается двумя правками обычного Levenshtein.

Поиск и решение разделены; shortlist упорядочен по расстоянию и слову.
В debug остаются кандидаты, расстояния, `score = -distance`, причина отказа и
полнота поиска. Confidence остаётся null: единственный кандидат не означает
высокую вероятность правильной замены. Частоты доступны как отдельный вариант B2.

Каждый output file/directory должен быть новым; при повторном запуске выберите
другое имя. Существующие файлы, каталоги и ссылки не перезаписываются.
Ошибочный корпус проверяется до создания директории benchmark.

`review` создаёт:

- `review.txt`: исходные строки и замены, с `-` на месте неизменённых lexical tokens;
- `corrected.txt`: точную копию входных байтов в режиме KEEP;
- `debug.jsonl`: исходник, окончания строк, байтовые диапазоны, токены, решения и переносы;
- `manifest.json`: параметры запуска, хеш входа и встроенные при компиляции хеши исходников.

`--width` принимает 32–130 (по умолчанию 130); ограничиваются и отображаемые
колонки, и число Unicode scalar values. Заголовок `[000001.2]` означает вторую
визуальную часть первой входной строки. `↳` означает продолжение одного длинного
токена/участка, а не новую позицию слова. В debug диапазон такого участка повторяется
и снабжён `continuation_part`. Управляющие символы экранируются только в review.
Обычный перенос жадный между lexical units, с учётом ширины SOURCE и CHANGES.
Предпочтение границ предложений пока не реализовано. Длинная замена и длинный
исходник показываются частями одного token range, без дополнительных позиций слов.

NFC и е/ё нормализуются только в поисковых ключах. `--strict-yo` сохраняет
различие е/ё при сравнении. Пунктуация и whitespace не занимают позиции маркеров;
числа, URL и технические атомы занимают одну позицию и защищены.
Это детерминированные эвристики токенизации, не морфологический анализ.
Для baseline режим е/ё задаётся в TOML; CLI `--strict-yo` применим к KEEP.

`benchmark` поддерживает KEEP, B1 и B2: сохраняет `metrics.json`,
`predictions.jsonl` с правками/решениями и `manifest.json` с хешами ресурсов.
Precision без предложений равен null. Для KEEP FCR равен нулю при наличии
правильных слов, WER до/после одинаков. Общая precision включает предложения
внутри неподдерживаемых compound spans; отдельные 1→1 метрики их не скрывают.
Recall всех gold spans и recall только 1→1 показаны отдельно. Recall@K здесь
считается по выданным кандидатам после защит/OOV-фильтра, а не по «чистому» генератору.
Поле времени включает проверку, коррекцию и расчёт метрик, **не isolated latency**.
Синтетические и реальные данные, а также разные splits нельзя смешивать в одном прогоне.
Все файлы `data/fixtures/` — искусственные технические примеры, не история Handy.

## Полный словарь и частоты B2

Подготовка данных и лицензии описаны в
[resource-sources.md](../docs/rechprav/resource-sources.md). После экспорта:

```bash
~/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --examples --bin rechprav
rechprav/target/release/rechprav inspect 'я установил драйвир' --config rechprav/configs/opencorpora-b2.toml
python3 rechprav/scripts/run_frequency_experiment.py rechprav/experiments/my-frequency-run
```

`[frequency]` в TOML включает B2; без этой секции остаётся B1. Генератор и защиты
одинаковы. Частотный список не расширяет словарь. Все коэффициенты находятся в
`[frequency.scoring]`; начальные значения эвристические, не обученные:

```text
prior(w) = weight * min(log10(1 + frequency_per_billion(w)), log_cap)
candidate_score = prior(candidate) - edit_penalty * distance
keep_score = prior(original) + keep_bias
```

Для замены нужны полный поиск, положительная известная частота лучшего кандидата,
перевес над KEEP ≥ `min_keep_margin` и над вторым кандидатом ≥
`min_runner_up_margin`. При единственном кандидате второй порог не применяется.
Отсутствующая частота даёт нулевой вклад, но не доказывает ошибочность исходника.
Оба порога положительны: равенство оценок всегда оставляет исходник. При равных
оценках список предложений упорядочен по ключу, без случайности.

В debug добавлены `keep_score`, `keep_margin`, `runner_up_margin`, а у кандидатов —
`frequency_per_billion`. `candidate_score` — оценка лучшей альтернативы даже при
KEEP; `candidate` по-прежнему означает выбранный выход. `confidence` остаётся null.
Защищённые/известные слова не проходят скоринг. При неполном поиске оценки относятся
только к найденной части, `search_complete=false` запрещает замену.

`examples/profile.rs` отдельно измеряет загрузку ресурсов и warm `analyze` на
технических строках; включает токенизацию, поиск, оценку и сборку результата.
Это предварительное измерение на синтетике, не задержка Handy или чистого lookup.

## Упакованный словарь

Для заранее собранного плоского trie вместо сборки BTreeMap при каждом старте:

```bash
rechprav/target/release/rechprav build-index rechprav/artifacts/opencorpora-wordfreq/dictionary.txt --output rechprav/artifacts/opencorpora-wordfreq/dictionary.rpt
rechprav/target/release/rechprav inspect 'я установил драйвир' --config rechprav/configs/opencorpora-packed-b2.toml
python3 rechprav/scripts/run_index_experiment.py rechprav/experiments/my-index-run --index rechprav/artifacts/opencorpora-wordfreq/dictionary.rpt
```

`build-index` принимает TXT или JSON от `build-dict`, создаёт новый `.rpt` и
сохраняет SHA256 исходного ресурса. Расширение `.rpt` в `dictionary` включает
готовый индекс; TOML и скоринг остаются прежними. Несовпадение `strict_yo`,
повреждение файла или нарушение структуры приводят к ошибке до создания review.
Молчаливого восстановления из другого словаря нет. Частоты читаются из TSV или `.rpf`.
Формат, ограничения и проверки: [packed-index.md](../docs/rechprav/packed-index.md).

Runner опыта индекса по умолчанию строит новый стандартный `.rpt`; `--index`
позволяет повторить сравнение с существующим файлом. Он проверяет точное совпадение
всех решений B2 с опытом 003 и результатов генератора между реализациями.
`examples/profile.rs --queries queries.json` отдельно измеряет прямые запросы
генератора, включая неподдерживаемые корректором distance=2; это проверка индекса,
не включение distance=2 в правила исправлений.

## Готовая частотная таблица и измерение загрузки

```bash
rechprav/target/release/rechprav build-frequency rechprav/artifacts/opencorpora-wordfreq/frequency.tsv --output rechprav/artifacts/opencorpora-wordfreq/frequency.rpf
rechprav/target/release/rechprav inspect 'я установил драйвир' --config rechprav/configs/opencorpora-packed-frequency-b2.toml
python3 rechprav/scripts/run_load_experiment.py rechprav/experiments/my-load-run --profile tsv=rechprav/configs/opencorpora-packed-b2.toml --profile packed=rechprav/configs/opencorpora-packed-frequency-b2.toml
```

Сборка сохраняет точные `f64` и проверяет все значения перед записью. Расширение
`.rpf` в `[frequency].path` включает бинарный поиск в готовой таблице.
[Формат и проверки](../docs/rechprav/packed-frequency.md).

Runner измеряет по три новых процесса каждого варианта в перемешанном порядке;
`--repetitions` меняет число процессов, `--analyze-repetitions` — повторения
корпуса внутри процесса (по умолчанию 100). Отдельно сохраняются read, SHA256,
decode и вложенные проверки словаря. Вложенные фазы нельзя складывать с decode,
а суммы медиан не обязаны совпадать с медианой общего времени.
Кеш ОС не очищается. Runner проверяет исходники бинарника и побайтное совпадение
предсказаний/метрик B2 с опытом 003; он предназначен для вариантов хранения
тех же ресурсов, а не для сравнения новых алгоритмов коррекции.
Выводы и отвергнутые варианты: [опыт 005](experiments/005-loading/report.md).

## Организация

`src/` — библиотека и CLI; `tests/` — проверки поведения, безопасности вывода и метрик;
`data/fixtures/` — технические примеры; `experiments/` — сохранённые прогоны.
Опыты не меняют сервер HandyProcessor до этапа интеграции.

Словари и модели собираются заранее в `artifacts/`; runtime работает без сети.
Приватные данные и построчные отчёты помещаются в ignored private-каталоги.
В отслеживаемых экспериментах сохраняются воспроизводимые команды, конфиги,
хеши ресурсов, агрегированные результаты и разрешённые примеры.

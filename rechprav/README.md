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

## Отключаемые наборы терминов

Отдельный экспериментальный проход поддерживает речевые варианты аббревиатур,
1–3 исходных слова, переключение наборов, вес отдельного термина и отказ при
конфликте. Примеры: `пэ эн зэ → ПНЗ`, `свод/ввод → слот` в выбранном контексте.
В обычный B2 он автоматически не включён; команды `terms-*` выполняют только
терминологический проход, без последовательного применения B2.

```bash
rechprav/target/release/rechprav terms-inspect 'проверь пэ эн зэ' --config rechprav/configs/terms-example.toml
rechprav/target/release/rechprav terms-review input.txt --config rechprav/configs/terms-example.toml --output-dir rechprav/artifacts/terms-001
rechprav/target/release/rechprav terms-inspect 'проверь пэнзэ' --config rechprav/configs/terms-example.toml --disable-terms
python3 rechprav/scripts/run_term_experiment.py rechprav/experiments/my-term-run
```

Настройки и формула score: [terms.md](../docs/rechprav/terms.md).
`boost` повышает/понижает предпочтение, но не является вероятностью.
Выключатели есть в корне TOML, у каждого набора и у каждого термина.
`terms-review` сохраняет corrected.txt, JSONL с исходными диапазонами и manifest.
Для нескольких исходных слов используется новый span evaluator.

[Опыт 006](experiments/006-terms/report.md): 43 технические записи, 12 профилей.
Edit-1 дал 18 верных и 6 неверных замен; простые контекстные подсказки не защищают
обычные слова достаточно надёжно. Distance=2 ухудшил компромисс. Механизм
управления принят для следующих опытов; автоматическая вставка не включается.

[Опыт 007](experiments/007-context/report.md) добавляет необязательные ограничения:
`max_known_distance`, соседние фразы с `{term}`, запрещающие `context_exclusions`
и `require_context` для OOV. Пример — `configs/terms-context.toml` с разными
контекстами ПНЗ и БНЗ. На новых 42 примерах профиль дал 14 TP/4 FP вместо 12/17;
обычные слова всё ещё могут быть испорчены, а часть полезных правок теряется.
Старые конфиги без новых полей побайтно повторяют прежние решения и диагностику.

```bash
rechprav/target/release/rechprav terms-inspect 'проверь пензе на карте' --config rechprav/configs/terms-context.toml
rechprav/target/release/rechprav terms-inspect 'пэнзэ на стенде' --config rechprav/configs/terms-context.toml
python3 rechprav/scripts/run_context_experiment.py rechprav/experiments/my-context-run
```

[Опыт 008](experiments/008-spans/report.md) сравнивает три варианта выбора
пересекающихся диапазонов: `policy.span_tie_break = "shortest"` (прежний default),
`"distance"` и `"longest"`. На новых 40 примерах две альтернативы добавили одну
правильную замену в legacy; на контекстном профиле 007 изменений нет.
Меньшая дистанция не гарантирует сохранение правильной соседней буквы `пэ`.
Default оставлен прежним; варианты доступны для экспериментов и review.

```bash
python3 rechprav/scripts/run_span_experiment.py rechprav/experiments/my-span-run
```

## Произношения букв аббревиатур

В определении отдельного термина `pronunciation = "ru_letters"` добавляет
названия букв и варианты разделения пробелами. Для ПНЗ это `пээнзэ`,
`пэ энзэ`, `пээн зэ`, `пэ эн зэ`. Ручные aliases объединяются с ними;
повторения не увеличивают оценку. Поле можно убрать, сохранив ручной список.
Для генерации без ручного списка допустимо `aliases = []`.

Поддерживаются 2–3 заглавные русские буквы, кроме Й/Ъ/Ь. Это фиксированная
эвристика, не фонетическая модель. Контекстные проверки и запреты сохраняются.
Происхождение варианта видно в debug, полный активный список — в manifest.
Пример содержит независимые термины ПНЗ, БНЗ, ПЛК, АЦП, ОЗУ, СКС и ручной `слот`:

```bash
rechprav/target/release/rechprav terms-inspect 'контроллер пэ эль ка' --config rechprav/configs/terms-pronunciation.toml
python3 rechprav/scripts/run_pronunciation_experiment.py rechprav/experiments/my-pronunciation-run
```

[Настройки и ограничения](../docs/rechprav/terms.md),
[опыт 009](experiments/009-pronunciation/report.md).
На новых 40 технических примерах edit-union дал 25 TP/2 FP вместо ручного
профиля 18/2; на старых корпусах его правки не изменились. Генератор без ручных
aliases теряет полезные исправления и иногда убирает нужного конкурента.
Даже exact может поглощать правильную соседнюю букву. Генерация остаётся
отдельно включаемой настройкой; реальное качество Handy не измерено.

## Фразы и пользовательские примеры

`kind = "phrase"` разрешает 2–3 выходных слова: `долгождать → долго ждать`.
Сохранённые края фразы не входят в правку, поэтому `проковол колесо` меняет
только первый токен. `known_words = "preserve"` защищает известные слова
внутри фактического edit и допускает известные соседние слова.

```bash
rechprav/target/release/rechprav terms-inspect 'долгождать' --config rechprav/configs/terms-phrases-canonical.toml
rechprav/target/release/rechprav terms-inspect 'без снятия сваты' --config rechprav/configs/terms-user-phrases.toml
python3 rechprav/scripts/run_user_phrase_experiment.py rechprav/experiments/my-user-phrase-run
```

[Опыт 010](experiments/010-user-phrases/report.md) отдельно оценивает девять
предоставленных пользователем пар и 43 синтетических контроля. С запомненными
aliases исправлены 9/9 пользовательских строк; без них — 3/9 при edit-1.
На контролях остаются четыре FP даже с защитой слов и контекстом `сваты → слота`.
Это не оценка будущих диктовок. Сравниваются размеры пакетов выражений 3/12/40,
веса, выключатели и конфликтующие наборы. Формы `слот`/`слота`, `таб`/`табы`
требуют явного задания; с этапа 011 ими можно управлять как одним термином. Подробности и ограничения — в [terms.md](../docs/rechprav/terms.md).

## Один термин с несколькими формами

`forms` объединяет базовую canonical и явно заданные словоформы. У них общий
вес и выключатель, у каждой формы могут быть свои aliases и условия контекста.
Все активные правильные формы защищаются, включая отсутствующие в словаре.
Автоматического склонения пока нет. Пример и правила наследования описаны в
[настройках](../docs/rechprav/terms.md#семейства-явных-форм-011).

```bash
rechprav/target/release/rechprav terms-inspect 'в свате' --config rechprav/configs/terms-families.toml
python3 rechprav/scripts/run_family_experiment.py rechprav/experiments/my-family-run
```

[Опыт 011](experiments/011-term-families/report.md) сравнивает группировку
с эквивалентными отдельными записями, изменение общего веса, удаление форм,
выключение термина/набора, контекст форм и конфликтующие словари.

## Подозрительные исходные слова и обратная связь

Опциональные `source_penalties` понижают предпочтение сохранения указанного
слова или 2–3 слов, глобально либо в заданном контексте. Вес цели задаётся
отдельно. Совпадающие штрафы берутся по максимуму; защиты и конфликты сохраняются.

```bash
rechprav/target/release/rechprav terms-inspect 'сняли свод' --config rechprav/configs/terms-source-feedback.toml
python3 rechprav/scripts/run_feedback_experiment.py rechprav/experiments/my-feedback-run
```

[Опыт 012](experiments/012-feedback/report.md) сравнивает эти сигналы на
30 технических примерах. Предложен будущий процесс обучения по подтверждённой
правке редактора: событие → отключаемое предложение пары/контекста → проверка
сохранности обычной речи → принятие/отмена. Интеграция и автоматическое обучение
пока не выполнены. [Настройки](../docs/rechprav/terms.md#понижение-предпочтения-исходных-слов-012).

## Взвешенная дистанция

В edit-поиске можно включить `policy.weighted_distance`: задать цены отдельных
направленных буквенных замен, вставки и удаления. Лимит для известных слов
остаётся в обычных правках. По умолчанию модель отключена; её стоимости —
проверяемая эвристика, не вероятности ASR.

```bash
rechprav/target/release/rechprav terms-inspect 'клиппорт' --config rechprav/configs/terms-weighted.toml
python3 rechprav/scripts/run_weighted_experiment.py rechprav/experiments/my-weighted-run
```

[Опыт 013](experiments/013-weighted/report.md) сравнивает семь профилей на новых
искажениях и отдельно на девяти пользовательских парах, с неизменными aliases
и защитами. Включены отдельные измерения warm analyze.
[Формат и ограничения](../docs/rechprav/terms.md#взвешенная-буквенная-дистанция-013).

## Организация

`src/` — библиотека и CLI; `tests/` — проверки поведения, безопасности вывода и метрик;
`data/fixtures/` — технические примеры; `experiments/` — сохранённые прогоны.
Опыты не меняют сервер HandyProcessor до этапа интеграции.

Словари и модели собираются заранее в `artifacts/`; runtime работает без сети.
Приватные данные и построчные отчёты помещаются в ignored private-каталоги.
В отслеживаемых экспериментах сохраняются воспроизводимые команды, конфиги,
хеши ресурсов, агрегированные результаты и разрешённые примеры.

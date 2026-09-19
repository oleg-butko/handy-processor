# 001 — проверка технического каркаса и KEEP

Дата: 2026-09-19. Код: `8be44bf`, ветка `feat/rechprav-foundation`.
Тип опыта: проверка инфраструктуры, не сравнение качества коррекции.
Пользователь явно выбрал технические примеры до подготовки реальных пар.

## Гипотеза

Можно сохранить исходный текст и связь позиций после токенизации/переноса,
получить ожидаемые контрольные метрики и обнаружить некорректную разметку
до подключения исправляющего алгоритма.

## Данные и параметры

- 67 искусственных примеров с ожидаемыми lexical tokens: Unicode, пунктуация,
  дефисы, числа, пути, URL, латиница, C++, регистр и управляющие символы.
- 12 искусственных reviewed-записей, split=dev; 37 source и 37 gold tokens.
- 6 ошибок 1→1 и 2 compound spans. Настоящей истории Handy здесь нет.
- KEEP, NFC + lowercase + normalize_yo; width=130; без словаря и обучения.
- Rust/Cargo 1.98.0; Debian VM, CPU Xeon E5-2678 v3, 16 логических CPU.
  Дополнительные сведения и хеши: [environment.json](environment.json).

## Воспроизведение

Из корня репозитория, при уже загруженных зависимостях:

```bash
/home/user1/.local/bin/cargo test --manifest-path rechprav/Cargo.toml --locked --offline
/home/user1/.local/bin/cargo clippy --manifest-path rechprav/Cargo.toml --locked --offline --all-targets -- -D warnings
/home/user1/.local/bin/cargo fmt --manifest-path rechprav/Cargo.toml -- --check
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline
rechprav/target/release/rechprav validate rechprav/data/fixtures/corpus.jsonl
mkdir -p rechprav/artifacts
rechprav/target/release/rechprav prepare rechprav/data/fixtures/review-input.txt --output rechprav/artifacts/001-pending.jsonl --group technical-pilot --kind synthetic
rechprav/target/release/rechprav review rechprav/data/fixtures/review-input.txt --output-dir rechprav/artifacts/001-review
rechprav/target/release/rechprav benchmark rechprav/data/fixtures/corpus.jsonl --output-dir rechprav/artifacts/001-keep
```

Выходные имена должны быть новыми; существующие результаты не перезаписываются.
На другой машине заменить путь cargo. Сравнивать metrics, predictions, review
и corrected; пути, время запуска и длительность в manifest естественно отличаются.
Хеши исходников встроены в бинарник при компиляции, а не вычисляются по текущему
содержимому checkout. Это позволяет отличить устаревший бинарник от текущего кода.

## Получено

19 тестов проходят (15 библиотечных integration tests и 4 CLI tests).
Один из тестов проверяет все 67 случаев токенизации. Clippy без предупреждений,
rustfmt и release build проходят. Дополнительные ручные проверки:
байтовое равенство input/corrected, соответствие predictions исходнику,
совпадение встроенных source hashes с файлами и восстановление source из debug.

| Метрика | Значение |
|---|---:|
| TP / FP / FN для 1→1 | 0 / 0 / 6 |
| Correct input / harmed correct input | 28 / 0 |
| Precision | null |
| Recall / F0.5 / F1 / FCR | 0 / 0 / 0 / 0 |
| Unsupported gold spans | 2 |
| WER errors: S / D / I | 8 / 1 / 1 |
| WER до / после | 10/37 / 10/37 |

Артефакты: [метрики](keep/metrics.json), [предсказания](keep/predictions.jsonl),
[валидация](validation.json), [review](review/review.txt),
[debug](review/debug.jsonl), [pending annotations](pending.jsonl).
Файлы review намеренно сохраняют пробелы исходных участков, включая пробелы
перед визуальным переносом. Corrected хранит исходный текст побайтно.

## Ограничения и решение

KEEP ничего не исправляет; его нулевой FCR не свидетельствует о полезности.
Поле `evaluation_wall_ns` измеряет проверку корпуса и расчёт метрик, не скорость
коррекции. RAM, качество кандидатов, доверительные интервалы и сравнение индексов
ещё не исследовались. Для синтетического корпуса не делаются выводы о Handy.

Первичная загрузка зависимостей через недоступный локальный proxy не удалась;
повторная загрузка без этих proxy-переменных только для команды прошла. Сборка
и runtime затем работали локально. Текущие правила сервера не менялись.

Решение: инфраструктура годится для следующего опыта с индексированным OOV
baseline. Полный этап 1 остаётся незавершённым до реального корпуса/разбиения.
Следующим алгоритмом будет Levenshtein distance=1 с отказом от сомнительных
замен; выбор SymSpell/FST и настройка качества ещё впереди.

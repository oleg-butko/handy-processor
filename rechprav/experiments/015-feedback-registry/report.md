# 015 — реестр принятия с отзывом одобренных evidence

2026-09-22, ветка `feat/rechprav-feedback-registry`.
Протокол/16 сценариев зафиксированы до реализации: `87000b7`.
Код, восемь тестов и runner: `0f7dd01`. На момент запуска implementation_status
пустой; runtime binary и его исходники совпадают с 014/013.

## Что проверяли

Новый этап управляет жизненным циклом правил, не меняет алгоритм коррекции.
Один JSONL содержит correction/retract, другой — явные accept/reject/disable.
Решение ссылается на historical checkpoint и proposal id; approved evidence
восстанавливается из этого checkpoint. В active.toml попадает правило только
при пересечении одобренных и текущих активных подтверждений.

Стабильного id недостаточно: повторная пара после отмены имеет прежний id, но
новый evidence id. Проверка `reborn-stale` оставляет такое правило выключенным,
а `reborn-accepted` возвращает его после нового accept. Это выбранное поведение;
автоматическое наследование старого принятия отвергнуто по этому контрпримеру.

Корпус тот же, что в 014: 28 synthetic/dev строк, 15 gold spans и 13 KEEP строк.
Это не новый holdout и не оценка реальной истории Handy. Проверка смены reference
config дополняет 16 заранее зафиксированных сценариев до 17 профилей.

## Результаты

| Сценарии | Активных правил | TP / FP / FN | Точных строк |
|---|---:|---:|---:|
| accepted, duplicate-decision, approved-repeat, reborn-accepted, reenabled, conflict-unapproved | 8 | 9 / 1 / 6 | 21 |
| unapproved-repeat, undo-last, reborn-stale, disabled, rejected, new-context | 7 | 6 / 0 / 9 | 19 |
| conflict-accepted | 9 | 6 / 0 / 9 | 19 |
| unreviewed, other-unreviewed, stale-environment | 0 | 0 / 0 / 15 | 13 |
| other-accepted | 1 | 0 / 4 / 15 | 12 |

Все 17 predictions.jsonl и metrics.json побайтно совпали с соответствующим
прежним профилем 014: accepted/undo-last/conflict/disabled/other-scope. Полный
принятый config структурно совпал с accepted config 014. Дубликаты решений,
дополнительные подтверждения и частичная отмена не увеличивают вес.

`approved-repeat` показывает два evidence до принятия: отмена первого оставляет
второе и восемь правил. В `unapproved-repeat` второе событие добавлено после
review; после отмены первого остаются семь правил и `stale_evidence` для слот.
`new-context` не наследует принятие старого контекста. `other-unreviewed` не
наследует решения technical; `other-accepted` содержит только правило свитч.
Его 4 FP измерены против technical gold: это контроль неверного выбранного домена,
не качество коррекции в другом домене. Три FP попадают на уже ошибочные spans,
один — на правильный вход, поэтому harmed_correct_input=1.

Непринятый конкурент свитч не попадает в профиль и не меняет слот. При явном
принятии обоих существующий runtime оставляет KEEP на конфликтующих spans.
Смена fingerprint окружения даёт stale_environment для всех восьми решений и
выключенный профиль до review. Более поздний disable не отменяется повторной
доставкой старого accept (отдельный unit test).

Единственный FP принятого technical профиля прежний: буквальное упоминание
«обсуждаем выражение сняли свод памяти». Реестр его не устраняет. Снятие правила
слот убирает этот FP и одновременно три TP; это эффект отзыва, не улучшение
распознавания. Автоматические веса и глобальное понижение вероятности свод
в этом этапе не добавлялись.

## Проверки и артефакты

- 127 тестов (119 прежних + 8 новых), clippy с `-D warnings`, fmt, offline release
  example build, Python py_compile прошли. Первое clippy замечание касалось
  ненужного clone в тестовых срезах; исправлено до прогона, сценарии не менялись.
- В каждом benchmark независимо пересчитаны TP/FP/FN/harmed positions,
  проверены id/complete и восстановление UTF-8 результата по spans.
- Проверены 39 source manifests: 22 registry (5 review + 17 exports) и 17 benchmark;
  180 hashes ресурсов. Все raw input/decision/output hashes проверены.
- Подмена принятого исторического префикса и конфликтующий decision id отклонены
  до создания output; повтор экспорта в существующий каталог его не изменил.
- Корректор не менялся: SHA256 runtime
  `93199bc0ae9c94b7f5721ab4d5ce28384529bac9aebac9e9f9cd978555694e6b`.
  Вся старая матрица 169 запусков повторно не выполнялась.

[Сводка](summary.json), [сравнение](comparison.csv), [построчно](case-comparison.csv),
[окружение/hashes](environment.json), [команды](commands.json),
[отказы CLI](invalid-checks.json), [принятое состояние](registry/accepted/report.json),
[вернувшаяся пара без нового принятия](registry/reborn-stale/report.json),
[ошибки принятого профиля](benchmark/accepted/errors.json).

## Воспроизведение и решение

Из корня репозитория, в новый output directory:

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --example feedback_registry
python3 rechprav/scripts/run_registry_experiment.py rechprav/experiments/my-registry-run
```

Нужны прежние локальные ресурсы OpenCorpora/wordfreq и сохранённые артефакты 014.
Manifest содержит абсолютные пути данной VM; runner создаёт новые на своей машине.

Принят offline-пересчёт с явным review, approved evidence и отдельными scopes.
Смена любого исходника сборки/config консервативно инвалидирует принятие;
семантическая миграция окружения отложена. Общий словарь и рабочие настройки не
менялись; `first-plan.md` сохранён без изменений. Сеть/системные изменения не нужны.

Active.toml содержит только generated rules, не объединяет reference manual
terms. Экспорт не обновляет уже загруженный профиль. Следующий небольшой этап —
адаптер событий редактора и надёжная запись/replay, затем проверка объединения
профилей и атомарного обновления. Независимая разметка реальных ошибок по-прежнему
нужна для выбора качества и настройки весов.

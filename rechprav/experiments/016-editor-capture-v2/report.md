# 016 — транзакционная запись исправлений редактора

2026-09-22, ветка `feat/rechprav-editor-capture`. Протокол и fixtures frozen
до реализации в `2e5cec0`; код `3b68fd5`, граница открытия БД усилена в `7d47d6c`.
Во время обоих прогонов implementation_status пустой. Первый прогон сохранён
в [016-editor-capture](../016-editor-capture/report.md), здесь итоговый.

## Что выяснилось и что реализовано

Существующий PlainTextEditor отдаёт конечные snapshots, а VoiceEditorPage
объединяет автосохранения. Backend хранит непрозрачную историю undo/redo. Эти
данные не подтверждают, что конкретная замена исправляла ASR; получение gold
через diff автосохранений отвергнуто. UI/сервер/рабочую БД не меняли.

Сделан отдельный Node/SQLite адаптер для явно переданного ограниченного окна
текста: before/after, optional UTF-16 selection, confirmed_asr, scope, request_id,
fragment_id, expected_revision. Node выбран для стыка с существующим backend,
уже использующим node:sqlite; алгоритмы коррекции и реестр остаются Rust.
Новых зависимостей/сетевых запросов нет.

BEGIN IMMEDIATE и FULL synchronous сохраняют request/event/receipt/frame revision
одной транзакцией; undo дополнительно сохраняет retraction. Повтор того же id и
payload возвращает прежний receipt; изменённый payload, stale revision и неверный
before отвергаются без частичной записи. JSONL — детерминированный экспорт
согласованного snapshot, не канонический журнал, требующий ремонта оборванных строк.

## Сквозные сценарии

Зафиксированы семь исходных запросов: слот, клипборд, долго ждать, ПНЗ,
неподтверждённая стилистика, свитч в другом scope и слот после emoji. Они проходят
реальную CLI-запись в экспериментальную БД; каждый вызов открывает её заново.
Результат экспорта точно совпадает с заранее ожидаемыми correction/retract events.

Предложения technical: четыре правила; слот имеет два evidence (`emoji`, `slot`).
Selection после emoji конвертируется из UTF-16 9..13 в UTF-8 16..24. Стилистика
помечена not_confirmed_asr; свитч не попадает в technical. Повтор slot не добавляет
строку и даёт побайтно идентичный экспорт.

После явного принятия четырёх правил последовательно отменены оба подтверждения
слот. Активный профиль теряет это правило. Redo создаёт новое evidence `redo-slot`;
старое принятие получает stale_evidence. Новое принятие возвращает правило.
В другом scope есть только свитч, технические решения его не принимают.

На прежнем корпусе 014, 28 synthetic/dev строк, 15 gold spans:

| Профиль | Правил | TP / FP / FN | Точных строк |
|---|---:|---:|---:|
| accepted | 4 | 6 / 1 / 9 | 18 |
| undo | 3 | 3 / 0 / 12 | 16 |
| redo-stale | 3 | 3 / 0 / 12 | 16 |
| redo-accepted | 4 | 6 / 1 / 9 | 18 |

Эти числа не следует сравнивать с 9 TP/1 FP полного профиля с восемью правилами
014/015 как изменение алгоритма: здесь явно выбраны только слот, клипборд,
долго ждать и ПНЗ. Каждый active config совпал со соответствующим подмножеством
старого accepted config 014. Accepted/redo-accepted и undo/redo-stale попарно
побайтно совпали по predictions/metrics. TP/FP/FN/harmed positions и UTF-8
восстановление независимо проверены runner. Старый FP буквального упоминания
«обсуждаем выражение сняли свод памяти» остаётся. Улучшение качества не заявляется.

## Надёжность и повторный прогон

Прошли 16 Node-тестов, включая:

- Повтор после restart, перестановку JSON-ключей и конфликт payload с тем же id.
- Stale/future revision, неверный before, чужой fragment/scope, неправильную
  обратную пару и попытку отменить не последнее активное изменение.
- Invalid Unicode, surrogate split, неверную реконструкцию after, неизвестные
  поля/лимиты и невалидный UTF-8 JSON на входе CLI.
- Реальный SIGKILL после insert, перед commit, после commit до receipt. В первых
  двух случаях запись и frame отсутствуют; в последнем повтор даёт replay.
- Отдельный SIGKILL внутри undo: retraction, request и frame откатываются вместе.
- Два конкурентных процесса: разные изменения одной revision дают один commit
  и один отказ; одинаковая доставка даёт один commit и один replay.
- Согласованный повторяемый экспорт, отказ перезаписать каталог и отказ открыть
  тестовую чужую SQLite-базу без изменения её bytes. Integrity_check после
  испытанных process crashes возвращает ok.

После первого успешного прогона просмотр кода показал, что проверять application_id
через SQLite поздно: открытие чужой БД может запустить recovery. В `7d47d6c`
заголовок проверяется обычным read-only чтением до передачи пути SQLite, затем
повторно через PRAGMA. Реальная чужая БД не использовалась и не менялась.
Повторены 16 тестов и полный сквозной прогон. [19 файлов](previous-run-equality.json)
совпали с первым прогоном: inputs, receipts, events/snapshots, predictions/metrics.
Это исправление границы ввода, без подгонки fixtures или качества.

Проверены 11 Rust source manifests (7 registry + 4 benchmark), 55 resource hashes,
4 capture manifests и hashes восьми Node/Python исходников. Rust/runtime/registry
не менялись: бинарники совпадают с 015. 127 Rust-тестов, clippy/fmt были пройдены
в 015; здесь их повторно не запускали, проверены новые Node-тесты, node --check,
Python py_compile и сквозные вызовы имеющихся Rust-бинарников.

[Сводка](summary.json), [построчно](case-comparison.csv),
[TAP тестов](capture-tests.tap), [receipts](receipts.json),
[исходные сценарии](frozen-inputs.json), [окружение/hashes](environment.json),
[команды](commands.json), [экспорт](capture/initial/events.jsonl),
[состояние после redo до принятия](registry/redo-stale/report.json).

## Воспроизведение и вывод

```bash
node --test rechprav/editor/capture-store.test.mjs
python3 rechprav/scripts/run_capture_experiment.py rechprav/experiments/my-capture-run
```

Нужны доступный Node с node:sqlite, собранные Rust CLI/feedback_registry и
локальные ресурсы OpenCorpora/wordfreq предыдущих этапов. Runner сам создаёт
новую generated SQLite-базу в ignored artifacts и сохраняет её путь в environment;
бинарные БД не добавляются в Git. Экспорт и решения сохраняются в новый каталог.

Принят прототип транзакционной записи и явного подтверждения ASR. Отвергнуто
обучение из автосохранений; append в JSONL без транзакции для source of truth
не выбран. SQLite опирается на штатный механизм восстановления; power-loss
и порча диска не моделировались. Atomic installation профиля не реализована.

Ограничения намеренные: окно ≤128 Unicode scalars, no auto window extraction,
собственная revision фрагмента, только точный undo последнего активного replace.
Нельзя считать это общей поддержкой undo CodeMirror. Живой capture/UI/outbox,
объединение generated/manual rules и обновление работающего приложения впереди.
Следующий небольшой этап — композиция профилей с весами/выключателями/конфликтами,
затем атомарная публикация проверенного снимка.

[Формат и дальнейшие подзадачи](../../../docs/rechprav/editor-capture.md).

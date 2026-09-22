# 017 — композиция ручного профиля и принятых исправлений

2026-09-22, ветка `feat/rechprav-profile-composition`. Протокол `9640bf1`,
Rust-композитор/восемь тестов `aacba3a`, исправление runner `a456c62`, исправление
gold `2d34543`. Последний commit указан в environment; исходники реализации
чистые на момент запуска. Runtime не менялся.

## Выбранный способ

Один Config и одно ранжирование TermEngine. Каждое принятое правило требует
явной привязки к ручному термину или owner=null для новой цели. Владелец передаёт
вес и выключатели; generated запись не изменяет его исходные aliases/forms.
Known-word gates пересекаются, context exclusions сохраняются, anchored patterns
пересекаются вместо объединения через OR. Удалённый/выключенный владелец не
превращается автоматически в независимый новый термин.

Это ограниченный композитор: feedback добавляется только в exact mode. При
manual edit mode или свободных context cues сохраняются ручные определения,
но соответствующий feedback пропускается с причиной в audit. Простое смешивание
двух конфигов не обеспечивает этих свойств; расширять генерацию exact alias
глобальным edit mode не принято. Универсальный композитор всех режимов не заявлен.

## Данные и исправления попыток

21 frozen сценарий плюс проверка stale-settings; 20 authored synthetic/dev строк,
8 gold spans и 12 KEEP строк. Четыре synthetic события: слот, слота, КТХ, долго
ждать. Есть повторные пользовательские примеры, поэтому это не независимый test.
Для каждого manual/binding варианта отдельно создаётся review/accept. Так
выключатели проверяются самой композицией, а не только stale guard реестра 015.

Сохранены все попытки:

1. [017-profile-composition](../017-profile-composition/report.md): состояния
   композитора проверены, runner остановился до benchmark на коллизии имени
   `duplicate-bindings.json`. Файл не перезаписан; для invalid fixture выбрано
   отдельное имя. Алгоритм не менялся.
2. [017-profile-composition-v2](../017-profile-composition-v2/report.md): полный
   прогон, но isolated «долгождать» ошибочно размечено KEEP. Это противоречило
   пользовательской паре «долгождать → долго ждать». Нельзя подменять правильный
   текст желательным консервативным поведением контекстного правила.
3. Здесь gold исправлен и метрики пересчитаны. Events/configs/алгоритм не менялись:
   [45 конфигов и 440 результатов коррекции](previous-run-equality.json) совпали
   с v2 (у prediction records исключены только gold и tags). Совпадающие источники
   также [сверены с пользовательским корпусом](user-gold-consistency.json).

## Результаты

| Профили | Добавленных правил | TP / FP / FN |
|---|---:|---:|
| manual-only, feedback-off, stale-settings | 0 | 2 / 0 / 6 |
| composed, duplicate, owner-high-conflict | 4 | 7 / 1 / 1 |
| binding-off, undo, new-over-existing | 3 | 5 / 0 / 3 |
| conflict, excluded | 4 | 5 / 0 / 3 |
| owner-off, owner-deleted | 2 | 3 / 0 / 5 |
| owner-low | 4 | 3 / 0 / 5 |
| new-low | 4 | 6 / 1 / 2 |
| known-never | 4 | 4 / 0 / 4 |
| source-penalty | 4 | 5 / 1 / 3 |
| unsupported-edit (только ручной edit profile) | 0 | 4 / 0 / 4 |
| global-off | 0 | 0 / 0 / 8 |

Полная таблица всех 22 профилей — [comparison.csv](comparison.csv).

Основной профиль сохраняет две ручные замены и добавляет пять верных spans.
FP остаётся на «обсуждаем выражение сняли свод памяти»: соседние слова не
определяют буквальный/предметный смысл. FN — isolated «долгождать», потому что
принятая пара требует `надо {term} ответа`. Ослабление этого контекста здесь
не подбиралось. Manual edit profile умеет исправить split вне этого контекста;
этот результат сохранён, его преимущество на данном случае не скрывается.

Проверено влияние настроек:

- Выключение owner убирает generated записи для слот и слота. Выключение set
  дополнительно убирает ручной клипборд. Удаление owner даёт owner_missing.
  Binding owner=null для уже существующего слот получает отказ добавления.
- Низкий owner boost=-5 влияет и на ручной alias, и на learned alias/форму;
  число evidence его не компенсирует. New-term boost=-5 подавляет КТХ отдельно.
- Конкурирующие слот/свитч при равном score дают KEEP. Повышение owner boost=3
  выбирает слот, но возвращает и буквальный FP. Это предпочтение, не уверенность.
- Source penalty=5 для свод в заданном контексте компенсирует owner boost=-5
  именно на этом source; другие aliases и форма остаются пониженными. Gates
  known_words=never и exclusions остаются отдельными ограничениями.
- Пересечение `снятия {term}` и `без снятия {term}` даёт второе. «Без снятия сваты»
  исправляется, «после снятия сваты» сохраняется. Несовместимые patterns блокируют
  добавление; optional manual patterns тоже становятся дополнительным gate.
- Retract убирает generated свод→слот, ручной свуты→слот остаётся. Duplicate не
  меняет config/predictions/metrics. Изменённый binding со старыми решениями
  получает stale_environment и побайтно повторяет manual-only prediction/metrics.

Не выбран «лучший корректор» по этим 20 строкам. Метрики измеряют действие
конкретных настроек, профили намеренно имеют разную область применения. Обычные
слова не получили общей гарантии: остаётся FP, а новые canonical/конкуренты
могут влиять на ранжирование и защиту в существующем runtime. Сохраняются ручные
определения, а не обещание неизменности всех их результатов при добавлении правил.

## Верификация и воспроизведение

135 Rust-тестов (127 прежних + 8 новых), clippy `-D warnings`, fmt, offline release
example build и Python py_compile прошли. [Лог тестов](rust-tests.log).
Все 22 benchmarks независимо проверены по exact spans TP/FP/FN/harmed positions,
complete/id и восстановлению UTF-8. Manual definitions/policy/source penalties
сохранены в каждом профиле; итоговый TOML равен report.config. Invalid duplicate
binding отвергается до создания output directory.

Проверены 66 source manifests (44 composition/review + 22 benchmarks), 325 hashes
ресурсов, input/decision/binding/output hashes. Disabled benchmark не загружает
baseline. Runtime SHA256 совпадает с 013–016:
`93199bc0ae9c94b7f5721ab4d5ce28384529bac9aebac9e9f9cd978555694e6b`.
Прежняя большая матрица не повторялась, старые Node-тесты 016 тоже не менялись.

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --example feedback_compose
python3 rechprav/scripts/run_composition_experiment.py rechprav/experiments/my-composition-run
```

Нужны готовый runtime CLI и прежние локальные ресурсы OpenCorpora/wordfreq.
Output directory должен быть новым. [Команды](commands.json), [окружение/hashes](environment.json),
[построчное сравнение](case-comparison.csv), [audit основного профиля](composed/composed/report.json),
[отказ invalid binding](invalid-check.json), [контракт](../../../docs/rechprav/profile-composition.md).

Принята ограниченная exact-композиция с owner/provenance. Не приняты неявное
копирование целей, объединение контекстов через OR и потеря выключателей.
Полный fingerprint консервативно требует review даже после небольших изменений
настроек; смягчение этой зависимости требует отдельного опыта. Следующий этап —
атомарная публикация проверенного снимка с контролем поколения и восстановлением.
Рабочее приложение/его настройки, first-plan.md и системная конфигурация не менялись.

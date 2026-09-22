# 020: ручной edit и точные выученные aliases

Дата: 2026-09-22. Ветка `feat/rechprav-mixed-generation`.
Замороженные протокол/controls: `a124227`. Реализация: `66a5cb2`.

```bash
python3 rechprav/scripts/run_mixed_experiment.py rechprav/experiments/020-mixed-generation
```

## Гипотеза и изменение

В 017 общий exact сохранял точность принятых aliases, но не позволял одновременно
использовать edit-поиск ручных записей. Теперь term.generation=exact ограничивает
только этот term; default inherit сохраняет прежнюю глобальную политику. Ветка
exact проверяется до weighted/edit и сохраняет границы слов. Композитор выставляет
exact каждому добавленному правилу, снимая прежний отказ unsupported_mode.

Ни веса, ни пороги, ни словарь не подбирались заново. Ручные definitions, gates,
context/exclusions, source penalties и общий budget сохранены. Формы наследуют
generation. Свободные context cues остаются неподдержанными в композиции.

## Фиксированные данные и сравнение

30 synthetic/dev строк: прежние 20 controls 017 плюс 10 новых. Всего 13 gold spans
и 17 KEEP строк. Новый подтверждённый alias — `пэ эн зэ` в контексте «проверь …
сегодня». Слитные варианты и иной контекст сохранены как положительные gold,
а не превращены в KEEP ради улучшения метрик. Это не независимая история Handy.

| Профиль | TP | FP | FN |
|---|---:|---:|---:|
| manual exact | 2 | 0 | 11 |
| composed exact | 8 | 1 | 5 |
| manual edit | 5 | 0 | 8 |
| **mixed: manual edit + generated exact** | **10** | **1** | **3** |
| mixed + uniform weighted distance-1 | 10 | 1 | 3 |
| diagnostic unpinned generated | 12 | 2 | 1 |

Mixed добавляет относительно composed exact две полезные коррекции: isolated
«долгождать → долго ждать» и «калиборт → клипборд». Принятые aliases, включая
«свод → слот», «сваты → слота» и «пэ эн зэ → ПНЗ», сохраняют свою точность.
Для неперечисленных «катэхи», «пээнзэ», «пэ энзэ» generated кандидат не создаётся.

Оставшиеся ошибки mixed:

- FP: «обсуждаем выражение сняли свод памяти» меняет буквальное упоминание.
- FN: «проверь пээнзэ сегодня» и «проверь пэ энзэ сегодня»: exact не расширяет
  трёхсловный alias на другие границы слов.
- FN: «проверь пэ эн зэ завтра»: не выполнен обязательный контекст «… сегодня».

Unpinned исправляет два слитных варианта ПНЗ, но дополнительно меняет «включи
катэхи» в КТХ, вопреки gold этого авторского контроля. Он имеет выше recall;
mixed не объявляется победителем по всем метрикам. Unpinned — только диагностический
TOML: снятое ограничение не соответствует composition manifest и не публикуется.
Uniform weighted здесь не улучшил качество; другие стоимости не настраивались.

## Настройки пользователя

Всего 20 профилей. Полная таблица в comparison.csv; построчный вывод в
case-comparison.csv. Выключатели global/set/owner/binding/feedback не восстанавливают
отключённые правила. Owner-low подавляет ручной и generated варианты владельца,
new-low — новые термины. Равный конкурент для слота даёт KEEP (8/0/5);
owner-high-conflict возвращает 10/1/3. Known-never и exclusions продолжают
запрещать замены; penalty не обходит эти gates. Undo удаляет только learned alias;
duplicate не добавляет голоса и побайтно совпадает с mixed.

## Совместимость и сквозной путь

У 22 прежних профилей 017 и семи weighted/edit профилей 013 без generation
побайтно совпали predictions.jsonl и metrics.json, включая трассы решений.
Старый binary сохранён до изменения runtime в ignored artifacts; hashes есть
в environment.json. Исторические результаты не переписаны.

Fingerprint runtime изменился. Проверка на ровно тех же старых input/manual/
bindings/decisions 017 получила stale_environment и ноль добавленных правил.
Старый publication export отклонён по compiled sources. Для текущих 20 профилей
созданы новые synthetic review/accept decisions; guard не отключался.

Mixed и global-off опубликованы в отдельную SQLite-базу. Reader удержал старый
mixed handle после выключения; 60 анализов, edits/decisions/comparisons/complete
совпали с соответствующими benchmark outputs. Новые handles видят поколение 2,
старые — поколение 1. Это прежняя однопоточная модель, не параллельный сервер.

Прошли 147 Rust-тестов (пять новых), 19 Node-тестов публикации, включая шесть
реальных SIGKILL и отказ историческому producer export. Также прошли clippy
all-targets с `-D warnings`, fmt, offline release builds и py_compile.
Проверены 91 source manifest и 445 resource hashes, исходники прогона чистые.
Дополнительная проверка сняла generation с mixed bundle и пересчитала output hash:
validator всё равно отклонил config, отличающийся от replay. Пакет и отказ
сохранены в publication/unpinned-bundle.json и unpinned-rejected.json.

Новые unit tests и полный опыт прошли с первого запуска. Технические неподходящие
варианты здесь — unpinned как default и uniform weighted как способ улучшить
качество на этом наборе; отказ от них обоснован таблицей, не сбоем реализации.
Сеть не использовалась, first-plan не менялся. Исторические runners 017–019
для точного повторения запускать на их commits; нынешний сквозной путь — 020.

## Решение

Принять generation=exact для generated правил: это позволяет совместить ручной
edit-поиск с границами явно принятой пары. Автоматическое наследование нечёткого
поиска generated не принимать как default — оно расширяет пользовательское
разрешение и добавило FP в этом опыте. Для повышения recall нужен отдельный
управляемый каталог aliases/контекстов, а не только больший вес.

Следующий 021 — предложения дополнительных произношений и разделений аббревиатур
с явным подтверждением и новыми заранее размеченными controls. Метаязыковой FP
остаётся отдельной задачей; обычные контекстные templates его пока не различают.
Рабочее приложение не подключено, настройки/БД HandyProcessor не менялись.

[Политика и ограничения](../../../docs/rechprav/mixed-generation.md),
[метрики](summary.json), [совместимость](legacy-equality.json),
[публикация](publication/mixed-snapshot.json), [среда](environment.json).

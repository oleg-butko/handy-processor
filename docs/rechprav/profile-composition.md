# Композиция ручного профиля и принятых исправлений

Этап 017 добавляет самостоятельный Rust example `feedback_compose`.
Он пересчитывает [реестр 015](feedback-registry.md) из актуальных событий и решений,
затем создаёт один Config для существующего TermEngine. Ручные определения не
перезаписываются. UI, работающий сервер и его настройки не меняются.

## Явная принадлежность правила

Binding-файл JSON содержит `enabled` и `bindings`. Пример формы записи:

```json
{
  "enabled": true,
  "bindings": [
    {
      "proposal_id": "fb_0123456789abcdef0123456789abcdef",
      "enabled": true,
      "owner": {"set_id": "hardware", "term_id": "slot"},
      "boost": 0.0
    }
  ]
}
```

Proposal id нужно взять из фактического draft/report; пример id выше условный.
Для совершенно нового термина задаётся **явное** `owner: null`; тогда boost
binding управляет его весом. Если owner указан, boost binding должен быть 0:
используются term boost и set boost ручного владельца плюс global policy boost.
Отсутствие поля owner не считается разрешением создать новый термин.

Сопоставление со всеми ручными canonical/forms, включая выключенные, не позволяет
обойти владельца через owner=null. Если владелец удалён, прежняя привязка получает
owner_missing; автоматического превращения в новый термин нет. Если владелец
имеет canonical `пнз`, а подтверждена цель `ПНЗ`, привязка блокируется как
owner_target_mismatch: инструмент не меняет подтверждённый регистр молча.

Generated запись добавляется отдельным term с id=proposal id в набор владельца.
Оригинальные aliases/forms/pronunciation владельца остаются без изменений.
Generated term содержит только подтверждённый alias и canonical; генератор
произношения и остальные формы в него не копируются. Новые термины без владельца
попадают в отдельный набор `fb_new_<hash scope>` с set boost=0.

## Что сохраняется и что ограничивается

- Manual config/set/term выключен → соответствующее generated правило не
  добавляется. `settings.enabled=false` выключает весь feedback, сохраняя manual.
  `binding.enabled=false` выключает только выбранное generated правило.
- Изменения требуют новой сборки. Уже загруженный сторонним приложением снимок
  сам не обновляется. Отмена evidence удаляет generated term, но не ручные aliases.
- Manual policy, source penalties и исходные определения sets/terms сохраняются.
  Новое правило участвует в общем ранжировании: конфликт не скрывается порядком
  двух отдельных проходов, повторные evidence не суммируют голоса/вес.
- Пересечение known-word gates выбирает более строгий: Never, Preserve, Context,
  Always. Max known distance остаётся 0. Взвешенное предпочтение не отменяет gate.
- Context patterns пересекаются, а не добавляются как альтернативы. Более длинный
  совместимый левый suffix/правый prefix усиливает ограничение. Например,
  `снятия {term}` + `без снятия {term}` → `без снятия {term}`. Несовместимые
  варианты исключаются; пустое пересечение блокирует добавление.
- Manual patterns даже при optional context трактуются в generated term как
  обязательное дополнительное ограничение. Это консервативное сужение применения.
  Form overrides учитываются. Context exclusions наследуются как veto.
- Свободные cues `context=[...]` пока не поддержаны при добавлении owned feedback:
  невозможно просто переписать их OR-семантику как пересечение anchored patterns.
  Ручной term продолжает работать, generated получает unsupported_context_cues.
- С этапа 020 feedback поддерживает global exact/edit/weighted: каждый generated
  term получает `generation="exact"`. Ручной поиск сохраняет свою политику,
  принятый alias не расширяется. Ранее, в 017, edit давал unsupported_mode;
  это ограничение снято отдельной политикой [генерации](mixed-generation.md).

Существующие score thresholds, context bonus, source penalties применяются ко
всему итоговому профилю. Это настройки пользователя, не вероятности и не
автоматически выученные коэффициенты. Увеличение веса может разрешить конфликт,
но само по себе не доказывает, что выбран правильный смысл.

## Review и выход

Environment включает hashes manual config, binding-файла, ресурсов и исходников
сборки. Старый accept после их изменения становится stale_environment. Поэтому
в матрице эксперимента каждая конфигурация отдельно проходит новый review/accept:
выключатели проверяются композитором, а не только инвалидированием старых решений.
Отдельный сценарий проверяет именно stale после изменения binding.

У предложения два статуса: `registry.states` сообщает принято ли оно, а
`audit` — удалось ли добавить его в итоговую конфигурацию. Принятое предложение
может быть заблокировано владельцем, контекстом или режимом. Audit связывает
proposal id с owner и output set; через registry доступны decision/evidence ids.

| Статус композиции | Значение |
|---|---|
| added | Правило добавлено |
| not_accepted | Нет действующего принятия/evidence |
| unbound | Нет явного binding |
| manual_disabled / feedback_disabled / binding_disabled | Сработал соответствующий выключатель |
| owner_set_disabled / owner_term_disabled | Владелец выключен |
| owner_missing / owner_target_mismatch | Привязка больше не соответствует ручному термину |
| existing_target_requires_owner | Нельзя создать независимую копию существующей цели |
| disjoint_context | Контексты не имеют совместимого anchored pattern |
| unsupported_context_cues | Композитор не умеет сохранить семантику свободных cues |

Некорректный binding/boost, дубли id, namespace collision или превышение пределов
итогового Config — ошибка всей сборки до создания output. Не выбирается случайная
часть правил для обхода лимита. Сохраняется один scope реестра; scope ручных
наборов явно определяется выбранным manual config и привязками.

## Запуск

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --example feedback_compose
python3 rechprav/scripts/run_mixed_experiment.py rechprav/experiments/my-mixed-run
```

Для собственного прогона:

```bash
rechprav/target/release/examples/feedback_compose events.jsonl \
  --config manual.toml --bindings bindings.json --decisions decisions.jsonl \
  --scope technical --output-dir new-composition-directory
```

Сначала получить proposal ids через feedback_draft, определить bindings и запустить
feedback_compose с пустым decisions.jsonl. Проверить `report.json`, затем копировать
нужный token из **registry.review_tokens** в accept decision по формату 015.
После принятия повторить команду в новый output directory.

Выходы: report.json с registry/audit/config, composed.toml и composition-manifest.json
с hashes. Для экспериментальной публикации и загрузки используются протоколы
[018](profile-publication.md) и [019](profile-reader.md). После изменения runtime
в 020 старые exports/reviews требуют новой проверки. Исторический runner 017
воспроизводится на его commit; текущий сквозной опыт — 020.

[Протокол](017-composition-plan.md),
[эксперимент](../../rechprav/experiments/017-profile-composition-v3/report.md).

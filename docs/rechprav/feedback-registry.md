# Реестр принятия offline-предложений

Этап 015, продолжение [черновиков](feedback-drafts.md). Отдельный Rust example
`feedback_registry` читает два локальных JSONL: события correction/retract и
решения accept/reject/disable. Создаёт новый каталог с report.json, active.toml
и registry-manifest.json. Ничего не пишет в журналы, опорный профиль, словарь
или работающий Handy Processor. Активный профиль — пересобираемый снимок.

## Как принимать предложение

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --example feedback_registry
```

Сначала запустить инструмент с пустым файлом решений. Все предложения будут
`unreviewed`, active.toml выключен, а report.json содержит `review_tokens`.
Пути ниже — пример для новых файлов; существующие выходные каталоги запрещены.

```bash
python3 - <<'PY'
from pathlib import Path
Path('rechprav/artifacts/015-empty.jsonl').touch(exist_ok=False)
PY
rechprav/target/release/examples/feedback_registry \
  rechprav/data/fixtures/feedback-drafts/events.jsonl \
  --decisions rechprav/artifacts/015-empty.jsonl \
  --config rechprav/configs/terms-families.toml --scope technical \
  --output-dir rechprav/artifacts/015-review
```

После просмотра конкретного предложения скопировать его token в решение.
Этот демонстрационный шаг явно принимает только свод→слот с контекстом
`сняли {term} памяти`; новое решение имеет новый уникальный id.

```bash
python3 - <<'PY'
import json
from pathlib import Path
r = json.loads(Path('rechprav/artifacts/015-review/report.json').read_text())
p = next(p for p in r['draft']['proposals'] if p['canonical'] == 'слот')
t = next(t for t in r['review_tokens'] if t['proposal_id'] == p['id'])
d = dict(schema_version=1, decision_id='accept-slot-1', action='accept', review=t)
with Path('rechprav/artifacts/015-decisions.jsonl').open('x') as f:
    f.write(json.dumps(d, ensure_ascii=False) + '\n')
PY
rechprav/target/release/examples/feedback_registry \
  rechprav/data/fixtures/feedback-drafts/events.jsonl \
  --decisions rechprav/artifacts/015-decisions.jsonl \
  --config rechprav/configs/terms-families.toml --scope technical \
  --output-dir rechprav/artifacts/015-accepted
```

В будущем запись решений будет выполнять редактор. Здесь экспериментальный
runner явно симулирует выбор пользователя, пользуясь тем же форматом.
Наличие `confirmed_asr=true` у исправления само по себе не принимает правило.

## Формат и состояния

Decision: `schema_version=1`, `decision_id`, `action`, `review`.
Review token: `scope`, `proposal_id`, `checkpoint`.
Checkpoint: `event_count`, `events_sha256`, `environment_sha256`.
Неизвестные поля отвергаются. Hash событий считается по сериализации parsed
`Vec<Event>` через serde_json: пробелы и порядок ключей во входном JSONL не важны,
порядок событий и их содержимое важны. Отдельный manifest фиксирует raw SHA файлов.

Для каждого checkpoint инструмент проверяет совпадение исторического префикса
текущего журнала и заново получает предложения на тот момент. Evidence берётся
из этого replay: в decision нет доверенного списка произвольных подтверждений.
Обновление active.toml требует нового запуска с актуальными журналами.

| Состояние | Значение |
|---|---|
| unreviewed | Для текущего предложения нет решения |
| accepted | Есть accept и хотя бы одно из показанных при принятии evidence ещё активно |
| stale_evidence | Пара существует, но все одобренные evidence отменены; новые требуют review |
| no_active_evidence | Принятое предложение исчезло после отмены подтверждений |
| stale_environment | Окружение review отличается от текущего |
| rejected | Последнее уникальное решение — reject |
| disabled | Последнее уникальное решение — disable |

Только accepted входит в active.toml. При исчезновении правила его canonical
защита тоже исчезает из этого профиля. Повторы/число подтверждений не повышают
вес: exact mode, boost=0, known-word gates и соседний контекст унаследованы из 014.
Без контекста known_words=never. Для возобновления после reject/disable нужен
новый accept с новым decision id; можно снова просмотреть актуальный token.

Последнее уникальное решение определяется порядком строк, отдельно для каждого
(scope, proposal id). Повтор старого decision id с идентичным содержимым не
переставляет решение в конец и не отменяет более поздний disable. Другое
содержимое с тем же id — ошибка всей сборки, даже в другом scope или для
устаревшего решения. Такой же принцип применяется к событиям исправлений в 014.

Идентичная пара после отмены всех подтверждений имеет прежний proposal id,
но новые evidence ids. Старого принятия недостаточно. Если два подтверждения
были показаны при accept и одно отменили, другое сохраняет правило. Если второе
появилось уже после accept, оно не подменяет отменённое одобренное подтверждение.

Environment fingerprint включает hashes исходников сборки, reference config
и baseline resources/metadata. При несовпадении исторический proposal не
переигрывается с новым окружением и не активируется. Даже изменение комментария
config или исходника требует review; миграции старого окружения пока нет.
Journal prefix проверяется и в этом случае. SHA фиксирует целостность, а не
личность принявшего решение: это доверенные локальные файлы, не протокол подписи.

## Границы этапа

- Active config содержит только принятые generated rules. Reference config
  используется для классификации, привязки к baseline и инвалидирования review;
  его ручные правила не объединяются с результатом. Такое объединение и его
  конфликты нужно проверить отдельно перед интеграцией.
- Изоляция scope обязательна; один запуск экспортирует один scope. Непринятые
  конкуренты остаются в draft для review. Два принятых конкурента передаются
  обычному runtime ambiguity/KEEP; реестр не выбирает победителя.
- Checkpoint предполагает сохранённый append-only журнал. Его сокращение или
  изменение подтверждённого префикса — ошибка; ротация/компактизация не реализована.
  Два файла не имеют общего времени/транзакции: это offline replay снимков,
  а не многопользовательский журнал редактора.
- Лимиты: 10000 событий, 1024 решения, 32 различных prefix lengths, 64 пары
  (prefix length, scope), входные файлы ≤64 MiB. Валидация всей истории перед
  экспортом; directory должен быть новым. Сбой диска может оставить неполный
  каталог: атомарная установка активного профиля не заявляется.
- Состояния относятся к актуальному снимку; старый externally loaded active.toml
  сам не выключится после добавления retract. Watcher/hot reload/API захвата
  исправлений/UI/автоматическое изменение весов остаются следующими задачами.

[Протокол](015-feedback-registry-plan.md),
[результат эксперимента](../../rechprav/experiments/015-feedback-registry/report.md).

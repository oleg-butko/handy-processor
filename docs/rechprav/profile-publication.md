# Публикация экспериментального профиля

018 добавляет транспорт готового профиля в отдельную SQLite-базу. Рабочее
приложение её не читает. Rust `profile_validate` повторяет сборку 017; Node
`publication/store.mjs` отвечает за поколения, идемпотентность и транзакции.

## Сборка и воспроизведение

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --example profile_validate --example feedback_compose
node --test rechprav/publication/store.test.mjs
python3 rechprav/scripts/run_publication_experiment.py rechprav/experiments/my-publication-run
```

Runner требует готовых ресурсов и артефактов 017-profile-composition-v3, сохраняет
новый каталог отчётов, а SQLite — в новом ignored каталоге `rechprav/artifacts/`.
Не запускает сервер, не меняет настройки приложения. `target/release/rechprav`
нужен для двух контрольных benchmarks; при необходимости собрать `cargo build
--manifest-path rechprav/Cargo.toml --release --locked --offline`.

CLI:

```text
node rechprav/publication/cli.mjs pack COMPOSITION_DIR MANUAL.toml NEW_BUNDLE.json
node rechprav/publication/cli.mjs create NEW_DB technical
node rechprav/publication/cli.mjs request DB REQUEST.json
node rechprav/publication/cli.mjs publish DB REQUEST_ID
node rechprav/publication/cli.mjs snapshot DB
```

`pack` читает три файла экспорта 017, исходный manual и пути events/decisions/bindings
из manifest. Передаётся именно исходный manual, не `composed.toml`. В 017 manifest
его путь не сохранён, поэтому он указан явно. `pack` собирает пакет и проверяет
лимиты; полная Rust-проверка выполняется при `publish`.

`REQUEST.json` имеет ровно четыре поля:

```json
{
  "schema_version": 1,
  "request_id": "build-42",
  "expected_revision": 0,
  "bundle": {
    "manual_path": "/absolute/path/manual.toml",
    "manual": "точный исходный TOML",
    "events": "точный JSONL событий",
    "decisions": "точный JSONL решений",
    "bindings": "точный JSON настроек",
    "report": "точный report.json",
    "config": "точный composed.toml",
    "manifest": "точный composition-manifest.json"
  }
}
```

Это схема, строки-заглушки не являются рабочим пакетом. Готовые запросы находятся
в [результатах опыта](../../rechprav/experiments/018-profile-publication/requests/initial.json).
Вложенные файлы сохраняются строками: нельзя разобрать/пересериализовать manifest
и считать это теми же байтами. В частности, `1.0` и `1` могут различаться при
проверке метаданных Rust. Семантическая миграция форматов здесь не реализована.

## Поколения и повторы

Одна база обслуживает один scope. `snapshot.desired_revision` — поколение
последнего намерения, `snapshot.active.generation` — последняя успешная публикация.
До первой публикации `active=null`. Неудачная/отменённая сборка оставляет разрыв
между поколениями. Нельзя вычислять expected_revision из active.generation.

1. Получить desired_revision **до** сбора входов. Зафиксировать их в bundle.
2. `request` делает CAS по expected_revision, записывает пакет и повышает desired.
   Повтор с тем же id/payload возвращает прежний receipt. Другой payload запрещён.
3. `publish` читает зафиксированный пакет, запускает Rust вне write transaction.
4. В BEGIN IMMEDIATE снова проверяет desired, одной транзакцией сохраняет результат
   проверки, receipt и active. Новая заявка во время проверки отменяет старую.

Драйвер обязан подавать новую заявку при новых настройках/решениях; наблюдателя
файлов здесь нет. При CAS-конфликте нужно заново получить входы и новое намерение,
а не автоматически заменить expected_revision в старом запросе.

Повтор успешного publish возвращает receipt с `replayed=true` даже после новых
публикаций. Это подтверждение **прошлой операции**, а не текущего active; старое
поколение не активируется повторно. Для осознанного возврата нужен новый request.
Непринятое старое намерение получает `superseded request`.

## Что проверяет Rust

- Hash каждого вложенного входа и обоих outputs по manifest 017.
- Полное совпадение embedded compiled source hashes с producer manifest.
- Валидация manual Config, загрузка baseline и точное совпадение resource metadata.
- Пересчёт environment fingerprint и registry из событий/решений, включая отзывы,
  stale evidence/environment и явно принятые bindings.
- Повторная композиция, совпадение report и TOML с её результатом, Config.validate
  и создание TermEngine из тех же загруженных ресурсов.

Подмена config/report с пересчитанными output hashes не заменяет повторную сборку.
Это проверено отдельным сценарием. Ответ Rust связывает bundle SHA, config SHA,
scope, environment, resource metadata, compiled sources и hash самого валидатора.
Хранилище проверяет привязку ответа к переданным байтам. Бинарник берётся из
фиксированного локального `target/release/examples/profile_validate`.

## Запись, чтение и границы

SQLite application_id=`0x52505031`, schema=1, DELETE journal, synchronous FULL.
Перед открытием проверяется 100-байтовый заголовок read-only: чужая база не должна
попасть под автоматическое recovery SQLite. Создание только exclusive, mode 0600.
Профиль хранится внутри транзакции; нет отдельного частично записанного TOML.
Snapshot API читает state и пакет одной read transaction и проверяет hashes и
связь receipt/validation/active. Прямое изменение таблиц не является API.

Лимиты прототипа: 128 заявок на базу, bundle ≤8 MiB, ids/scope 1–64 ASCII символа
`[A-Za-z0-9_-]`, fatal UTF-8, дополнительные поля request/bundle запрещены.
SQLite ждёт write lock до 5 секунд; Rust-процесс ограничен 60 секундами.
История пока не очищается; у большой базы есть цена хранения повторяющихся пакетов.

Проверки SIGKILL покрывают потерю процесса/ответа, но не сбой питания или носителя.
Хеши обеспечивают проверку связности, не подпись; локальные компилятор/код/БД
считаются доверенными. Словарь и частоты остаются внешними файлами: их hashes
проверены при сборке, но после неё файлы могут измениться. Поэтому этот snapshot
ещё нельзя считать готовым hot-reload engine. Следующий этап должен загружать и
проверять ресурсы, удерживать прежний engine при ошибках и атомарно менять engine
вместе с поколением. Повторную проверку ресурсов нельзя заменить одним receipt.

Публикация не улучшает алгоритм коррекции: ограничения exact-композиции, ложная
замена буквального упоминания и пропущенное isolated «долгождать» из 017 остаются.

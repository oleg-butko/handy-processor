# Reader и время жизни engine

Этап 019 проверяет Rust-модель загрузки профиля из снимка 018. Код:
`rechprav/src/profile_reader.rs`, общий `src/profile_validation.rs`, JSONL-driver
`examples/profile_reader.rs`. Это отдельный экспериментальный процесс, без
подключения рабочего сервера.

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --example feedback_compose --example profile_validate --example profile_reader
/home/user1/.local/bin/cargo test --manifest-path rechprav/Cargo.toml --locked --offline --test profile_reader
python3 rechprav/scripts/run_reader_experiment.py rechprav/experiments/my-reader-run
```

Runner использует опубликованные JSON snapshots 018 и контрольные predictions 017.
Сохраняет команды, ответы, hashes, проверки совпадения и логи. Он также повторяет
тесты публикации: выделение общего валидатора меняет её кодовую зависимость.

## Три операции загрузки

`Reader::observe(snapshot)` проверяет scope, generation, hashes и связь с
receipt/validation, отмечает последнее наблюдённое desired и активную цель.
Возвращает ticket, если нужен новый engine. Понижение поколений, исчезновение
уже опубликованного active и другие bytes/id при прежней generation запрещены.
Повтор уже загруженного профиля возвращает None и не читает ресурсы заново.

`prepare(ticket)` повторяет полноценный replay 018 и возвращает Candidate.
Из общего валидатора получен **тот же TermEngine**, который был создан на
проверенных байтах Baseline; второго открытия ресурсов между проверкой и install
нет. Старое поле validation в snapshot не заменяет текущую проверку.

`Reader::install(candidate)` сравнивает scope, desired и identity с последней
целью; несоответствие даёт `superseded load`. Установка меняет одну ссылку на
объект, в котором вместе находятся engine/generation/request_id/config_hash.
Ошибка prepare/install оставляет прежний engine. При этом новая наблюдённая цель
сохраняется, поэтому ошибка нового профиля не открывает путь запоздавшей старой
сборке. После исправления ресурсов можно повторить prepare того же снимка.

Ожидающая/неуспешная публикация может дать desired=4, active=3. Reader обслуживает
поколение 3, если оно успешно загружено; status показывает отдельно desired,
target и фактически загруженный active. Молчаливого возврата непроверенного
результата до первой загрузки нет: `acquire` сообщает `no loaded profile`.

## Начатые запросы

`acquire()` возвращает собственный `Rc<LoadedProfile>`. Запрос удерживает его до
завершения. Новый reload не меняет старый handle; ответ содержит generation,
request_id, config_sha256 и обычный Analysis (`corrected/edits/decisions/complete`).
После освобождения последнего handle старый engine уничтожается — это проверено
через Weak. Возврат к прежнему содержимому с новым поколением создаёт новый engine.

Удаление/изменение внешних файлов после загрузки не меняет ответы уже загруженного
engine: словарь, частоты и настройки находятся в памяти. Новый reader при тех же
повреждённых/удалённых ресурсах загрузку отклонит. Изменение файлов само по себе
не является командой переключения; драйвер должен получить новый опубликованный
снимок. Автоматического наблюдения файлов здесь нет.

## JSONL driver

Запуск: `rechprav/target/release/examples/profile_reader technical`. Одна команда
на строку stdin, один JSON-ответ на stdout с немедленным flush:

```json
{"kind":"reload","snapshot":{"scope":"technical","desired_revision":0,"active":null}}
{"kind":"status"}
{"kind":"acquire","id":"request-1"}
{"kind":"analyze","id":"request-1","text":"без снятия сваты"}
{"kind":"release","id":"request-1"}
```

Нулевой snapshot выше демонстрирует отсутствие опубликованного профиля: acquire
вернёт ошибку. Для рабочего прогона подставить полный результат PublicationStore
snapshot; готовая последовательность сохранена в
[input.jsonl](../../rechprav/experiments/019-profile-reader/input.jsonl).

Ответ содержит `result.ok`, `result.output` либо `result.error`, `result.status`
и embedded hashes исходников reader/driver/validator. Ошибка обычной команды
сохраняет процесс и загруженный engine. Ограничения: команда ≤10 MiB, bundle
≤8 MiB, текст ≤64 KiB, не более 16 удерживаемых handles, id 1–64 ASCII символа.
Слишком длинная строка завершает driver, чтобы не продолжать с разорванным JSON.
Сигналы/перезапуск процесса и прикладные таймауты этот driver не обслуживает.

## Граница доказанного

CandidateGenerator пока не объявлен Send/Sync. Поэтому модель использует Rc и
проверяет ownership/чередование операций **на одном потоке**. Отдельные tickets
позволяют смоделировать позднюю установку подготовленного профиля, но CLI reload
синхронен: на время prepare он не обслуживает следующие команды. Это не проверка
параллельного HTTP-сервера или фоновой загрузки.

Для интеграции нужен отдельный опыт с процессами либо осознанная поддержка
Send/Sync: очередь, timeouts, падение worker, новый запрос во время загрузки,
ограничение памяти при одновременном удержании нескольких словарей. Лимит handles
сейчас ограничивает количество ссылок, но не RSS. Алгоритм коррекции, его веса
и ограничения exact-композиции в этом этапе не менялись.

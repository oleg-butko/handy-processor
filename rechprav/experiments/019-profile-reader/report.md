# 019: загрузка профиля и время жизни engine

Дата: 2026-09-22. Ветка `feat/rechprav-profile-reader`.
Протокол `d83defc`, реализация `e3bbaa3`; исходники чистые при запуске.

```bash
python3 rechprav/scripts/run_reader_experiment.py rechprav/experiments/019-profile-reader
```

## Результат

Общий Rust replay validator возвращает TermEngine с уже проверенными ресурсами.
Reader разделяет observe/prepare/install, при установке проверяет ticket по
последней наблюдённой цели. Engine, generation и hash содержатся в одном объекте.
Новый запрос получает ссылку на текущий engine; начатый запрос удерживает старый.

| Проверка | Результат |
|---|---|
| Ошибка первой загрузки | Явный no loaded profile |
| Повтор того же профиля | Тот же Rc, без чтения ресурсов |
| Новое поколение выключения | Новые handles возвращают KEEP |
| Handle, полученный до переключения | Старый ответ и старое поколение |
| Освобождение последнего старого handle | Weak больше не восстанавливает engine |
| Установка старого подготовленного кандидата | superseded load |
| Ошибка нового ресурса | Прежний engine сохранён, старые tickets запрещены |
| Исправление ресурса и повтор того же снимка | Успешная загрузка новой цели |
| Удаление файлов после загрузки | Старый engine работает на удерживаемых данных |
| Новый reader после удаления файлов | Загрузка отклонена |
| Регрессия/изменение identity прежнего поколения/scope/hash/receipt | Отказ |

Семь новых Rust-тестов прошли с первого запуска. Полный прогон: **142 Rust-теста**,
**18 повторных Node-тестов публикации**, clippy all-targets `-D warnings`, fmt,
offline release build reader/validator и py_compile обоих runners. Шесть точек
SIGKILL публикации повторены, поскольку она теперь использует общий модуль replay.

## Сквозной опыт

JSONL driver получил 97 команд, включая 80 анализов прежних 20 synthetic/dev строк.
Использованы настоящие JSON snapshots 018 с поколениями 1, 3 и 5.

| Группа | Handle generation | Сравнение |
|---|---:|---|
| Первые запросы composed | 1 | Все 20 равны composed 017 |
| Старые handles после reload | 1 | Все 20 по-прежнему равны composed 017 |
| Новые handles выключенного профиля | 3 | Все 20 равны global-off 017 |
| Осознанное восстановление профиля | 5 | Все 20 равны composed 017 |

Сравнивались corrected, edits, decisions, complete и comparisons, а не только
видимый текст. Каждый ответ содержит поколение и embedded source hashes reader.
Повтор reload не меняет engine; старый snapshot и повреждённый receipt отклонены,
при этом новое acquire/status сохраняет прежнюю работоспособную generation.
После release обращение к handle явно запрещено.

Runtime и composer binaries не менялись. Validator изменился только выделением
общего replay и возвратом загруженного engine; его metadata теперь содержит hash
`src/profile_validation.rs`. Provenance сохранён в environment.json, inputs и
responses. Исторические snapshots 018 не переписаны: reader проверил их текущим
кодом заново. Старые acceptance fingerprints композитора по-прежнему действуют,
так как его собственные compiled sources остались прежними.

## Что принято, что отложено

Принята модель immutable engine с отдельными handles и проверкой ticket при
install. Повторная загрузка словаря после validation не использовалась: она могла
бы отделить проверенные bytes от фактически используемых. Вместо копирования
больших ресурсов на каждый текст engine удерживает уже загруженные структуры.

Это **однопоточный** опыт Rc/чередования. CandidateGenerator не объявлен Send/Sync;
trait не меняли, чтобы не выдавать непроверенную поддержку потоков за готовую.
JSONL reload синхронен. Не измерялись параллельная загрузка, время ожидания
HTTP-запросов, RSS при нескольких словарях, worker crash и сервисные таймауты.
Перед интеграцией нужен process bridge либо отдельное обоснование Send/Sync.

Нового результата качества ASR нет: composed остаётся 7 TP/1 FP/1 FN, off 0/0/8.
Неизменны ложная коррекция буквального упоминания и пропуск isolated «долгождать».
Следующий алгоритмический опыт 020: manual edit + generated exact, с сохранением
gates/весов/выключателей и без расширения принятого alias. Сервер/UI не подключены.

[Контракт](../../../docs/rechprav/profile-reader.md), [сводка](summary.json),
[построчные сравнения](comparison.json), [команды driver](input.jsonl),
[ответы](output.jsonl), [среда](environment.json).

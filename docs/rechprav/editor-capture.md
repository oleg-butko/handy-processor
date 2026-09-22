# Адаптер записи исправлений: этап 016

`rechprav/editor/capture-store.mjs` — локальный Node/SQLite прототип транспорта
подтверждённых исправлений в Rust [черновики](feedback-drafts.md) и
[реестр принятия](feedback-registry.md). Использует доступный Node 24.19.0 и
node:sqlite без новых зависимостей. Рабочий сервер, UI и их SQLite-база не меняются.

## Почему не использовать автосохранение

В `ui/src/PlainTextEditor.tsx` updateListener передаёт только состояние после
изменения, позиции selection и undo/redo depths. `VoiceEditorPage.tsx` объединяет
отложенные сохранения. Backend `PUT /api/editor/draft` сохраняет snapshot и
увеличивает собственную revision. Отдельная revision активной сессии служит
для защиты актуальности контекста; она не описывает сохранённую редакторскую
транзакцию. Ни один из этих интерфейсов не подтверждает ошибку ASR.

Адаптер требует явных before/after и confirmed_asr. `true` означает, что правку
подтвердили как исправление распознавания, но не включает правило автоматически.
`false` сохраняет обычную/стилистическую правку без обучающего предложения.
Прототип сам не определяет, была ли вставка диктовкой, набором с клавиатуры или
вставкой из буфера. Реальная UI-интеграция должна получать это подтверждение.

## Запрос и смещения

Одна операция относится к ограниченному окну текста с соседями, имеющему
`fragment_id`. Это не весь документ. До 128 Unicode scalar values в before/after/
replacement, не более 4096 UTF-8 bytes. Этот консервативный предел гарантирует
соблюдение ограничения 128 lexical tokens downstream. Автоматическое выделение
окна и сопровождение его позиции при изменениях большого документа не реализованы.

```json
{
  "schema_version": 1,
  "request_id": "correction-1",
  "fragment_id": "note-1-fragment-1",
  "session_id": "window-1",
  "scope": "technical",
  "expected_revision": 0,
  "kind": "replace",
  "before": "😀 сняли свод памяти",
  "after": "😀 сняли слот памяти",
  "confirmed_asr": true,
  "selection": {"start_utf16": 9, "end_utf16": 13, "replacement": "слот"}
}
```

Selection необязателен; без него 014 ищет минимальное изменение. Если передан,
он относится к before **этого окна** в UTF-16, а не к текущему выделению после
правки. Пример преобразуется в byte_start=16, byte_end=24. Emoji занимает две
UTF-16 позиции и четыре UTF-8 bytes. Разрезание surrogate pair, невалидный Unicode,
пустой/неверный диапазон и несоответствие after отвергаются до записи.
CLI также отвергает невалидный UTF-8 входного JSON вместо подстановки U+FFFD.

Все ids — 1..64 ASCII буквы/цифры/`_`/`-`; request_id уникален глобально, включая
разные scopes. Поля и типы проверяются строго, неизвестные поля запрещены.
Обязательное expected_revision — **ревизия адаптера для фрагмента**, не revision
сессии или server autosave. Первая запись создаёт окно при revision=0; следующая
требует точного совпадения before, scope и ожидаемой revision.

## Отмена и повтор

```json
{
  "schema_version": 1,
  "request_id": "undo-1",
  "fragment_id": "note-1-fragment-1",
  "session_id": "window-1",
  "scope": "technical",
  "expected_revision": 1,
  "kind": "undo",
  "before": "😀 сняли слот памяти",
  "after": "😀 сняли свод памяти",
  "target_request_id": "correction-1"
}
```

Undo допускается только для последнего неотменённого replace данного фрагмента
и точной обратной пары before/after. В журнал для Rust попадает retract с
target_event_id=correction-1; состояние окна и revision обновляются вместе.
Повтор undo с тем же id возвращает прежний receipt. Redo — новый replace с новым
request_id и текущей revision. Старое принятие не переносится на новое evidence.

Это ограниченный протокол отмены; произвольная история CodeMirror с объединением
нескольких изменений, перемещением окон и переключением заметок ему пока не
сопоставлена. Нельзя перехватывать Ctrl+Z и без проверки считать его отменой
последнего ASR-исправления. Нужно сохранять связь editor transaction ↔ request ids.

## Хранение и доставка

Схема отдельной БД: requests (неизменяемые payload/event/receipt), fragments
(текст/scope/revision), retractions (связь отмены с исходной записью). `BEGIN
IMMEDIATE` сериализует писателей. Все три части меняются одной транзакцией,
режим SQLite DELETE journal, synchronous=FULL. База имеет собственные application_id
и user_version: сигнатура и эти поля читаются из заголовка обычным read-only
дескриптором ещё до открытия SQLite, затем сверяются повторно через PRAGMA.
Это не даёт запустить SQLite recovery для файла с чужой маркировкой. Враждебная
подмена пути между проверкой и открытием не моделировалась; каталог доверенный.

Одинаковый request_id с тем же нормализованным JSON возвращает сохранённый receipt,
даже если snapshot уже продвинулся дальше. Порядок JSON-ключей не имеет значения.
Другая нагрузка с этим id — ошибка. Ошибочная операция не занимает id. Receipt
содержит request_id, sequence, fragment_id, revision, event_id, replayed.
При неопределённом результате доставки нужно повторить **тот же** payload/id,
а не генерировать новый id и повышать число подтверждений.

Проверены реальными дочерними процессами SIGKILL после вставки записи, перед
commit и после commit до ответа; после открытия остаётся либо вся операция,
либо ничего. Отдельно прервана отмена с записью retraction. SQLite integrity_check
проходит. Два процесса с одной revision и разными правками коммитят одну; два
идентичных запроса дают один commit и один replay. Это проверки process crash,
не моделирование потери питания или повреждения файловой системы.

Лимит — 10000 операций, включая undo. Дальнейшая ротация/компактизация потребует
сохранения checkpoint-префиксов 015 и отдельно не реализована. Другие пределы
014/015 (например, максимум предложений и checkpoint pairs) проверяются там;
успешная запись события не обещает, что любое накопленное множество поместится
в downstream-конфигурацию.

## Команды

```bash
node rechprav/editor/capture-cli.mjs create rechprav/artifacts/my-capture.sqlite3
node rechprav/editor/capture-cli.mjs append rechprav/artifacts/my-capture.sqlite3 request.json
node rechprav/editor/capture-cli.mjs snapshot rechprav/artifacts/my-capture.sqlite3
node rechprav/editor/capture-cli.mjs export rechprav/artifacts/my-capture.sqlite3 rechprav/artifacts/my-capture-export
node --test rechprav/editor/capture-store.test.mjs
python3 rechprav/scripts/run_capture_experiment.py rechprav/experiments/my-capture-run
```

Create требует нового файла; append принимает один JSON, максимум 64 KiB.
Export требует нового каталога и читает согласованный SQLite snapshot. Содержит
events.jsonl для Rust, исходные requests.jsonl, snapshot.json с receipts/frames,
capture-manifest.json с hashes. Файлы не дописываются; повторный export создаёт
новый снимок. Поэтому оборванный JSONL не нужно чинить в каноническом журнале:
источник истины — SQLite. Сбой во время экспорта может оставить неполный каталог;
нужно экспортировать заново в новый. Атомарная публикация профиля не реализована.

Тексты остаются локально. Эксперимент использует только synthetic fixtures,
а его SQLite-файлы хранятся в ignored `rechprav/artifacts/`. Частные события
следует хранить в private-каталогах, не в отслеживаемых экспериментах.

## Следующие подзадачи

1. Проверить композицию manual terms и принятого generated профиля: приоритеты,
   конфликтующие sets, выключение и удаление правила, provenance, атомарная
   замена проверенного снимка. Реестр 015 сам не копирует reference manual terms.
2. Подготовить UI-протокол подтверждения исправления: before/start selection,
   окно с контекстом, связь с документом/undo group, сохраняемый outbox с request ids.
   Подключить только после проверки сценариев повторной отправки/перезагрузки.
3. Проверить фактический маршрут review→accept→rebuild→live reload и отмену
   загруженного правила. Нынешние offline snapshots ничего в сервер не устанавливают.

[Протокол](016-editor-capture-plan.md),
[эксперимент](../../rechprav/experiments/016-editor-capture-v2/report.md).

# Политика генерации отдельных терминов

Этап 020 разрешает смешанную композицию: глобальный `policy.mode="edit"` для
ручных записей и `generation="exact"` у принятых исправлений. И ручные, и
выученные кандидаты ранжируются в одном TermEngine с прежними весами/gates.

У `sets.terms` добавлено необязательное поле `generation`:

| Значение | Поиск кандидатов |
|---|---|
| Поле отсутствует или `inherit` | Прежний глобальный exact/edit/weighted режим |
| `exact` | Только нормализованное совпадение одного из перечисленных aliases |

Это ограничение поиска конкретного правила, не разрешение менять known слова.
Exact проверяется **до** edit/weighted веток, поэтому включение weighted distance
не расширяет такой alias. Границы слов сохраняются: `пэ эн зэ` не равно `пээнзэ`
или `пэ энзэ`. Регистр, ё/е согласно strict_yo и горизонтальные пробелы нормализуются
так же, как в прежнем global exact. Пунктуация, newline, protected spans и общий
budget не обходятся. Для exact-кандидата distance=0, weighted_cost отсутствует.

Политика наследуется всеми формами термина. Отдельного generation у формы нет.
Существующий pronunciation generator, если указан в ручной записи, всё ещё
создаёт свой явный каталог aliases; exact ограничивает дальнейший поиск по нему.
Выученное правило не получает pronunciation/forms владельца, как и прежде.

Композитор выставляет `generation="exact"` **каждому** добавляемому правилу, даже
при global exact. Сохраняются enabled, boost владельца/набора, известные слова,
max_known_distance, пересечение контекстов, exclusions и source penalties.
Ручные definitions и их generation остаются прежними. Global edit теперь
поддержан; свободные context cues по-прежнему дают `unsupported_context_cues`.

## Повторная проверка и воспроизведение

```bash
/home/user1/.local/bin/cargo build --manifest-path rechprav/Cargo.toml --release --locked --offline --bin rechprav --example feedback_compose --example profile_validate --example profile_reader
python3 rechprav/scripts/run_mixed_experiment.py rechprav/experiments/my-mixed-run
node --test rechprav/publication/store.test.mjs
```

Runner сравнивает замороженные профили, старые конфиги, публикацию и reader.
Для поля previous_binary_sha256 нужен сохранённый binary 013–019 по адресу
`rechprav/artifacts/020-mixed-generation-baseline/rechprav`; он сохранён до правки.
Ресурсы и fixtures предыдущих опытов тоже нужны локально. Ничего не скачивается.

Изменились compiled source fingerprints. Старые решения review становятся
`stale_environment`; старый экспорт публикации отклоняется по compiled sources.
Нужно получить новые tokens и заново принять тестовые proposals. Миграции или
автоматического переноса пользовательского согласия нет. Publication-тесты
строят свои временные synthetic profiles с новым review, не переписывают 017.

Для точного воспроизведения исторических runners 017–019 нужен соответствующий
commit: они проверяли прежние режимы/экспорты и намеренно не обходят новый guard.
В текущем коде сквозной путь проверяет runner 020. Старые TOML без generation
совместимы для анализа текста; несовместимость review/export — отдельный guard.

## Цена точности

Exact не обобщает даже правдоподобное новое произношение/слитное написание.
Непринятые слитные варианты ПНЗ в корпусе остаются положительными gold и учитываются
как FN. Для расширения покрытия нужны дополнительные явно подтверждённые aliases
или отдельное исследование обучения обобщений. Усиление веса не создаёт отсутствующий
кандидат и не обходит обязательный контекст.

Diagnostic `unpinned` в опыте удаляет generation у выученных правил. Это отдельный
TOML для сравнения, он не соответствует принятому composition manifest и не
предназначен для публикации. Его дополнительные TP и FP надо рассматривать вместе.
Результаты synthetic/dev не определяют лучшую политику для реальной истории Handy.

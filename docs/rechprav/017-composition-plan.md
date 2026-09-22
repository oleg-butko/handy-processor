# 017: композиция ручных и принятых правил

2026-09-22, протокол до реализации. Offline-композиция, runtime не меняется.
Атомарную публикацию и UI выполнить следующим отдельным этапом после проверки
композиции. Корпус/сценарии закрепляются в scripts/composition_profiles.py.

Проблема: простой append может вернуть отключённый термин, потерять его вес и
ограничения, распространить global edit mode на exact feedback aliases.
Последовательный запуск двух корректоров также скрывает конкурентов/перекрытия.
Выбран один итоговый Config с единым ранжированием существующего TermEngine.

## Контракт

- Manual config копируется без изменения существующих sets/terms/policy/penalties.
- Явные bindings: proposal_id, enabled, owner={set_id,term_id} либо owner=null для
  нового термина. Без binding предложение не добавляется. Для owner=null boost
  задаётся отдельно; для owner boost binding должен быть 0, используется вес
  ручного термина и его набора. Повторы evidence не меняют вес.
- Existing canonical/form, включая выключенные, запрещает owner=null. Owner
  проверяется и после удаления: missing не превращается автоматически в new term.
  Canonical должен совпасть побайтно, нормализованное совпадение и другой регистр
  требуют review. Это не неявное переименование цели подтверждённой правки.
- Выключенные manual config/set/term или feedback settings/binding блокируют
  добавление правила. Отмена evidence удаляет только generated запись, ручная остаётся.
- Generated alias остаётся отдельной term-записью в наборе владельца, с исходным
  контекстом и boost владельца. Forms/pronunciation не переносятся в generated rule.
- Known-word gates пересекаются (Never строже Preserve, затем Context, Always),
  max_known_distance=0. Exclusions наследуются. Для forms берутся form overrides.
- Anchored patterns пересекаются: совместимые left suffix/right prefix дают
  более длинный контекст; несовместимые дают пустое пересечение. Объединение
  через OR запрещено. Manual свободные context cues пока unsupported, правило
  пропускается с причиной. Это намеренно ограниченный композитор.
- Для добавления feedback нужен global mode=exact. Edit mode сохраняется для
  manual rules, feedback получает unsupported_mode. Не расширять exact alias
  через OOV edit distance. Source penalties и thresholds сохраняются как явные
  настройки пользователя, их изменение меняет fingerprint review.
- Решения заново проверяются через 015, не импортируются из произвольного active.toml.
  Environment включает manual config, bindings, ресурсы и исходники инструмента.
  После изменения этих файлов прежние решения stale; эксперименты переключателей
  отдельно делают новый accept, чтобы проверить composition guard независимо от stale.
- Выход: registry, composition audit/provenance, composed.toml и manifest в новом
  каталоге. Namespace коллизия/невалидный итоговый Config — ошибка до записи.

Проверки: unchanged manual prefix; отказ resurrect disabled/deleted owner;
веса и конфликт; family context; новый термин; known gate; exclusions;
source penalty; unsupported edit/cues; отзыв evidence; stale settings; повтор;
независимый KEEP и minimal UTF-8 spans. Метрики synthetic/dev, не реальный holdout.

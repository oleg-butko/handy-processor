import { Badge, Group, Stack, Table, Text } from "@mantine/core";
import type { AppliedRuleRecord } from "./types.js";

const reasons: Record<string, string> = {
  term_selected: "Кандидат прошёл ограничения и пороги выбора",
  keep_score_margin: "Недостаточно баллов для замены исходника",
  term_conflict: "Конкурирующие варианты слишком близки по оценке",
  overlap_conflict: "Конфликт замен пересекающихся фрагментов",
  overlap_lower_priority: "Другой пересекающийся фрагмент имеет приоритет",
  context_required: "Нет необходимого контекста",
  context_excluded: "Сработало контекстное исключение",
  known_word_blocked: "Замена существующего слова запрещена",
  known_changed_word_blocked: "Правка затронула бы защищённое существующее слово",
  known_distance_limit: "Превышено расстояние для существующего слова",
  comparison_limit: "Исчерпан лимит поиска; все исправления отменены",
  eligible: "Допущен к сравнению",
};
const components: Record<string, string> = {
  base: "Базовая оценка", global: "Общий вес", set: "Вес набора", term: "Вес термина",
  distance: "Штраф расстояния", context: "Бонус контекста", known: "Защита известного слова", source: "Поправка исходника",
};
const number = (value: number) => new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 3 }).format(value);

export function CorrectionDetails({ rule }: { readonly rule: AppliedRuleRecord }) {
  const trace = rule.correction;
  if (rule.outcome !== "changed" || !rule.changed || !trace?.complete ||
      !trace.decisions.some(decision => decision.selected !== null)) return null;
  return (
    <details className="correction-details">
      <summary>Почему выбраны эти исправления</summary>
      <Stack gap="sm" mt="sm">
        {rule.error ? <Text c="orange" size="sm">Коррекция пропущена: {rule.error}</Text> : null}
        {trace ? <>
          <Text size="sm" c="dimmed">Баллы — предпочтения, не вероятности. Минимум для замены: {number(trace.minScore)};
            необходимый отрыв от конкурента: {number(trace.minMargin)}. Анализ выполнен до изменения регистра и чисел.</Text>
          {!trace.complete ? <Text c="orange">Поиск не завершён. Все предложенные исправления отменены.</Text> : null}
          {trace.decisions.length === 0 ? <Text size="sm">Подходящих кандидатов нет: варианты не найдены или фрагменты защищены.</Text> : null}
          {trace.decisions.map((decision, index) => (
            <details className="correction-decision" key={`${decision.byte_start}:${decision.byte_end}:${index}`}>
              <summary>«{decision.original}» → {decision.selected === null || !trace.complete ? "оставлено без изменения" : `«${decision.selected}»`}</summary>
              <Stack gap="xs" mt="xs">
                <Text size="sm">{reasons[decision.reason] ?? decision.reason}
                  {decision.runner_up_margin !== null ? `. Отрыв: ${number(decision.runner_up_margin)}` : ". Допущенного конкурента нет"}.</Text>
                <div className="correction-candidates-scroll">
                  <Table striped withTableBorder style={{ minWidth: 480 }}>
                    <Table.Thead><Table.Tr><Table.Th>Кандидат</Table.Th><Table.Th>Баллы</Table.Th><Table.Th>Почему</Table.Th></Table.Tr></Table.Thead>
                    <Table.Tbody>{decision.candidates.map((candidate, i) => (
                      <Table.Tr key={`${candidate.set}:${candidate.term}:${i}`}>
                        <Table.Td><Text size="sm" fw={600}>{candidate.canonical}</Text>
                          {trace.complete && candidate.canonical === decision.selected ? <Badge size="xs" color="teal">Выбран</Badge> : null}
                          <Text size="xs">Вариант: {candidate.alias}</Text>
                          <Text size="xs" c="dimmed">Набор: {candidate.set}; термин: {candidate.term}</Text>
                          {candidate.alias_origin?.includes("generated") ? <Badge size="xs">Произношение букв</Badge> : null}
                        </Table.Td>
                        <Table.Td>{number(candidate.score)}</Table.Td>
                        <Table.Td><Text size="sm">{reasons[candidate.gate] ?? candidate.gate}</Text>
                          <Text size="xs">Расстояние: {candidate.distance}</Text>
                          <details><summary>Из чего сложилась оценка</summary>
                            {Object.entries(candidate.parts).map(([key, value]) => (
                              <Group key={key} justify="space-between" gap="xs"><Text size="xs">{components[key] ?? key}</Text><Text size="xs">{number(value)}</Text></Group>
                            ))}
                          </details>
                          {[...candidate.context_hits, ...(candidate.context_pattern_hits ?? [])].length > 0 ?
                            <Text size="xs">Контекст: {[...candidate.context_hits, ...(candidate.context_pattern_hits ?? [])].join(", ")}</Text> : null}
                          {candidate.context_exclusion_hits?.length ? <Text size="xs">Исключения: {candidate.context_exclusion_hits.join(", ")}</Text> : null}
                          {candidate.source_penalty_hits?.length ? <Text size="xs">Поправки исходника: {candidate.source_penalty_hits.join(", ")}</Text> : null}
                        </Table.Td>
                      </Table.Tr>
                    ))}</Table.Tbody>
                  </Table>
                </div>
              </Stack>
            </details>
          ))}
          {trace.omittedDecisions > 0 || trace.omittedCandidates > 0 ? <Text size="sm" c="dimmed">
            Показана часть диагностики. Ещё фрагментов: {trace.omittedDecisions}; кандидатов в показанных фрагментах: {trace.omittedCandidates}.
          </Text> : null}
          <Text size="xs" c="dimmed">Снимок профиля: {trace.profile.slice(0, 16)}. Объяснение сохранено при обработке и не пересчитывается после изменения настроек.</Text>
        </> : null}
      </Stack>
    </details>
  );
}

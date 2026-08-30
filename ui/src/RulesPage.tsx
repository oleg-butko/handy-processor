import {
  Alert,
  Badge,
  Button,
  Card,
  Checkbox,
  Center,
  Group,
  Loader,
  Paper,
  SimpleGrid,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import type { TransformationRuleState } from "./types.js";

interface RulesPageProps {
  readonly rules: readonly TransformationRuleState[];
  readonly loading: boolean;
  readonly error: string | undefined;
  readonly pendingRuleIds: ReadonlySet<string>;
  readonly onReload: () => void;
  readonly onToggle: (ruleId: string, enabled: boolean) => void;
}

function RuleCard({
  rule,
  pending,
  onToggle,
}: {
  readonly rule: TransformationRuleState;
  readonly pending: boolean;
  readonly onToggle: (ruleId: string, enabled: boolean) => void;
}) {
  return (
    <Card className="hp-surface" withBorder radius="md" padding="md">
      <Stack gap="sm">
        <Group justify="space-between" align="flex-start" wrap="nowrap">
          <Checkbox
            aria-label={`${rule.enabled ? "Выключить" : "Включить"} правило «${rule.title}»`}
            checked={rule.enabled}
            disabled={pending}
            mt={2}
            size="sm"
            onChange={(event) =>
              onToggle(rule.id, event.currentTarget.checked)
            }
          />
          <Stack gap={4} className="rule-summary">
            <Group gap={6}>
              <Text fw={700}>{rule.title}</Text>
              <Badge color="blue" size="sm" variant="outline">
                {rule.sources.length === 2
                  ? "Везде"
                  : rule.sources[0] === "voice-editor"
                    ? "Редактор"
                    : "Внешний ввод"}
              </Badge>
              <Badge
                color={rule.enabled ? "teal" : "gray"}
                size="sm"
                variant={rule.enabled ? "light" : "outline"}
              >
                {pending
                  ? "Сохранение…"
                  : rule.enabled
                    ? "Включено"
                    : "Выключено"}
              </Badge>
            </Group>
            <Text c="dimmed" size="sm">
              {rule.description}
            </Text>
          </Stack>
        </Group>

        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm">
          <Stack gap={4}>
            <Text c="dimmed" fw={700} size="xs" tt="uppercase">
              До
            </Text>
            <Paper className="rule-example" p="xs" radius="sm">
              <Text size="sm">{rule.example.input}</Text>
            </Paper>
          </Stack>
          <Stack gap={4}>
            <Text c="cyan.8" fw={700} size="xs" tt="uppercase">
              После
            </Text>
            <Paper
              className="rule-example rule-example-result"
              p="xs"
              radius="sm"
            >
              <Text size="sm">{rule.example.output}</Text>
            </Paper>
          </Stack>
        </SimpleGrid>
      </Stack>
    </Card>
  );
}

export function RulesPage({
  rules,
  loading,
  error,
  pendingRuleIds,
  onReload,
  onToggle,
}: RulesPageProps) {
  const groups = [
    {
      category: "common" as const,
      title: "Общие правила",
      description: "Обычные преобразования текста и внешней вставки.",
    },
    {
      category: "voice-editor" as const,
      title: "Правила голосового редактора",
      description: "Учитывают позицию курсора и контекст документа.",
    },
  ];

  return (
    <Stack gap="md" pt="md">
      <Stack gap={4}>
        <Title order={2}>Список правил</Title>
        <Text c="dimmed" size="sm">
          Изменения применяются только к новым трансформациям. Существующая
          история не пересчитывается.
        </Text>
      </Stack>

      {error ? (
        <Alert
          color="red"
          title="Не удалось сохранить или загрузить правила"
        >
          <Group justify="space-between">
            <Text size="sm">{error}</Text>
            <Button color="red" size="xs" variant="light" onClick={onReload}>
              Повторить
            </Button>
          </Group>
        </Alert>
      ) : null}

      {loading && rules.length === 0 ? (
        <Center py={80}>
          <Loader />
        </Center>
      ) : null}

      <Stack gap="lg" opacity={loading ? 0.65 : 1}>
        {groups.map((group) => {
          const groupedRules = rules.filter(
            (rule) => rule.category === group.category,
          );
          if (groupedRules.length === 0) {
            return null;
          }
          return (
            <Stack gap="sm" key={group.category}>
              <Stack gap={2}>
                <Title order={3}>{group.title}</Title>
                <Text c="dimmed" size="sm">
                  {group.description}
                </Text>
              </Stack>
              {groupedRules.map((rule) => {
                const pending = pendingRuleIds.has(rule.id);
                return (
                  <RuleCard
                    key={rule.id}
                    rule={rule}
                    pending={pending}
                    onToggle={onToggle}
                  />
                );
              })}
            </Stack>
          );
        })}
      </Stack>
    </Stack>
  );
}

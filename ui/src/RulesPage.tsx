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

export function RulesPage({
  rules,
  loading,
  error,
  pendingRuleIds,
  onReload,
  onToggle,
}: RulesPageProps) {
  return (
    <Stack gap="xl" pt="xl">
      <Stack gap={4}>
        <Title order={2}>Список правил</Title>
        <Text c="dimmed">
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

      <Stack gap="md" opacity={loading ? 0.65 : 1}>
        {rules.map((rule) => {
          const pending = pendingRuleIds.has(rule.id);
          return (
            <Card key={rule.id} withBorder radius="lg" padding="lg" shadow="xs">
              <Stack gap="md">
                <Group justify="space-between" align="flex-start" wrap="nowrap">
                  <Checkbox
                    aria-label={`${rule.enabled ? "Выключить" : "Включить"} правило «${rule.title}»`}
                    checked={rule.enabled}
                    disabled={pending}
                    mt={4}
                    size="md"
                    onChange={(event) =>
                      onToggle(rule.id, event.currentTarget.checked)
                    }
                  />
                  <Stack gap={4} className="rule-summary">
                    <Group gap="sm">
                      <Text fw={700}>{rule.title}</Text>
                      <Badge
                        color={rule.enabled ? "teal" : "gray"}
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

                <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
                  <Stack gap={5}>
                    <Text c="dimmed" fw={700} size="xs" tt="uppercase">
                      До
                    </Text>
                    <Paper className="rule-example" p="sm" radius="md">
                      <Text size="sm">{rule.example.input}</Text>
                    </Paper>
                  </Stack>
                  <Stack gap={5}>
                    <Text c="cyan.8" fw={700} size="xs" tt="uppercase">
                      После
                    </Text>
                    <Paper
                      className="rule-example rule-example-result"
                      p="sm"
                      radius="md"
                    >
                      <Text size="sm">{rule.example.output}</Text>
                    </Paper>
                  </Stack>
                </SimpleGrid>
              </Stack>
            </Card>
          );
        })}
      </Stack>
    </Stack>
  );
}

import {
  Alert,
  Badge,
  Button,
  Card,
  Center,
  Group,
  Loader,
  Pagination,
  Paper,
  SimpleGrid,
  Stack,
  Text,
  Title,
  Tooltip,
} from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import type {
  HistoryResponse,
  TransformationRecord,
  TransformationRuleState,
} from "./types.js";

const PAGE_SIZE = 10;

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat("ru-RU", {
        dateStyle: "medium",
        timeStyle: "medium",
      }).format(date);
}

function AppliedRules({
  item,
  ruleTitles,
}: {
  readonly item: TransformationRecord;
  readonly ruleTitles: ReadonlyMap<string, string>;
}) {
  if (item.appliedRules === null) {
    return (
      <Badge color="gray" variant="outline">
        Правила не записывались
      </Badge>
    );
  }

  if (item.appliedRules.length === 0) {
    return (
      <Badge color="gray" variant="outline">
        Все правила были выключены
      </Badge>
    );
  }

  return (
    <Group gap="xs">
      {item.appliedRules.map((rule) => (
        <Tooltip
          key={`${rule.id}:${rule.version}`}
          label={
            rule.changed
              ? "Правило изменило текст"
              : "Правило было включено, но изменений не потребовалось"
          }
        >
          <Badge
            color={rule.changed ? "cyan" : "gray"}
            variant={rule.changed ? "light" : "outline"}
          >
            {ruleTitles.get(rule.id) ?? rule.id}
          </Badge>
        </Tooltip>
      ))}
    </Group>
  );
}

function TransformationCard({
  item,
  ruleTitles,
}: {
  readonly item: TransformationRecord;
  readonly ruleTitles: ReadonlyMap<string, string>;
}) {
  return (
    <Card withBorder radius="lg" padding="lg" shadow="xs">
      <Stack gap="md">
        <Group justify="space-between" align="center">
          <Badge variant="light" color="gray">
            #{item.id}
          </Badge>
          <Text c="dimmed" size="xs">
            {formatDate(item.createdAt)}
          </Text>
        </Group>

        <SimpleGrid cols={{ base: 1, md: 2 }} spacing="lg">
          <Stack gap={6}>
            <Text c="dimmed" fw={700} size="xs" tt="uppercase">
              Исходный текст
            </Text>
            <Paper className="text-panel" p="md" radius="md">
              <Text component="div" className="log-text">
                {item.originalText}
              </Text>
            </Paper>
          </Stack>

          <Stack gap={6}>
            <Text c="cyan.8" fw={700} size="xs" tt="uppercase">
              Результат
            </Text>
            <Paper className="text-panel text-panel-result" p="md" radius="md">
              <Text component="div" className="log-text">
                {item.transformedText}
              </Text>
            </Paper>
          </Stack>
        </SimpleGrid>

        <Stack gap={6}>
          <Text c="dimmed" fw={700} size="xs" tt="uppercase">
            Применённые правила
          </Text>
          <AppliedRules item={item} ruleTitles={ruleTitles} />
        </Stack>
      </Stack>
    </Card>
  );
}

export function HistoryPage({
  rules,
}: {
  readonly rules: readonly TransformationRuleState[];
}) {
  const [page, setPage] = useState(1);
  const [history, setHistory] = useState<HistoryResponse>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [refreshToken, setRefreshToken] = useState(0);
  const ruleTitles = new Map(rules.map((rule) => [rule.id, rule.title]));

  const loadHistory = useCallback(async (signal: AbortSignal) => {
    setLoading(true);
    setError(undefined);

    try {
      const response = await fetch(
        `/api/transformations?page=${page}&pageSize=${PAGE_SIZE}`,
        { signal },
      );
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }

      setHistory((await response.json()) as HistoryResponse);
    } catch (requestError) {
      if (
        requestError instanceof DOMException &&
        requestError.name === "AbortError"
      ) {
        return;
      }
      setError(
        requestError instanceof Error
          ? requestError.message
          : "Не удалось загрузить историю",
      );
    } finally {
      if (!signal.aborted) {
        setLoading(false);
      }
    }
  }, [page]);

  useEffect(() => {
    const controller = new AbortController();
    void loadHistory(controller.signal);
    return () => controller.abort();
  }, [loadHistory, refreshToken]);

  return (
    <Stack gap="xl" pt="xl">
      <Group justify="space-between" align="flex-end">
        <Stack gap={4}>
          <Title order={2}>История преобразований</Title>
          <Text c="dimmed">Сначала показаны самые свежие записи</Text>
        </Stack>
        <Button
          variant="light"
          loading={loading}
          onClick={() => setRefreshToken((value) => value + 1)}
        >
          Обновить
        </Button>
      </Group>

      {error ? (
        <Alert color="red" title="Ошибка загрузки">
          {error}
        </Alert>
      ) : null}

      {loading && history === undefined ? (
        <Center py={80}>
          <Loader />
        </Center>
      ) : null}

      {!loading && history?.items.length === 0 ? (
        <Paper withBorder radius="lg" p={48} ta="center">
          <Title order={3}>История пока пуста</Title>
          <Text c="dimmed" mt="xs">
            Записи появятся после первой post-processing трансформации Handy.
          </Text>
        </Paper>
      ) : null}

      {history?.items.length ? (
        <Stack gap="md" opacity={loading ? 0.55 : 1}>
          {history.items.map((item) => (
            <TransformationCard
              item={item}
              key={item.id}
              ruleTitles={ruleTitles}
            />
          ))}
        </Stack>
      ) : null}

      {history && history.pagination.totalPages > 1 ? (
        <Group justify="space-between" align="center">
          <Text c="dimmed" size="sm">
            Всего записей: {history.pagination.total}
          </Text>
          <Pagination
            value={page}
            total={history.pagination.totalPages}
            onChange={setPage}
            withEdges
          />
        </Group>
      ) : null}
    </Stack>
  );
}

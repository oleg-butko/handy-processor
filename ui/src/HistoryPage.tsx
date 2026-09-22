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
import { CorrectionDetails } from "./CorrectionDetails.js";
import type {
  HistoryResponse,
  TransformationRecord,
  TransformationRuleState,
} from "./types.js";

const PAGE_SIZE = 10;

const RULE_REASON_LABELS: Readonly<Record<string, string>> = {
  "correction-unavailable": "Коррекция недоступна; исходник сохранён для последующих правил",
  "correction-incomplete": "Поиск не завершён; предложенные исправления отменены",
  "sentence-start": "Пропущено: курсор находился в начале предложения",
  "context-unknown": "Пропущено: контекст редактора неизвестен",
  "source-not-supported": "Пропущено: правило не применяется к этому источнику",
};

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
      <Badge color="gray" size="sm" variant="outline">
        Правила не записывались
      </Badge>
    );
  }

  if (item.appliedRules.length === 0) {
    return (
      <Badge color="gray" size="sm" variant="outline">
        Все правила были выключены
      </Badge>
    );
  }

  return (
    <Stack gap="xs" w="100%" miw={0}>
    <Group gap={6}>
      {item.appliedRules.map((rule) => {
        const skipped = rule.outcome === "skipped";
        const label = skipped
          ? (RULE_REASON_LABELS[rule.reason ?? ""] ?? "Пропущено по условию")
          : rule.outcome === "changed"
            ? "Правило изменило текст"
            : "Правило было выполнено, но изменений не потребовалось";
        return (
          <Tooltip key={`${rule.id}:${rule.version}`} label={label}>
            <Badge
              size="sm"
              color={
                rule.outcome === "changed"
                  ? "cyan"
                  : skipped
                    ? "orange"
                    : "gray"
              }
              variant={rule.outcome === "changed" ? "light" : "outline"}
            >
              {ruleTitles.get(rule.id) ?? rule.id}
              {skipped ? " · пропущено" : ""}
            </Badge>
          </Tooltip>
        );
      })}
    </Group>
    {item.appliedRules.map((rule) => <CorrectionDetails key={`${rule.id}:${rule.version}`} rule={rule} />)}
    </Stack>
  );
}

function ContextBadges({ item }: { readonly item: TransformationRecord }) {
  if (item.source === "external") {
    return (
      <Badge color="gray" size="sm" variant="outline">
        Внешний ввод
      </Badge>
    );
  }

  const position = item.context?.editor.sentencePosition;
  const positionLabel =
    position === "start"
      ? "Начало предложения"
      : position === "middle"
        ? "Середина предложения"
        : "Контекст неизвестен";

  return (
    <Group gap={6}>
      <Badge color="violet" size="sm" variant="light">
        Голосовой редактор
      </Badge>
      <Badge
        color={position === "start" || position === "middle" ? "blue" : "orange"}
        size="sm"
        variant="outline"
      >
        {positionLabel}
      </Badge>
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
    <Card className="hp-surface" withBorder radius="md" padding="sm">
      <Stack gap="sm">
        <Group justify="space-between" align="center" wrap="wrap">
          <Group gap="xs">
            <Badge size="sm" variant="light" color="gray">
              #{item.id}
            </Badge>
            <ContextBadges item={item} />
          </Group>
          <Text c="dimmed" size="xs">
            {formatDate(item.createdAt)}
          </Text>
        </Group>

        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm">
          <Stack gap={4}>
            <Text c="dimmed" fw={700} size="xs" tt="uppercase">
              Исходный текст
            </Text>
            <Paper className="text-panel" p="xs" radius="sm">
              <Text component="div" className="log-text" size="sm">
                {item.originalText}
              </Text>
            </Paper>
          </Stack>

          <Stack gap={4}>
            <Text c="cyan.8" fw={700} size="xs" tt="uppercase">
              Результат
            </Text>
            <Paper className="text-panel text-panel-result" p="xs" radius="sm">
              <Text component="div" className="log-text" size="sm">
                {item.transformedText}
              </Text>
            </Paper>
          </Stack>
        </SimpleGrid>

        <Group gap={6} align="center">
          <Text c="dimmed" fw={700} size="xs" tt="uppercase">
            Применённые правила
          </Text>
          <AppliedRules item={item} ruleTitles={ruleTitles} />
        </Group>
      </Stack>
    </Card>
  );
}

export function HistoryPage({
  rules,
  active = true,
}: {
  readonly rules: readonly TransformationRuleState[];
  readonly active?: boolean;
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
    if (!active) return;
    const controller = new AbortController();
    void loadHistory(controller.signal);
    return () => controller.abort();
  }, [active, loadHistory, refreshToken]);

  return (
    <Stack gap="md" pt="md">
      <Group justify="space-between" align="flex-end">
        <Stack gap={4}>
          <Title order={2}>История преобразований</Title>
          <Text c="dimmed" size="sm">
            Сначала показаны самые свежие записи
          </Text>
        </Stack>
        <Button
          size="xs"
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
        <Paper className="hp-surface" withBorder radius="md" p={32} ta="center">
          <Title order={3}>История пока пуста</Title>
          <Text c="dimmed" mt="xs">
            Записи появятся после первой post-processing трансформации Handy.
          </Text>
        </Paper>
      ) : null}

      {history?.items.length ? (
        <Stack gap="sm" opacity={loading ? 0.55 : 1}>
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

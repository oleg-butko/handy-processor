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
  SegmentedControl,
  SimpleGrid,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
  Tooltip,
} from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import { TRANSFORMATION_RULE_IDS } from "../../src/transformation-rules.js";
import { CorrectionDetails } from "./CorrectionDetails.js";
import type {
  HistoryResponse,
  TransformationRecord,
  TransformationRuleState,
} from "./types.js";

const PAGE_SIZE = 10;
type SearchMode = "both" | "original" | "result";

function hasCorrectionReplacement(item: TransformationRecord): boolean {
  return item.appliedRules?.some(rule =>
    rule.id === TRANSFORMATION_RULE_IDS.correction && rule.outcome === "changed" && rule.changed) ?? false;
}

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
    {item.appliedRules.filter(rule => rule.id === TRANSFORMATION_RULE_IDS.correction &&
      rule.outcome === "changed" && rule.changed).map(rule =>
      <CorrectionDetails key={`${rule.id}:${rule.version}`} rule={rule} />)}
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
    <Card className="hp-surface history-card" withBorder radius="md" padding="sm">
      <Stack gap="xs">
        <Group justify="space-between" align="center" wrap="wrap">
          <Group gap="xs">
            <Badge size="sm" variant="light" color="gray">
              #{item.id}
            </Badge>
            <ContextBadges item={item} />
            {hasCorrectionReplacement(item) ? <Badge size="sm" color="red" variant="filled">Замена</Badge> : null}
          </Group>
          <Text c="dimmed" size="xs">
            {formatDate(item.createdAt)}
          </Text>
        </Group>

        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="xs">
            <Paper className="text-panel" p={6} radius="sm" role="group" aria-label="Исходный текст">
              <Text component="div" className="log-text" size="sm">
                {item.originalText}
              </Text>
            </Paper>

            <Paper className="text-panel text-panel-result" p={6} radius="sm" role="group" aria-label="Результат">
              <Text component="div" className="log-text" size="sm">
                {item.transformedText}
              </Text>
            </Paper>
        </SimpleGrid>

        <details className="history-rules-details">
          <summary>Применённые правила</summary>
          <AppliedRules item={item} ruleTitles={ruleTitles} />
        </details>
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
  const [query, setQuery] = useState("");
  const [searchTerm, setSearchTerm] = useState("");
  const [searchIn, setSearchIn] = useState<SearchMode>("both");
  const [correctionOnly, setCorrectionOnly] = useState(false);
  const [history, setHistory] = useState<HistoryResponse>();
  const [loadedKey, setLoadedKey] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [refreshToken, setRefreshToken] = useState(0);
  const ruleTitles = new Map(rules.map((rule) => [rule.id, rule.title]));
  const requestKey = `${page}:${searchIn}:${correctionOnly}:${searchTerm}`;
  const queryPending = query.trim() !== searchTerm;

  useEffect(() => {
    const timer = setTimeout(() => setSearchTerm(query.trim()), 300);
    return () => clearTimeout(timer);
  }, [query]);

  const loadHistory = useCallback(async (signal: AbortSignal) => {
    setLoading(true);
    setError(undefined);

    try {
      const response = await fetch(
        `/api/transformations?${new URLSearchParams({ page: String(page), pageSize: String(PAGE_SIZE),
          query: searchTerm, searchIn, correctionOnly: String(correctionOnly) })}`,
        { signal },
      );
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }

      const result = (await response.json()) as HistoryResponse;
      if (!signal.aborted) {
        setHistory(result);
        setLoadedKey(requestKey);
      }
    } catch (requestError) {
      if (signal.aborted) return;
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
  }, [page, searchTerm, searchIn, correctionOnly, requestKey]);

  useEffect(() => {
    if (!active || queryPending) return;
    const controller = new AbortController();
    void loadHistory(controller.signal);
    return () => controller.abort();
  }, [active, loadHistory, refreshToken, queryPending]);

  const visibleHistory = !loading && !queryPending && loadedKey === requestKey ? history : undefined;
  const filtered = searchTerm.length > 0 || correctionOnly;

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

      <Stack gap="xs" className="history-filters">
        <TextInput label="Поиск по истории" placeholder="Слово или фраза" value={query} maxLength={256}
          onChange={event => { setQuery(event.currentTarget.value); setPage(1); }} />
        <Group gap="xs" align="center" wrap="wrap">
          <Text size="sm">Искать в:</Text>
          <SegmentedControl size="xs" aria-label="Область поиска" value={searchIn} data={[
            { label: "Оба текста", value: "both" },
            { label: "Исходный", value: "original" },
            { label: "Результат", value: "result" },
          ]} onChange={value => { setSearchIn(value as SearchMode); setPage(1); }} />
        </Group>
        <Switch label="Только с заменой терминов и фраз" checked={correctionOnly}
          onChange={event => { setCorrectionOnly(event.currentTarget.checked); setPage(1); }} />
      </Stack>

      {error ? (
        <Alert color="red" title="Ошибка загрузки">
          {error}
        </Alert>
      ) : null}

      {!visibleHistory && !error ? (
        <Center py={80}>
          <Loader />
        </Center>
      ) : null}

      {visibleHistory?.items.length === 0 ? (
        <Paper className="hp-surface" withBorder radius="md" p={32} ta="center">
          <Title order={3}>{filtered ? "Ничего не найдено" : "История пока пуста"}</Title>
          <Text c="dimmed" mt="xs">
            {filtered ? "Попробуйте другой запрос или отключите фильтр." : "Записи появятся после первой обработки текста Handy."}
          </Text>
        </Paper>
      ) : null}

      {visibleHistory?.items.length ? (
        <Stack gap="sm">
          {visibleHistory.items.map((item) => (
            <TransformationCard
              item={item}
              key={item.id}
              ruleTitles={ruleTitles}
            />
          ))}
        </Stack>
      ) : null}

      {visibleHistory && (filtered || visibleHistory.pagination.totalPages > 1) ? (
        <Group justify="space-between" align="center">
          <Text c="dimmed" size="sm">
            {filtered ? "Найдено записей" : "Всего записей"}: {visibleHistory.pagination.total}
          </Text>
          {visibleHistory.pagination.totalPages > 1 ? <Pagination
            value={page}
            total={visibleHistory.pagination.totalPages}
            onChange={setPage}
            withEdges
          /> : null}
        </Group>
      ) : null}
    </Stack>
  );
}

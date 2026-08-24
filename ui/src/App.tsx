import {
  Badge,
  Container,
  Group,
  MantineProvider,
  Stack,
  Tabs,
  Title,
  createTheme,
} from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import { HistoryPage } from "./HistoryPage.js";
import { RulesPage } from "./RulesPage.js";
import { VoiceEditorPage } from "./VoiceEditorPage.js";
import type {
  RulesResponse,
  TransformationRuleState,
} from "./types.js";

const theme = createTheme({
  primaryColor: "cyan",
  defaultRadius: "md",
  fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif",
  headings: {
    fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif",
    fontWeight: "700",
  },
});

function AppContent() {
  const [rules, setRules] = useState<readonly TransformationRuleState[]>([]);
  const [rulesLoading, setRulesLoading] = useState(true);
  const [rulesError, setRulesError] = useState<string>();
  const [rulesRefreshToken, setRulesRefreshToken] = useState(0);
  const [pendingRuleIds, setPendingRuleIds] = useState<ReadonlySet<string>>(
    new Set(),
  );

  const loadRules = useCallback(async (signal: AbortSignal) => {
    setRulesLoading(true);
    setRulesError(undefined);
    try {
      const response = await fetch("/api/rules", { signal });
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }
      const result = (await response.json()) as RulesResponse;
      setRules(result.items);
    } catch (requestError) {
      if (
        requestError instanceof DOMException &&
        requestError.name === "AbortError"
      ) {
        return;
      }
      setRulesError(
        requestError instanceof Error
          ? requestError.message
          : "Не удалось загрузить правила",
      );
    } finally {
      if (!signal.aborted) {
        setRulesLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void loadRules(controller.signal);
    return () => controller.abort();
  }, [loadRules, rulesRefreshToken]);

  const updateRule = useCallback(async (ruleId: string, enabled: boolean) => {
    setPendingRuleIds((current) => new Set(current).add(ruleId));
    setRulesError(undefined);

    try {
      const response = await fetch(`/api/rules/${encodeURIComponent(ruleId)}`, {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }

      const updatedRule = (await response.json()) as TransformationRuleState;
      setRules((current) =>
        current.map((rule) => (rule.id === updatedRule.id ? updatedRule : rule)),
      );
    } catch (requestError) {
      setRulesError(
        requestError instanceof Error
          ? requestError.message
          : "Не удалось сохранить правило",
      );
    } finally {
      setPendingRuleIds((current) => {
        const next = new Set(current);
        next.delete(ruleId);
        return next;
      });
    }
  }, []);

  return (
    <Container size="lg" py={{ base: "xl", md: 48 }}>
      <Stack gap="xl">
        <Group gap="sm">
          <Title order={1}>Handy Processor</Title>
          <Badge color="teal" variant="dot">
            localhost
          </Badge>
        </Group>

        <Tabs defaultValue="editor" keepMounted>
          <Tabs.List>
            <Tabs.Tab value="editor">Голосовой редактор</Tabs.Tab>
            <Tabs.Tab value="history">История</Tabs.Tab>
            <Tabs.Tab value="rules">Список правил</Tabs.Tab>
          </Tabs.List>

          <Tabs.Panel value="editor">
            <VoiceEditorPage />
          </Tabs.Panel>
          <Tabs.Panel value="history">
            <HistoryPage rules={rules} />
          </Tabs.Panel>
          <Tabs.Panel value="rules">
            <RulesPage
              rules={rules}
              loading={rulesLoading}
              error={rulesError}
              pendingRuleIds={pendingRuleIds}
              onReload={() => setRulesRefreshToken((value) => value + 1)}
              onToggle={(ruleId, enabled) => {
                void updateRule(ruleId, enabled);
              }}
            />
          </Tabs.Panel>
        </Tabs>
      </Stack>
    </Container>
  );
}

export function App() {
  return (
    <MantineProvider theme={theme} defaultColorScheme="auto">
      <AppContent />
    </MantineProvider>
  );
}

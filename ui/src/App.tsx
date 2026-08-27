import {
  Container,
  MantineProvider,
  Tabs,
  createTheme,
  useMantineColorScheme,
} from "@mantine/core";
import { useLocalStorage } from "@mantine/hooks";
import { useCallback, useEffect, useMemo, useState } from "react";
import { HistoryPage } from "./HistoryPage.js";
import { RulesPage } from "./RulesPage.js";
import { SettingsPage } from "./SettingsPage.js";
import { VoiceEditorPage } from "./VoiceEditorPage.js";
import type {
  EditorFontSize,
  FontSizePreference,
  RulesResponse,
  TransformationRuleState,
  UiFontSize,
} from "./types.js";

const FONT_FAMILY = "Inter, ui-sans-serif, system-ui, sans-serif";
const FONT_SCALE: Readonly<Record<UiFontSize, number>> = {
  small: 0.875,
  medium: 1,
  large: 1.125,
};

function scaledRem(pixels: number, preference: UiFontSize): string {
  return `${(pixels * FONT_SCALE[preference]) / 16}rem`;
}

function createAppTheme(preference: UiFontSize) {
  return createTheme({
    primaryColor: "cyan",
    defaultRadius: "md",
    fontFamily: FONT_FAMILY,
    fontSizes: {
      xs: scaledRem(12, preference),
      sm: scaledRem(14, preference),
      md: scaledRem(16, preference),
      lg: scaledRem(18, preference),
      xl: scaledRem(20, preference),
    },
    headings: {
      fontFamily: FONT_FAMILY,
      fontWeight: "700",
      sizes: {
        h1: { fontSize: scaledRem(34, preference) },
        h2: { fontSize: scaledRem(26, preference) },
        h3: { fontSize: scaledRem(22, preference) },
        h4: { fontSize: scaledRem(18, preference) },
        h5: { fontSize: scaledRem(16, preference) },
        h6: { fontSize: scaledRem(14, preference) },
      },
    },
  });
}

function deserializeFontSize(value: string | undefined): FontSizePreference {
  return value === "small" || value === "large" ? value : "medium";
}

function AppContent({
  uiFontSize,
  onUiFontSizeChange,
}: {
  readonly uiFontSize: UiFontSize;
  readonly onUiFontSizeChange: (value: UiFontSize) => void;
}) {
  const { colorScheme, setColorScheme } = useMantineColorScheme({
    keepTransitions: true,
  });
  const [editorFontSize, setEditorFontSize] = useLocalStorage<EditorFontSize>({
    key: "handy-processor-editor-font-size",
    defaultValue: "medium",
    getInitialValueInEffect: false,
    serialize: (value) => value,
    deserialize: deserializeFontSize,
  });
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
    <Container className="app-container" size="lg" py={0}>
      <Tabs defaultValue="editor" keepMounted>
        <Tabs.List>
          <Tabs.Tab value="editor">Голосовой редактор</Tabs.Tab>
          <Tabs.Tab value="history">История</Tabs.Tab>
          <Tabs.Tab value="rules">Список правил</Tabs.Tab>
          <Tabs.Tab value="settings">Настройки</Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="editor">
          <VoiceEditorPage editorFontSize={editorFontSize} />
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
        <Tabs.Panel value="settings">
          <SettingsPage
            colorScheme={colorScheme}
            editorFontSize={editorFontSize}
            uiFontSize={uiFontSize}
            onColorSchemeChange={setColorScheme}
            onEditorFontSizeChange={setEditorFontSize}
            onUiFontSizeChange={onUiFontSizeChange}
          />
        </Tabs.Panel>
      </Tabs>
    </Container>
  );
}

export function App() {
  const [uiFontSize, setUiFontSize] = useLocalStorage<UiFontSize>({
    key: "handy-processor-ui-font-size",
    defaultValue: "medium",
    getInitialValueInEffect: false,
    serialize: (value) => value,
    deserialize: deserializeFontSize,
  });
  const theme = useMemo(() => createAppTheme(uiFontSize), [uiFontSize]);

  return (
    <MantineProvider theme={theme} defaultColorScheme="auto">
      <AppContent
        uiFontSize={uiFontSize}
        onUiFontSizeChange={setUiFontSize}
      />
    </MantineProvider>
  );
}

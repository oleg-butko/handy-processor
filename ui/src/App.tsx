import {
  Container,
  MantineProvider,
  Tabs,
  createTheme,
} from "@mantine/core";
import { useLocalStorage } from "@mantine/hooks";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
} from "react";
import { HistoryPage } from "./HistoryPage.js";
import { RulesPage } from "./RulesPage.js";
import { SettingsPage } from "./SettingsPage.js";
import { VoiceEditorPage } from "./VoiceEditorPage.js";
import type {
  EditorFontSize,
  FontSizePreference,
  RulesResponse,
  ThemePreference,
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

function deserializeHistoryLimit(value: string | undefined): number {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 10 && parsed <= 1_000
    ? parsed
    : 100;
}

function deserializeTheme(value: string | undefined): ThemePreference {
  return value === "soft-light" || value === "soft-dark" || value === "dark"
    ? value
    : "light";
}

function AppContent({
  uiFontSize,
  onUiFontSizeChange,
  historyLimit,
  onHistoryLimitChange,
  themePreference,
  onThemePreferenceChange,
}: {
  readonly uiFontSize: UiFontSize;
  readonly onUiFontSizeChange: (value: UiFontSize) => void;
  readonly historyLimit: number;
  readonly onHistoryLimitChange: (value: number) => void;
  readonly themePreference: ThemePreference;
  readonly onThemePreferenceChange: (value: ThemePreference) => void;
}) {
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
      <Tabs className="app-tabs" defaultValue="editor" keepMounted>
        <Tabs.List>
          <Tabs.Tab value="editor">Голосовой редактор</Tabs.Tab>
          <Tabs.Tab value="history">История</Tabs.Tab>
          <Tabs.Tab value="rules">Список правил</Tabs.Tab>
          <Tabs.Tab value="settings">Настройки</Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel className="app-tab-panel voice-editor-panel" value="editor">
          <VoiceEditorPage
            editorFontSize={editorFontSize}
            historyLimit={historyLimit}
          />
        </Tabs.Panel>
        <Tabs.Panel className="app-tab-panel" value="history">
          <HistoryPage rules={rules} />
        </Tabs.Panel>
        <Tabs.Panel className="app-tab-panel" value="rules">
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
        <Tabs.Panel className="app-tab-panel" value="settings">
          <SettingsPage
            editorFontSize={editorFontSize}
            uiFontSize={uiFontSize}
            historyLimit={historyLimit}
            themePreference={themePreference}
            onEditorFontSizeChange={setEditorFontSize}
            onUiFontSizeChange={onUiFontSizeChange}
            onHistoryLimitChange={onHistoryLimitChange}
            onThemePreferenceChange={onThemePreferenceChange}
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
  const [historyLimit, setHistoryLimit] = useLocalStorage<number>({
    key: "handy-processor-editor-history-limit",
    defaultValue: 100,
    getInitialValueInEffect: false,
    serialize: String,
    deserialize: deserializeHistoryLimit,
  });
  const [themePreference, setThemePreference] =
    useLocalStorage<ThemePreference>({
      key: "handy-processor-theme",
      defaultValue: "light",
      getInitialValueInEffect: false,
      serialize: String,
      deserialize: deserializeTheme,
    });
  const theme = useMemo(() => createAppTheme(uiFontSize), [uiFontSize]);
  const colorScheme =
    themePreference === "dark" || themePreference === "soft-dark"
      ? "dark"
      : "light";

  useLayoutEffect(() => {
    document.documentElement.dataset.hpTheme = themePreference;
  }, [themePreference]);

  return (
    <MantineProvider
      defaultColorScheme="light"
      forceColorScheme={colorScheme}
      theme={theme}
    >
      <AppContent
        uiFontSize={uiFontSize}
        historyLimit={historyLimit}
        themePreference={themePreference}
        onUiFontSizeChange={setUiFontSize}
        onHistoryLimitChange={setHistoryLimit}
        onThemePreferenceChange={setThemePreference}
      />
    </MantineProvider>
  );
}

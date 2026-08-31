import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Modal,
  NumberInput,
  SegmentedControl,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useEffect, useState } from "react";
import type {
  EditorFontSize,
  ThemePreference,
  TransformationHistorySettings,
  UiFontSize,
} from "./types.js";
import {
  isLauncherWebView,
  saveHistoryExportInLauncher,
} from "./launcher-bridge.js";

const DEFAULT_TRANSFORMATION_HISTORY_LIMIT = 50_000;
const MAX_TRANSFORMATION_HISTORY_LIMIT = 1_000_000;

interface SettingsPageProps {
  readonly editorFontSize: EditorFontSize;
  readonly uiFontSize: UiFontSize;
  readonly historyLimit: number;
  readonly themePreference: ThemePreference;
  readonly onEditorFontSizeChange: (value: EditorFontSize) => void;
  readonly onUiFontSizeChange: (value: UiFontSize) => void;
  readonly onHistoryLimitChange: (value: number) => void;
  readonly onThemePreferenceChange: (value: ThemePreference) => void;
}

export function SettingsPage({
  editorFontSize,
  uiFontSize,
  historyLimit,
  themePreference,
  onEditorFontSizeChange,
  onUiFontSizeChange,
  onHistoryLimitChange,
  onThemePreferenceChange,
}: SettingsPageProps) {
  const [transformationHistoryLimit, setTransformationHistoryLimit] = useState(
    DEFAULT_TRANSFORMATION_HISTORY_LIMIT,
  );
  const [savedTransformationHistoryLimit, setSavedTransformationHistoryLimit] =
    useState(DEFAULT_TRANSFORMATION_HISTORY_LIMIT);
  const [historySettingsLoading, setHistorySettingsLoading] = useState(true);
  const [historySettingsSaving, setHistorySettingsSaving] = useState(false);
  const [historySettingsError, setHistorySettingsError] = useState<string>();
  const [historyLimitConfirmationOpened, setHistoryLimitConfirmationOpened] =
    useState(false);
  const [exportingHistory, setExportingHistory] = useState(false);
  const [exportError, setExportError] = useState<string>();
  const [exportResult, setExportResult] = useState<string>();

  useEffect(() => {
    const controller = new AbortController();
    void (async () => {
      setHistorySettingsLoading(true);
      setHistorySettingsError(undefined);
      try {
        const response = await fetch("/api/settings/history", {
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`Сервер вернул HTTP ${response.status}`);
        }
        const settings =
          (await response.json()) as TransformationHistorySettings;
        setTransformationHistoryLimit(settings.maxEntries);
        setSavedTransformationHistoryLimit(settings.maxEntries);
      } catch (requestError) {
        if (
          requestError instanceof DOMException &&
          requestError.name === "AbortError"
        ) {
          return;
        }
        setHistorySettingsError(
          requestError instanceof Error
            ? requestError.message
            : "Не удалось загрузить настройку истории",
        );
      } finally {
        if (!controller.signal.aborted) setHistorySettingsLoading(false);
      }
    })();
    return () => controller.abort();
  }, []);

  async function saveTransformationHistoryLimit(): Promise<void> {
    setHistorySettingsSaving(true);
    setHistorySettingsError(undefined);
    try {
      const response = await fetch("/api/settings/history", {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ maxEntries: transformationHistoryLimit }),
      });
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }
      const settings = (await response.json()) as TransformationHistorySettings;
      setTransformationHistoryLimit(settings.maxEntries);
      setSavedTransformationHistoryLimit(settings.maxEntries);
      setHistoryLimitConfirmationOpened(false);
    } catch (requestError) {
      setHistoryLimitConfirmationOpened(false);
      setHistorySettingsError(
        requestError instanceof Error
          ? requestError.message
          : "Не удалось сохранить настройку истории",
      );
    } finally {
      setHistorySettingsSaving(false);
    }
  }

  async function exportTransformationHistory(): Promise<void> {
    setExportingHistory(true);
    setExportError(undefined);
    setExportResult(undefined);
    try {
      if (isLauncherWebView()) {
        const path = await saveHistoryExportInLauncher();
        setExportResult(`Файл сохранён: ${path}`);
        return;
      }

      const response = await fetch("/api/transformations/export");
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }

      const blobUrl = URL.createObjectURL(await response.blob());
      const link = document.createElement("a");
      link.href = blobUrl;
      link.download = `handy-processor-history-${new Date().toISOString().slice(0, 10)}.txt`;
      link.hidden = true;
      document.body.append(link);
      try {
        link.click();
        setExportResult("Загрузка файла началась");
      } finally {
        link.remove();
        setTimeout(() => URL.revokeObjectURL(blobUrl), 1_000);
      }
    } catch (requestError) {
      setExportError(
        requestError instanceof Error
          ? requestError.message
          : "Не удалось экспортировать историю",
      );
    } finally {
      setExportingHistory(false);
    }
  }

  return (
    <Stack gap="md" pt="md">
      <Stack gap={2}>
        <Title order={2}>Настройки</Title>
        <Text c="dimmed" size="sm">
          Интерфейс сохраняется локально в окне приложения, а параметры истории
          — в SQLite рядом с executable.
        </Text>
      </Stack>

      <Card className="hp-surface" withBorder radius="md" padding="md">
        <Stack gap="md">
          <Group
            className="settings-row"
            justify="space-between"
            align="center"
            wrap="nowrap"
          >
            <Stack gap={2}>
              <Text fw={600}>Тема</Text>
              <Text c="dimmed" size="xs">
                Четыре фиксированных уровня яркости интерфейса.
              </Text>
            </Stack>
            <SegmentedControl
              size="xs"
              value={themePreference}
              data={[
                { value: "light", label: "Светлая" },
                { value: "soft-light", label: "Светлая 2" },
                { value: "soft-dark", label: "Тёмная 2" },
                { value: "dark", label: "Тёмная" },
              ]}
              onChange={(value) =>
                onThemePreferenceChange(value as ThemePreference)
              }
            />
          </Group>

          <Group
            className="settings-row"
            justify="space-between"
            align="center"
            wrap="nowrap"
          >
            <Stack gap={2}>
              <Text fw={600}>История отмены и повтора</Text>
              <Text c="dimmed" size="xs">
                Сколько последних изменений хранить для каждой заметки.
              </Text>
            </Stack>
            <NumberInput
              aria-label="Количество изменений undo и redo"
              allowDecimal={false}
              allowNegative={false}
              clampBehavior="strict"
              max={1000}
              min={10}
              size="xs"
              step={10}
              value={historyLimit}
              w={110}
              onChange={(value) => {
                if (typeof value === "number") onHistoryLimitChange(value);
              }}
            />
          </Group>

          <Group
            className="settings-row"
            justify="space-between"
            align="center"
            wrap="nowrap"
          >
            <Stack gap={2}>
              <Text fw={600}>Размер текста интерфейса</Text>
              <Text c="dimmed" size="xs">
                Меняет текст вкладок, кнопок, карточек и заголовков.
              </Text>
            </Stack>
            <SegmentedControl
              size="xs"
              value={uiFontSize}
              data={[
                { value: "small", label: "Мелкий" },
                { value: "medium", label: "Обычный" },
                { value: "large", label: "Крупный" },
              ]}
              onChange={(value) => onUiFontSizeChange(value as UiFontSize)}
            />
          </Group>

          <Group
            className="settings-row"
            justify="space-between"
            align="center"
            wrap="nowrap"
          >
            <Stack gap={2}>
              <Text fw={600}>Размер текста редактора</Text>
              <Text c="dimmed" size="xs">
                Меняет только рабочий текст, не элементы управления.
              </Text>
            </Stack>
            <SegmentedControl
              size="xs"
              value={editorFontSize}
              data={[
                { value: "small", label: "Мелкий" },
                { value: "medium", label: "Обычный" },
                { value: "large", label: "Крупный" },
              ]}
              onChange={(value) =>
                onEditorFontSizeChange(value as EditorFontSize)
              }
            />
          </Group>
        </Stack>
      </Card>

      <Card className="hp-surface" withBorder radius="md" padding="md">
        <Stack gap="sm">
          <Stack gap={2}>
            <Text fw={600}>История преобразований</Text>
            <Text c="dimmed" size="xs">
              Старые записи сверх лимита удаляются без возможности
              восстановления. По умолчанию хранится 50 000 записей.
            </Text>
          </Stack>

          {historySettingsError ? (
            <Alert color="red" title="Не удалось обновить настройку">
              {historySettingsError}
            </Alert>
          ) : null}

          <Group gap="xs" align="flex-end">
            <NumberInput
              aria-label="Максимальное количество записей истории"
              allowDecimal={false}
              allowNegative={false}
              clampBehavior="strict"
              disabled={historySettingsLoading || historySettingsSaving}
              label="Максимальное количество записей"
              max={MAX_TRANSFORMATION_HISTORY_LIMIT}
              min={1}
              step={1_000}
              value={transformationHistoryLimit}
              w={240}
              onChange={(value) => {
                if (typeof value === "number") {
                  setTransformationHistoryLimit(value);
                }
              }}
            />
            <Button
              disabled={
                historySettingsLoading ||
                transformationHistoryLimit === savedTransformationHistoryLimit
              }
              loading={historySettingsSaving}
              onClick={() => {
                if (
                  transformationHistoryLimit < savedTransformationHistoryLimit
                ) {
                  setHistoryLimitConfirmationOpened(true);
                } else {
                  void saveTransformationHistoryLimit();
                }
              }}
            >
              Сохранить
            </Button>
          </Group>
        </Stack>
      </Card>

      <Card className="hp-surface" withBorder radius="md" padding="md">
        <Stack gap="sm">
          <Group gap="xs">
            <Text fw={600}>Экспортировать историю как текстовый файл</Text>
            <Badge color="orange" size="sm" variant="light">
              Экспериментально
            </Badge>
          </Group>
          <Text c="dimmed" size="xs">
            В файл попадут только исходный текст и результат. Между ними будет
            строка «--- РЕЗУЛЬТАТ ---»; служебные данные и правила не
            экспортируются.
          </Text>
          {exportError ? (
            <Alert color="red" title="Не удалось экспортировать историю">
              {exportError}
            </Alert>
          ) : null}
          {exportResult ? (
            <Alert color="teal" title="Экспорт завершён">
              {exportResult}
            </Alert>
          ) : null}
          <Group>
            <Button
              loading={exportingHistory}
              variant="light"
              onClick={() => void exportTransformationHistory()}
            >
              Скачать текстовый файл
            </Button>
          </Group>
        </Stack>
      </Card>

      <Modal
        centered
        opened={historyLimitConfirmationOpened}
        size="sm"
        title="Уменьшить лимит истории?"
        onClose={() => setHistoryLimitConfirmationOpened(false)}
      >
        <Stack gap="md">
          <Text size="sm">
            Лимит уменьшится с{" "}
            {savedTransformationHistoryLimit.toLocaleString("ru-RU")} до{" "}
            {transformationHistoryLimit.toLocaleString("ru-RU")} записей. Самые
            старые записи сверх нового лимита будут сразу удалены.
          </Text>
          <Text c="red" size="sm">
            Это действие нельзя отменить.
          </Text>
          <Group justify="flex-end">
            <Button
              variant="default"
              onClick={() => setHistoryLimitConfirmationOpened(false)}
            >
              Отмена
            </Button>
            <Button
              color="red"
              loading={historySettingsSaving}
              onClick={() => void saveTransformationHistoryLimit()}
            >
              Уменьшить и удалить старые записи
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Stack>
  );
}

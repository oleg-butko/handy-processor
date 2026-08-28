import {
  Card,
  Group,
  NumberInput,
  SegmentedControl,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import type {
  EditorFontSize,
  ThemePreference,
  UiFontSize,
} from "./types.js";

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
  return (
    <Stack gap="md" pt="md">
      <Stack gap={2}>
        <Title order={2}>Настройки интерфейса</Title>
        <Text c="dimmed" size="sm">
          Сохраняются локально для этого браузера или окна приложения.
        </Text>
      </Stack>

      <Card withBorder radius="md" padding="md">
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
    </Stack>
  );
}

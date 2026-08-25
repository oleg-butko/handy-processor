import {
  Card,
  Group,
  SegmentedControl,
  Stack,
  Text,
  Title,
  type MantineColorScheme,
} from "@mantine/core";
import type { EditorFontSize } from "./types.js";

interface SettingsPageProps {
  readonly colorScheme: MantineColorScheme;
  readonly editorFontSize: EditorFontSize;
  readonly onColorSchemeChange: (value: MantineColorScheme) => void;
  readonly onEditorFontSizeChange: (value: EditorFontSize) => void;
}

export function SettingsPage({
  colorScheme,
  editorFontSize,
  onColorSchemeChange,
  onEditorFontSizeChange,
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
                Можно следовать настройке Windows или выбрать тему явно.
              </Text>
            </Stack>
            <SegmentedControl
              size="xs"
              value={colorScheme}
              data={[
                { value: "auto", label: "Системная" },
                { value: "light", label: "Светлая" },
                { value: "dark", label: "Тёмная" },
              ]}
              onChange={(value) =>
                onColorSchemeChange(value as MantineColorScheme)
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

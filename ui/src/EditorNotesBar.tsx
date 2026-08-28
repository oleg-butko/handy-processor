import {
  Button,
  Group,
  Modal,
  Paper,
  Select,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useMemo, useState } from "react";
import type { EditorNoteSummary } from "./types.js";

function pluralize(
  value: number,
  one: string,
  few: string,
  many: string,
): string {
  const modulo100 = value % 100;
  if (modulo100 >= 11 && modulo100 <= 14) {
    return many;
  }
  const modulo10 = value % 10;
  return modulo10 === 1 ? one : modulo10 >= 2 && modulo10 <= 4 ? few : many;
}

function formatRelativeTime(timestamp: string, now = Date.now()): string {
  const parsed = Date.parse(timestamp);
  if (!Number.isFinite(parsed)) {
    return timestamp;
  }
  const elapsedMinutes = Math.max(0, Math.floor((now - parsed) / 60_000));
  if (elapsedMinutes < 1) {
    return "только что";
  }
  if (elapsedMinutes < 60) {
    const unit = pluralize(elapsedMinutes, "минуту", "минуты", "минут");
    return `${elapsedMinutes} ${unit} назад`;
  }
  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) {
    const unit = pluralize(elapsedHours, "час", "часа", "часов");
    return `${elapsedHours} ${unit} назад`;
  }
  const elapsedDays = Math.floor(elapsedHours / 24);
  if (elapsedDays <= 3) {
    const unit = pluralize(elapsedDays, "день", "дня", "дней");
    return `${elapsedDays} ${unit} назад`;
  }
  return new Intl.DateTimeFormat("ru-RU", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(parsed);
}

export function EditorNotesBar({
  notes,
  currentNoteId,
  busy,
  error,
  onSelect,
  onSaveAs,
}: {
  readonly notes: readonly EditorNoteSummary[];
  readonly currentNoteId: number | null;
  readonly busy: boolean;
  readonly error?: string;
  readonly onSelect: (noteId: number) => void;
  readonly onSaveAs: (name: string) => Promise<boolean>;
}) {
  const [opened, setOpened] = useState(false);
  const [name, setName] = useState("");
  const options = useMemo(
    () =>
      notes.map((note) => ({
        value: String(note.id),
        label: `${note.name} · ${formatRelativeTime(note.updatedAt)}`,
      })),
    [notes],
  );

  const save = async () => {
    const normalizedName = name.trim();
    if (normalizedName === "" || busy) {
      return;
    }
    if (await onSaveAs(normalizedName)) {
      setOpened(false);
      setName("");
    }
  };

  return (
    <>
      {error ? (
        <Text c="red" lineClamp={1} size="xs">
          {error}
        </Text>
      ) : null}

      <Paper className="editor-notes-bar" p={6} radius="md" withBorder>
        <Group gap="xs" h="100%" wrap="nowrap">
          <Select
            aria-label="Сохранённые заметки"
            className="editor-note-select"
            data={options}
            disabled={busy}
            nothingFoundMessage="Сохранённых заметок пока нет"
            placeholder="Выберите сохранённую заметку"
            searchable
            size="xs"
            value={currentNoteId === null ? null : String(currentNoteId)}
            onChange={(value) => {
              if (value !== null) {
                onSelect(Number(value));
              }
            }}
          />
          <Button
            loading={busy}
            size="xs"
            variant="light"
            onClick={() => setOpened(true)}
          >
            Сохранить как
          </Button>
        </Group>
      </Paper>

      <Modal
        centered
        opened={opened}
        size="sm"
        title="Сохранить заметку как"
        onClose={() => setOpened(false)}
      >
        <Stack gap="sm">
          <TextInput
            autoFocus
            data-autofocus
            label="Название"
            maxLength={100}
            placeholder="Например, План встречи"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void save();
              }
            }}
          />
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setOpened(false)}>
              Отмена
            </Button>
            <Button
              disabled={name.trim() === ""}
              loading={busy}
              onClick={() => void save()}
            >
              Сохранить
            </Button>
          </Group>
        </Stack>
      </Modal>
    </>
  );
}

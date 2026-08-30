import {
  ActionIcon,
  Button,
  Group,
  Modal,
  Paper,
  Select,
  Stack,
  Text,
  TextInput,
  Tooltip,
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

function PencilIcon() {
  return (
    <svg aria-hidden="true" fill="none" height="16" viewBox="0 0 24 24" width="16">
      <path
        d="M4 20h4l10.5-10.5a2.8 2.8 0 0 0-4-4L4 16v4Zm9-13 4 4"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
      />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg aria-hidden="true" fill="none" height="16" viewBox="0 0 24 24" width="16">
      <path
        d="M4 7h16M9 7V4h6v3m3 0-1 13H7L6 7m4 4v5m4-5v5"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
      />
    </svg>
  );
}

export function EditorNotesBar({
  notes,
  currentNoteId,
  busy,
  error,
  onSelect,
  onSaveAs,
  onRename,
  onDelete,
}: {
  readonly notes: readonly EditorNoteSummary[];
  readonly currentNoteId: number | null;
  readonly busy: boolean;
  readonly error?: string;
  readonly onSelect: (noteId: number) => void;
  readonly onSaveAs: (name: string) => Promise<boolean>;
  readonly onRename: (name: string) => Promise<boolean>;
  readonly onDelete: () => Promise<boolean>;
}) {
  const [saveAsOpened, setSaveAsOpened] = useState(false);
  const [saveAsName, setSaveAsName] = useState("");
  const [renameOpened, setRenameOpened] = useState(false);
  const [renameName, setRenameName] = useState("");
  const [deleteOpened, setDeleteOpened] = useState(false);
  const currentNote = notes.find((note) => note.id === currentNoteId);
  const options = useMemo(
    () =>
      notes.map((note) => ({
        value: String(note.id),
        label: `${note.name} · ${formatRelativeTime(note.updatedAt)}`,
      })),
    [notes],
  );

  const saveAs = async () => {
    const normalizedName = saveAsName.trim();
    if (normalizedName === "" || busy) {
      return;
    }
    if (await onSaveAs(normalizedName)) {
      setSaveAsOpened(false);
      setSaveAsName("");
    }
  };

  const rename = async () => {
    const normalizedName = renameName.trim();
    if (normalizedName === "" || currentNote === undefined || busy) {
      return;
    }
    if (await onRename(normalizedName)) {
      setRenameOpened(false);
    }
  };

  const deleteNote = async () => {
    if (currentNote === undefined || busy) {
      return;
    }
    if (await onDelete()) {
      setDeleteOpened(false);
    }
  };

  return (
    <>
      {error ? (
        <Text c="red" lineClamp={1} size="xs">
          {error}
        </Text>
      ) : null}

      <Paper className="editor-notes-bar hp-surface" p={6} radius="md" withBorder>
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
          <Group gap={4} wrap="nowrap">
            <Tooltip label="Переименовать заметку">
              <ActionIcon
                aria-label="Переименовать заметку"
                disabled={currentNote === undefined || busy}
                size="lg"
                variant="subtle"
                onClick={() => {
                  if (currentNote !== undefined) {
                    setRenameName(currentNote.name);
                    setRenameOpened(true);
                  }
                }}
              >
                <PencilIcon />
              </ActionIcon>
            </Tooltip>
            <Tooltip label="Удалить заметку">
              <ActionIcon
                aria-label="Удалить заметку"
                color="red"
                disabled={currentNote === undefined || busy}
                size="lg"
                variant="subtle"
                onClick={() => setDeleteOpened(true)}
              >
                <TrashIcon />
              </ActionIcon>
            </Tooltip>
            <Button
              loading={busy}
              size="xs"
              variant="light"
              onClick={() => setSaveAsOpened(true)}
            >
              Сохранить как
            </Button>
          </Group>
        </Group>
      </Paper>

      <Modal
        centered
        opened={saveAsOpened}
        size="sm"
        title="Сохранить заметку как"
        onClose={() => setSaveAsOpened(false)}
      >
        <Stack gap="sm">
          <TextInput
            autoFocus
            data-autofocus
            label="Название"
            maxLength={100}
            placeholder="Например, План встречи"
            value={saveAsName}
            onChange={(event) => setSaveAsName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void saveAs();
              }
            }}
          />
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setSaveAsOpened(false)}>
              Отмена
            </Button>
            <Button
              disabled={saveAsName.trim() === ""}
              loading={busy}
              onClick={() => void saveAs()}
            >
              Сохранить
            </Button>
          </Group>
        </Stack>
      </Modal>

      <Modal
        centered
        opened={renameOpened}
        size="sm"
        title="Переименовать заметку"
        onClose={() => setRenameOpened(false)}
      >
        <Stack gap="sm">
          <TextInput
            autoFocus
            data-autofocus
            label="Название"
            maxLength={100}
            value={renameName}
            onChange={(event) => setRenameName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void rename();
              }
            }}
          />
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setRenameOpened(false)}>
              Отмена
            </Button>
            <Button
              disabled={renameName.trim() === ""}
              loading={busy}
              onClick={() => void rename()}
            >
              Сохранить
            </Button>
          </Group>
        </Stack>
      </Modal>

      <Modal
        centered
        opened={deleteOpened}
        size="sm"
        title="Удалить заметку"
        onClose={() => setDeleteOpened(false)}
      >
        <Stack gap="md">
          <Text>
            Уверены, что хотите удалить «{currentNote?.name ?? ""}»?
          </Text>
          <Text c="dimmed" size="sm">
            Вместе с заметкой будет удалена вся история её редактирования.
          </Text>
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setDeleteOpened(false)}>
              Отмена
            </Button>
            <Button
              color="red"
              loading={busy}
              onClick={() => void deleteNote()}
            >
              Удалить
            </Button>
          </Group>
        </Stack>
      </Modal>
    </>
  );
}

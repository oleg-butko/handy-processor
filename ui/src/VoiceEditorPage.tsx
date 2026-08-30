import {
  Alert,
  Button,
  Center,
  Group,
  Loader,
  Paper,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { EditorNotesBar } from "./EditorNotesBar.js";
import {
  PlainTextEditor,
  type EditorDocumentSnapshot,
  type PlainTextEditorHandle,
  type PlainTextEditorState,
} from "./PlainTextEditor.js";
import type {
  EditorDraft,
  EditorFontSize,
  EditorNote,
  EditorNoteSummary,
  EditorNotesResponse,
  EditorSessionState,
} from "./types.js";

const SESSION_HEARTBEAT_MS = 5_000;
const SESSION_SYNC_DELAY_MS = 75;
const EDITOR_SAVE_DELAY_MS = 600;

interface LocalEditorState extends PlainTextEditorState {
  readonly active: boolean;
}

interface PendingEditorSave extends EditorDocumentSnapshot {
  readonly noteId: number | null;
  readonly saveNote: boolean;
  readonly saveId: number;
}

function createSessionId(): string {
  return typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `editor-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function countWords(text: string): number {
  const trimmed = text.trim();
  return trimmed === "" ? 0 : trimmed.split(/\s+/u).length;
}

function sortNotes(notes: readonly EditorNoteSummary[]): EditorNoteSummary[] {
  return [...notes].sort(
    (left, right) =>
      Date.parse(right.updatedAt) - Date.parse(left.updatedAt) ||
      right.id - left.id,
  );
}

export function VoiceEditorPage({
  editorFontSize,
  historyLimit,
}: {
  readonly editorFontSize: EditorFontSize;
  readonly historyLimit: number;
}) {
  const editorRef = useRef<PlainTextEditorHandle>(null);
  const sessionIdRef = useRef(createSessionId());
  const sessionRevisionRef = useRef(0);
  const sessionRequestIdRef = useRef(0);
  const stateRef = useRef<LocalEditorState | undefined>(undefined);
  const currentNoteIdRef = useRef<number | null>(null);
  const notesBusyRef = useRef(false);
  const previousHistoryLimitRef = useRef(historyLimit);
  const suppressAutosaveRef = useRef(false);
  const sessionSyncTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const editorSaveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const pendingSaveRef = useRef<PendingEditorSave | undefined>(undefined);
  const latestSaveRef = useRef(0);
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve());
  const [draft, setDraft] = useState<EditorDraft>();
  const [notes, setNotes] = useState<readonly EditorNoteSummary[]>([]);
  const [currentNoteId, setCurrentNoteId] = useState<number | null>(null);
  const [notesBusy, setNotesBusy] = useState(false);
  const [noteError, setNoteError] = useState<string>();
  const [loadError, setLoadError] = useState<string>();
  const [sessionError, setSessionError] = useState<string>();
  const [saveState, setSaveState] = useState<"saved" | "saving" | "error">(
    "saved",
  );
  const [editorState, setEditorState] = useState<LocalEditorState>();

  useEffect(() => {
    const controller = new AbortController();
    void (async () => {
      try {
        const [draftResponse, notesResponse] = await Promise.all([
          fetch("/api/editor/draft", { signal: controller.signal }),
          fetch("/api/editor/notes", { signal: controller.signal }),
        ]);
        if (!draftResponse.ok || !notesResponse.ok) {
          throw new Error(
            `Сервер вернул HTTP ${!draftResponse.ok ? draftResponse.status : notesResponse.status}`,
          );
        }
        const loaded = (await draftResponse.json()) as EditorDraft;
        const loadedNotes = (await notesResponse.json()) as EditorNotesResponse;
        const validCurrentNoteId = loadedNotes.items.some(
          (note) => note.id === loaded.currentNoteId,
        )
          ? loaded.currentNoteId
          : null;
        currentNoteIdRef.current = validCurrentNoteId;
        setCurrentNoteId(validCurrentNoteId);
        setNotes(sortNotes(loadedNotes.items));
        setDraft(loaded);
        const initial: LocalEditorState = {
          documentText: loaded.content,
          serializedState: loaded.state,
          selectionStart: 0,
          selectionEnd: 0,
          undoDepth: 0,
          redoDepth: 0,
          active: false,
        };
        stateRef.current = initial;
        setEditorState(initial);
      } catch (error) {
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        setLoadError(
          error instanceof Error
            ? error.message
            : "Не удалось загрузить черновик",
        );
      }
    })();
    return () => controller.abort();
  }, []);

  const publishSession = useCallback(async () => {
    const state = stateRef.current;
    if (state === undefined) return;
    const requestId = sessionRequestIdRef.current + 1;
    sessionRequestIdRef.current = requestId;
    try {
      const response = await fetch(
        `/api/editor/sessions/${encodeURIComponent(sessionIdRef.current)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            documentText: state.documentText,
            documentRevision: sessionRevisionRef.current,
            selectionStart: state.selectionStart,
            selectionEnd: state.selectionEnd,
            active: state.active,
          }),
        },
      );
      if (!response.ok) throw new Error(`Сервер вернул HTTP ${response.status}`);
      const session = (await response.json()) as EditorSessionState;
      if (
        requestId === sessionRequestIdRef.current &&
        session.sessionId === sessionIdRef.current
      ) {
        setSessionError(undefined);
      }
    } catch (error) {
      if (requestId === sessionRequestIdRef.current) {
        setSessionError(
          error instanceof Error
            ? error.message
            : "Не удалось синхронизировать контекст",
        );
      }
    }
  }, []);

  const scheduleSessionSync = useCallback(() => {
    if (sessionSyncTimerRef.current !== undefined) {
      clearTimeout(sessionSyncTimerRef.current);
    }
    sessionSyncTimerRef.current = setTimeout(() => {
      sessionSyncTimerRef.current = undefined;
      void publishSession();
    }, SESSION_SYNC_DELAY_MS);
  }, [publishSession]);

  const updateNoteSummary = useCallback((note: EditorNote) => {
    setNotes((current) =>
      sortNotes([
        { id: note.id, name: note.name, updatedAt: note.updatedAt },
        ...current.filter((item) => item.id !== note.id),
      ]),
    );
  }, []);

  const enqueueEditorSave = useCallback(
    (pending: PendingEditorSave): Promise<void> => {
      const operation = saveQueueRef.current.then(async () => {
        if (pending.saveNote && pending.noteId !== null) {
          const noteResponse = await fetch(`/api/editor/notes/${pending.noteId}`, {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ content: pending.content, state: pending.state }),
          });
          if (!noteResponse.ok) {
            throw new Error(`Сервер вернул HTTP ${noteResponse.status}`);
          }
          updateNoteSummary((await noteResponse.json()) as EditorNote);
        }
        const draftResponse = await fetch("/api/editor/draft", {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content: pending.content,
            state: pending.state,
            currentNoteId: pending.noteId,
          }),
        });
        if (!draftResponse.ok) {
          throw new Error(`Сервер вернул HTTP ${draftResponse.status}`);
        }
        await draftResponse.json();
        if (pending.saveId === latestSaveRef.current) setSaveState("saved");
      });
      const reportedOperation = operation.catch((error: unknown) => {
        if (pending.saveId === latestSaveRef.current) {
          setSaveState("error");
          setNoteError(
            error instanceof Error ? error.message : "Не удалось сохранить",
          );
        }
        throw error;
      });
      saveQueueRef.current = reportedOperation.catch(() => {});
      return reportedOperation;
    },
    [updateNoteSummary],
  );

  const createPendingSave = useCallback(
    (
      snapshot: EditorDocumentSnapshot,
      noteId: number | null,
      saveNote: boolean,
    ): PendingEditorSave => {
      const saveId = latestSaveRef.current + 1;
      latestSaveRef.current = saveId;
      return { ...snapshot, noteId, saveNote, saveId };
    },
    [],
  );

  const scheduleEditorSave = useCallback(
    (snapshot: EditorDocumentSnapshot) => {
      if (editorSaveTimerRef.current !== undefined) {
        clearTimeout(editorSaveTimerRef.current);
      }
      setSaveState("saving");
      const noteId = currentNoteIdRef.current;
      pendingSaveRef.current = createPendingSave(
        snapshot,
        noteId,
        noteId !== null,
      );
      editorSaveTimerRef.current = setTimeout(() => {
        editorSaveTimerRef.current = undefined;
        const pending = pendingSaveRef.current;
        pendingSaveRef.current = undefined;
        if (pending !== undefined) {
          void enqueueEditorSave(pending).catch(() => {});
        }
      }, EDITOR_SAVE_DELAY_MS);
    },
    [createPendingSave, enqueueEditorSave],
  );

  useEffect(() => {
    if (previousHistoryLimitRef.current === historyLimit) {
      return;
    }
    previousHistoryLimitRef.current = historyLimit;
    const timer = setTimeout(() => {
      const snapshot = editorRef.current?.snapshot();
      if (snapshot !== undefined) {
        scheduleEditorSave(snapshot);
      }
    }, 0);
    return () => clearTimeout(timer);
  }, [historyLimit, scheduleEditorSave]);

  const flushScheduledSave = useCallback(async () => {
    if (editorSaveTimerRef.current !== undefined) {
      clearTimeout(editorSaveTimerRef.current);
      editorSaveTimerRef.current = undefined;
    }
    const pending = pendingSaveRef.current;
    pendingSaveRef.current = undefined;
    if (pending !== undefined) await enqueueEditorSave(pending);
    else await saveQueueRef.current;
  }, [enqueueEditorSave]);

  const saveWorkspace = useCallback(
    (snapshot: EditorDocumentSnapshot, noteId: number | null) => {
      setSaveState("saving");
      return enqueueEditorSave(createPendingSave(snapshot, noteId, false));
    },
    [createPendingSave, enqueueEditorSave],
  );

  const handleStateChange = useCallback(
    (next: PlainTextEditorState) => {
      const previous = stateRef.current;
      const state: LocalEditorState = {
        ...next,
        active: previous?.active ?? false,
      };
      stateRef.current = state;
      sessionRevisionRef.current += 1;
      setEditorState(state);
      scheduleSessionSync();
      if (
        previous?.documentText !== next.documentText &&
        !suppressAutosaveRef.current
      ) {
        scheduleEditorSave({
          content: next.documentText,
          state: next.serializedState,
        });
      }
    },
    [scheduleEditorSave, scheduleSessionSync],
  );

  const handleFocusChange = useCallback(
    (active: boolean) => {
      const previous = stateRef.current;
      if (previous === undefined || previous.active === active) return;
      const state = { ...previous, active };
      stateRef.current = state;
      sessionRevisionRef.current += 1;
      setEditorState(state);
      void publishSession();
    },
    [publishSession],
  );

  const selectNote = useCallback(
    async (noteId: number) => {
      if (noteId === currentNoteIdRef.current || notesBusyRef.current) return;
      notesBusyRef.current = true;
      setNotesBusy(true);
      setNoteError(undefined);
      try {
        await flushScheduledSave();
        const response = await fetch(`/api/editor/notes/${noteId}`);
        if (!response.ok) throw new Error(`Сервер вернул HTTP ${response.status}`);
        const note = (await response.json()) as EditorNote;
        currentNoteIdRef.current = note.id;
        setCurrentNoteId(note.id);
        suppressAutosaveRef.current = true;
        editorRef.current?.replace({ content: note.content, state: note.state });
        suppressAutosaveRef.current = false;
        const snapshot = editorRef.current?.snapshot();
        if (snapshot !== undefined) await saveWorkspace(snapshot, note.id);
      } catch (error) {
        suppressAutosaveRef.current = false;
        setNoteError(
          error instanceof Error ? error.message : "Не удалось открыть заметку",
        );
      } finally {
        notesBusyRef.current = false;
        setNotesBusy(false);
      }
    },
    [flushScheduledSave, saveWorkspace],
  );

  const saveAs = useCallback(async (name: string): Promise<boolean> => {
    const snapshot = editorRef.current?.snapshot();
    if (snapshot === undefined || notesBusyRef.current) return false;
    notesBusyRef.current = true;
    setNotesBusy(true);
    setNoteError(undefined);
    try {
      await flushScheduledSave();
      const response = await fetch("/api/editor/notes", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name, ...snapshot }),
      });
      if (!response.ok) throw new Error(`Сервер вернул HTTP ${response.status}`);
      const note = (await response.json()) as EditorNote;
      currentNoteIdRef.current = note.id;
      setCurrentNoteId(note.id);
      updateNoteSummary(note);
      await saveWorkspace(snapshot, note.id);
      return true;
    } catch (error) {
      setNoteError(
        error instanceof Error ? error.message : "Не удалось создать заметку",
      );
      return false;
    } finally {
      notesBusyRef.current = false;
      setNotesBusy(false);
    }
  }, [flushScheduledSave, saveWorkspace, updateNoteSummary]);

  const renameCurrentNote = useCallback(
    async (name: string): Promise<boolean> => {
      const noteId = currentNoteIdRef.current;
      if (noteId === null || notesBusyRef.current) return false;
      notesBusyRef.current = true;
      setNotesBusy(true);
      setNoteError(undefined);
      try {
        await flushScheduledSave();
        const response = await fetch(`/api/editor/notes/${noteId}`, {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ name }),
        });
        if (!response.ok) throw new Error(`Сервер вернул HTTP ${response.status}`);
        updateNoteSummary((await response.json()) as EditorNote);
        return true;
      } catch (error) {
        setNoteError(
          error instanceof Error
            ? error.message
            : "Не удалось переименовать заметку",
        );
        return false;
      } finally {
        notesBusyRef.current = false;
        setNotesBusy(false);
      }
    },
    [flushScheduledSave, updateNoteSummary],
  );

  const deleteCurrentNote = useCallback(async (): Promise<boolean> => {
    const noteId = currentNoteIdRef.current;
    if (noteId === null || notesBusyRef.current) return false;
    notesBusyRef.current = true;
    setNotesBusy(true);
    setNoteError(undefined);
    try {
      await flushScheduledSave();
      const response = await fetch(`/api/editor/notes/${noteId}`, {
        method: "DELETE",
      });
      if (!response.ok) throw new Error(`Сервер вернул HTTP ${response.status}`);
      currentNoteIdRef.current = null;
      setCurrentNoteId(null);
      setNotes((current) => current.filter((note) => note.id !== noteId));
      setSaveState("saved");
      return true;
    } catch (error) {
      setNoteError(
        error instanceof Error ? error.message : "Не удалось удалить заметку",
      );
      return false;
    } finally {
      notesBusyRef.current = false;
      setNotesBusy(false);
    }
  }, [flushScheduledSave]);

  useEffect(() => {
    const interval = setInterval(() => {
      if (stateRef.current?.active) void publishSession();
    }, SESSION_HEARTBEAT_MS);
    return () => {
      clearInterval(interval);
      if (sessionSyncTimerRef.current !== undefined) {
        clearTimeout(sessionSyncTimerRef.current);
      }
      if (editorSaveTimerRef.current !== undefined) {
        clearTimeout(editorSaveTimerRef.current);
      }
      const pending = pendingSaveRef.current;
      if (pending !== undefined) {
        const body = JSON.stringify({
          content: pending.content,
          state: pending.state,
        });
        if (pending.saveNote && pending.noteId !== null) {
          void fetch(`/api/editor/notes/${pending.noteId}`, {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body,
            keepalive: true,
          });
        }
        void fetch("/api/editor/draft", {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content: pending.content,
            state: pending.state,
            currentNoteId: pending.noteId,
          }),
          keepalive: true,
        });
      }
      void fetch(
        `/api/editor/sessions/${encodeURIComponent(sessionIdRef.current)}?documentRevision=${sessionRevisionRef.current}`,
        { method: "DELETE", keepalive: true },
      );
    };
  }, [publishSession]);

  if (loadError !== undefined) {
    return (
      <Stack pt="xl">
        <Alert color="red" title="Не удалось открыть голосовой редактор">
          {loadError}
        </Alert>
      </Stack>
    );
  }
  if (draft === undefined || editorState === undefined) {
    return (
      <Center py={100}>
        <Loader />
      </Center>
    );
  }

  const statusColor = sessionError
    ? "red"
    : editorState.active
      ? "teal"
      : "gray";
  const statusText = sessionError
    ? "Контекст недоступен"
    : editorState.active
      ? "Контекст редактора активен"
      : "Нажмите в поле, чтобы активировать контекст";

  return (
    <Stack className="voice-editor-page" gap="sm" pt="md">
      <Stack gap={3}>
        <Title order={2}>Голосовой редактор</Title>
        <Text c="dimmed" size="sm">
          Диктуйте через shortcut post-processing Handy и редактируйте текст
          вручную.
        </Text>
        <Group
          aria-live="polite"
          className="voice-editor-status"
          data-status={statusColor}
          gap={6}
          wrap="nowrap"
        >
          <span aria-hidden="true" className="voice-editor-status-dot" />
          <Text c={statusColor} size="xs">
            {statusText}
          </Text>
        </Group>
      </Stack>

      {sessionError ? (
        <Alert color="red" title="Контекстные правила временно недоступны">
          {sessionError}. Текст редактора продолжает автоматически сохраняться.
        </Alert>
      ) : null}

      <Paper
        withBorder
        radius="md"
        className="voice-editor-shell"
        data-editor-font-size={editorFontSize}
      >
        <Group
          className="voice-editor-toolbar"
          justify="space-between"
          p="xs"
          wrap="nowrap"
        >
          <Group gap={6} wrap="nowrap">
            <Button
              size="xs"
              variant="light"
              disabled={editorState.undoDepth === 0}
              onClick={() => editorRef.current?.undo()}
            >
              Отменить
            </Button>
            <Button
              size="xs"
              variant="light"
              disabled={editorState.redoDepth === 0}
              onClick={() => editorRef.current?.redo()}
            >
              Повторить
            </Button>
            <Button
              size="xs"
              variant="subtle"
              onClick={() => {
                void navigator.clipboard.writeText(
                  editorRef.current?.text() ?? "",
                );
              }}
            >
              Копировать всё
            </Button>
          </Group>
          <Text c={saveState === "error" ? "red" : "dimmed"} size="xs">
            {saveState === "saving"
              ? "Сохранение…"
              : saveState === "error"
                ? "Не удалось сохранить"
                : "Сохранено"}
          </Text>
        </Group>

        <PlainTextEditor
          ref={editorRef}
          historyLimit={historyLimit}
          initialState={draft.state}
          initialValue={draft.content}
          onStateChange={handleStateChange}
          onFocusChange={handleFocusChange}
        />

        <Group
          className="voice-editor-footer"
          justify="space-between"
          p="xs"
          wrap="nowrap"
        >
          <Text c="dimmed" size="xs">
            Ctrl+Z — отменить · Ctrl+Y или Ctrl+Shift+Z — повторить
          </Text>
          <Text c="dimmed" size="xs">
            {countWords(editorState.documentText)} слов ·{" "}
            {editorState.documentText.length} символов
          </Text>
        </Group>
      </Paper>

      <EditorNotesBar
        busy={notesBusy}
        currentNoteId={currentNoteId}
        error={noteError}
        notes={notes}
        onDelete={deleteCurrentNote}
        onRename={renameCurrentNote}
        onSaveAs={saveAs}
        onSelect={(noteId) => void selectNote(noteId)}
      />
    </Stack>
  );
}

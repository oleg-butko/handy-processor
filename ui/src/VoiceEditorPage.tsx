import {
  Alert,
  Badge,
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
import {
  PlainTextEditor,
  type PlainTextEditorHandle,
  type PlainTextEditorState,
} from "./PlainTextEditor.js";
import type { EditorDraft, EditorSessionState } from "./types.js";

const SESSION_HEARTBEAT_MS = 5_000;
const SESSION_SYNC_DELAY_MS = 75;
const DRAFT_SAVE_DELAY_MS = 600;

interface LocalEditorState extends PlainTextEditorState {
  readonly active: boolean;
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

export function VoiceEditorPage() {
  const editorRef = useRef<PlainTextEditorHandle>(null);
  const sessionIdRef = useRef(createSessionId());
  const sessionRevisionRef = useRef(0);
  const sessionRequestIdRef = useRef(0);
  const stateRef = useRef<LocalEditorState | undefined>(undefined);
  const sessionSyncTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const draftSaveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const latestSaveRef = useRef(0);
  const [draft, setDraft] = useState<EditorDraft>();
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
        const response = await fetch("/api/editor/draft", {
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`Сервер вернул HTTP ${response.status}`);
        }
        const loaded = (await response.json()) as EditorDraft;
        setDraft(loaded);
        const initial: LocalEditorState = {
          documentText: loaded.content,
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
    if (state === undefined) {
      return;
    }
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
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }
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

  const saveDraft = useCallback(async (content: string, saveId: number) => {
    try {
      const response = await fetch("/api/editor/draft", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ content }),
      });
      if (!response.ok) {
        throw new Error(`Сервер вернул HTTP ${response.status}`);
      }
      await response.json();
      if (saveId === latestSaveRef.current) {
        // Keep the initial value stable: replacing it would recreate CodeMirror
        // and discard the user's in-memory undo history after every save.
        setSaveState("saved");
      }
    } catch {
      if (saveId === latestSaveRef.current) {
        setSaveState("error");
      }
    }
  }, []);

  const scheduleDraftSave = useCallback(
    (content: string) => {
      if (draftSaveTimerRef.current !== undefined) {
        clearTimeout(draftSaveTimerRef.current);
      }
      setSaveState("saving");
      const saveId = latestSaveRef.current + 1;
      latestSaveRef.current = saveId;
      draftSaveTimerRef.current = setTimeout(() => {
        draftSaveTimerRef.current = undefined;
        void saveDraft(content, saveId);
      }, DRAFT_SAVE_DELAY_MS);
    },
    [saveDraft],
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
      if (previous?.documentText !== next.documentText) {
        scheduleDraftSave(next.documentText);
      }
    },
    [scheduleDraftSave, scheduleSessionSync],
  );

  const handleFocusChange = useCallback(
    (active: boolean) => {
      const previous = stateRef.current;
      if (previous === undefined || previous.active === active) {
        return;
      }
      const state = { ...previous, active };
      stateRef.current = state;
      setEditorState(state);
      void publishSession();
    },
    [publishSession],
  );

  useEffect(() => {
    const interval = setInterval(() => {
      if (stateRef.current?.active) {
        void publishSession();
      }
    }, SESSION_HEARTBEAT_MS);

    return () => {
      clearInterval(interval);
      if (sessionSyncTimerRef.current !== undefined) {
        clearTimeout(sessionSyncTimerRef.current);
      }
      if (draftSaveTimerRef.current !== undefined) {
        clearTimeout(draftSaveTimerRef.current);
        const content = stateRef.current?.documentText;
        if (content !== undefined) {
          void fetch("/api/editor/draft", {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ content }),
            keepalive: true,
          });
        }
      }
      void fetch(
        `/api/editor/sessions/${encodeURIComponent(sessionIdRef.current)}`,
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
      ? "Готов к голосовому вводу"
      : "Нажмите в поле для голосового ввода";

  return (
    <Stack gap="lg" pt="xl">
      <Group justify="space-between" align="flex-end">
        <Stack gap={4}>
          <Title order={2}>Голосовой редактор</Title>
          <Text c="dimmed">
            Диктуйте через shortcut post-processing Handy и редактируйте текст
            вручную.
          </Text>
        </Stack>
        <Badge color={statusColor} variant="dot" size="lg">
          {statusText}
        </Badge>
      </Group>

      {sessionError ? (
        <Alert color="red" title="Контекстные правила временно недоступны">
          {sessionError}. Текст редактора продолжает автоматически сохраняться.
        </Alert>
      ) : null}

      <Paper withBorder radius="lg" shadow="xs" className="voice-editor-shell">
        <Group className="voice-editor-toolbar" justify="space-between" p="sm">
          <Group gap="xs">
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
          initialValue={draft.content}
          onStateChange={handleStateChange}
          onFocusChange={handleFocusChange}
        />

        <Group className="voice-editor-footer" justify="space-between" p="sm">
          <Text c="dimmed" size="xs">
            Ctrl+Z — отменить · Ctrl+Y или Ctrl+Shift+Z — повторить
          </Text>
          <Text c="dimmed" size="xs">
            {countWords(editorState.documentText)} слов ·{" "}
            {editorState.documentText.length} символов
          </Text>
        </Group>
      </Paper>
    </Stack>
  );
}

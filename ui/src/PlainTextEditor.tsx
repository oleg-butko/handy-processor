import {
  defaultKeymap,
  history,
  historyField,
  historyKeymap,
  redo,
  redoDepth,
  undo,
  undoDepth,
} from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { EditorView, drawSelection, keymap } from "@codemirror/view";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
} from "react";

export interface PlainTextEditorState {
  readonly documentText: string;
  readonly serializedState: unknown;
  readonly selectionStart: number;
  readonly selectionEnd: number;
  readonly undoDepth: number;
  readonly redoDepth: number;
}

export interface PlainTextEditorHandle {
  focus(): void;
  undo(): boolean;
  redo(): boolean;
  text(): string;
  snapshot(): EditorDocumentSnapshot;
  replace(snapshot: EditorDocumentSnapshot): void;
}

export interface EditorDocumentSnapshot {
  readonly content: string;
  readonly state: unknown | null;
}

interface PlainTextEditorProps {
  readonly initialValue: string;
  readonly initialState: unknown | null;
  readonly historyLimit: number;
  readonly onStateChange: (state: PlainTextEditorState) => void;
  readonly onFocusChange: (focused: boolean) => void;
}

export const PlainTextEditor = forwardRef<
  PlainTextEditorHandle,
  PlainTextEditorProps
>(function PlainTextEditor(
  { initialValue, initialState, historyLimit, onStateChange, onFocusChange },
  forwardedRef,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const callbacksRef = useRef({ onStateChange, onFocusChange });
  const historyLimitRef = useRef(historyLimit);
  const createStateRef = useRef<
    ((snapshot: EditorDocumentSnapshot) => EditorState) | undefined
  >(undefined);
  callbacksRef.current = { onStateChange, onFocusChange };
  historyLimitRef.current = historyLimit;

  function limitEditorHistory(value: unknown): unknown {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      return value;
    }
    const editor = { ...(value as Record<string, unknown>) };
    const savedHistory = editor.history;
    if (
      typeof savedHistory === "object" &&
      savedHistory !== null &&
      !Array.isArray(savedHistory)
    ) {
      const historyRecord = {
        ...(savedHistory as Record<string, unknown>),
      };
      for (const branch of ["done", "undone"] as const) {
        const events = historyRecord[branch];
        if (Array.isArray(events)) {
          historyRecord[branch] = events.slice(-historyLimitRef.current);
        }
      }
      editor.history = historyRecord;
    }
    return editor;
  }

  function serializeState(state: EditorState): unknown {
    const editor = limitEditorHistory(
      state.toJSON({ history: historyField }),
    );
    return { schemaVersion: 1, editor };
  }

  function describeState(state: EditorState): PlainTextEditorState {
    const selection = state.selection.main;
    return {
      documentText: state.doc.toString(),
      serializedState: serializeState(state),
      selectionStart: selection.from,
      selectionEnd: selection.to,
      undoDepth: undoDepth(state),
      redoDepth: redoDepth(state),
    };
  }

  useImperativeHandle(
    forwardedRef,
    () => ({
      focus: () => viewRef.current?.focus(),
      undo: () => (viewRef.current ? undo(viewRef.current) : false),
      redo: () => (viewRef.current ? redo(viewRef.current) : false),
      text: () => viewRef.current?.state.doc.toString() ?? "",
      snapshot: () => {
        const state = viewRef.current?.state;
        return state === undefined
          ? { content: "", state: null }
          : { content: state.doc.toString(), state: serializeState(state) };
      },
      replace: (snapshot) => {
        const view = viewRef.current;
        const createState = createStateRef.current;
        if (view === null || createState === undefined) {
          return;
        }
        const state = createState(snapshot);
        view.setState(state);
        callbacksRef.current.onStateChange(describeState(state));
        view.focus();
      },
    }),
    [],
  );

  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return;
    }

    const baseExtensions = [
      drawSelection(),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      EditorView.lineWrapping,
      EditorView.contentAttributes.of({
        "aria-label": "Текст голосового редактора",
        spellcheck: "true",
      }),
      EditorView.domEventHandlers({
        focus: () => {
          callbacksRef.current.onFocusChange(true);
          return false;
        },
        blur: () => {
          callbacksRef.current.onFocusChange(false);
          return false;
        },
      }),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged && !update.selectionSet) {
          return;
        }
        callbacksRef.current.onStateChange(describeState(update.state));
      }),
    ];
    const extensions = () => [
      history({ minDepth: historyLimitRef.current }),
      ...baseExtensions,
    ];
    const createState = (snapshot: EditorDocumentSnapshot): EditorState => {
      const saved = snapshot.state;
      if (
        typeof saved === "object" &&
        saved !== null &&
        !Array.isArray(saved) &&
        (saved as Record<string, unknown>).schemaVersion === 1
      ) {
        try {
          const state = EditorState.fromJSON(
            limitEditorHistory(
              (saved as Record<string, unknown>).editor,
            ),
            { extensions: extensions() },
            { history: historyField },
          );
          if (state.doc.toString() === snapshot.content) {
            return state;
          }
        } catch {
          // Fall back to the separately stored text when history is corrupted.
        }
      }
      return EditorState.create({
        doc: snapshot.content,
        extensions: extensions(),
      });
    };
    createStateRef.current = createState;

    const initialEditorState = createState({
      content: initialValue,
      state: initialState,
    });
    const view = new EditorView({
      parent: container,
      state: initialEditorState,
    });
    viewRef.current = view;
    callbacksRef.current.onStateChange(describeState(initialEditorState));

    return () => {
      viewRef.current = null;
      createStateRef.current = undefined;
      view.destroy();
    };
  }, [initialState, initialValue]);

  useEffect(() => {
    const view = viewRef.current;
    const createState = createStateRef.current;
    if (view !== null && createState !== undefined) {
      const state = createState({
        content: view.state.doc.toString(),
        state: serializeState(view.state),
      });
      view.setState(state);
      callbacksRef.current.onStateChange(describeState(state));
    }
  }, [historyLimit]);

  return <div className="voice-editor-field" ref={containerRef} />;
});

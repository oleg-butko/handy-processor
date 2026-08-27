import {
  defaultKeymap,
  history,
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
}

interface PlainTextEditorProps {
  readonly initialValue: string;
  readonly onStateChange: (state: PlainTextEditorState) => void;
  readonly onFocusChange: (focused: boolean) => void;
}

export const PlainTextEditor = forwardRef<
  PlainTextEditorHandle,
  PlainTextEditorProps
>(function PlainTextEditor(
  { initialValue, onStateChange, onFocusChange },
  forwardedRef,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const callbacksRef = useRef({ onStateChange, onFocusChange });
  callbacksRef.current = { onStateChange, onFocusChange };

  useImperativeHandle(
    forwardedRef,
    () => ({
      focus: () => viewRef.current?.focus(),
      undo: () => (viewRef.current ? undo(viewRef.current) : false),
      redo: () => (viewRef.current ? redo(viewRef.current) : false),
      text: () => viewRef.current?.state.doc.toString() ?? "",
    }),
    [],
  );

  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return;
    }

    const view = new EditorView({
      parent: container,
      state: EditorState.create({
        doc: initialValue,
        extensions: [
          history(),
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
            const selection = update.state.selection.main;
            callbacksRef.current.onStateChange({
              documentText: update.state.doc.toString(),
              selectionStart: selection.from,
              selectionEnd: selection.to,
              undoDepth: undoDepth(update.state),
              redoDepth: redoDepth(update.state),
            });
          }),
        ],
      }),
    });
    viewRef.current = view;

    return () => {
      viewRef.current = null;
      view.destroy();
    };
  }, [initialValue]);

  return <div className="voice-editor-field" ref={containerRef} />;
});

export const TRANSFORMATION_SOURCES = {
  external: "external",
  voiceEditor: "voice-editor",
} as const;

export type TransformationSource =
  (typeof TRANSFORMATION_SOURCES)[keyof typeof TRANSFORMATION_SOURCES];

export type SentencePosition = "start" | "middle" | "unknown";

export interface ExternalTransformationContext {
  readonly source: typeof TRANSFORMATION_SOURCES.external;
}

export interface VoiceEditorTransformationContext {
  readonly source: typeof TRANSFORMATION_SOURCES.voiceEditor;
  readonly editor: {
    readonly sessionId: string;
    readonly documentRevision: number;
    readonly selectionStart: number;
    readonly selectionEnd: number;
    readonly sentencePosition: SentencePosition;
  };
}

export type TransformationContext =
  | ExternalTransformationContext
  | VoiceEditorTransformationContext;

export const EXTERNAL_TRANSFORMATION_CONTEXT: ExternalTransformationContext =
  Object.freeze({ source: TRANSFORMATION_SOURCES.external });

const SENTENCE_TERMINATORS = /[.!?…]$/u;
const TRAILING_CLOSERS = /[\s\u00a0"'»”’）)\]}]+$/gu;
const ONLY_LEADING_DECORATION = /^[\s\u00a0"'«„“‘（([{—–-]*$/u;

/**
 * Derive only the stable fact rules need. The document itself is deliberately
 * kept out of persisted transformation context snapshots.
 */
export function detectSentencePosition(
  documentText: string,
  selectionStart: number,
): SentencePosition {
  if (
    !Number.isSafeInteger(selectionStart) ||
    selectionStart < 0 ||
    selectionStart > documentText.length
  ) {
    return "unknown";
  }

  const prefix = documentText.slice(0, selectionStart);
  if (ONLY_LEADING_DECORATION.test(prefix)) {
    return "start";
  }

  const beforeClosingPunctuation = prefix.replace(TRAILING_CLOSERS, "");
  return SENTENCE_TERMINATORS.test(beforeClosingPunctuation)
    ? "start"
    : "middle";
}

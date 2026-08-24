import {
  EXTERNAL_TRANSFORMATION_CONTEXT,
  TRANSFORMATION_SOURCES,
  detectSentencePosition,
  type TransformationContext,
} from "./transformation-context.js";

export const DEFAULT_EDITOR_CONTEXT_TTL_MS = 15_000;

export interface EditorSessionUpdate {
  readonly documentText: string;
  readonly documentRevision: number;
  readonly selectionStart: number;
  readonly selectionEnd: number;
  readonly active: boolean;
}

export interface EditorSessionState {
  readonly sessionId: string;
  readonly documentRevision: number;
  readonly selectionStart: number;
  readonly selectionEnd: number;
  readonly active: boolean;
  readonly expiresAt: number;
  readonly sentencePosition: "start" | "middle" | "unknown";
}

interface MutableEditorSession extends EditorSessionUpdate {
  readonly sessionId: string;
  updatedAt: number;
  active: boolean;
}

export type EditorSessionUpdateResult =
  | { readonly ok: true; readonly state: EditorSessionState }
  | { readonly ok: false; readonly reason: "stale-revision" };

export class EditorSessionRegistry {
  readonly #sessions = new Map<string, MutableEditorSession>();
  readonly #ttlMs: number;
  readonly #now: () => number;

  constructor(
    ttlMs = DEFAULT_EDITOR_CONTEXT_TTL_MS,
    now: () => number = Date.now,
  ) {
    if (!Number.isSafeInteger(ttlMs) || ttlMs < 1) {
      throw new Error("Editor session TTL must be a positive integer");
    }
    this.#ttlMs = ttlMs;
    this.#now = now;
  }

  update(
    sessionId: string,
    update: EditorSessionUpdate,
  ): EditorSessionUpdateResult {
    const now = this.#now();
    this.#removeExpired(now);

    const existing = this.#sessions.get(sessionId);
    if (
      existing !== undefined &&
      (update.documentRevision < existing.documentRevision ||
        (update.documentRevision === existing.documentRevision &&
          (update.documentText !== existing.documentText ||
            update.selectionStart !== existing.selectionStart ||
            update.selectionEnd !== existing.selectionEnd)))
    ) {
      return { ok: false, reason: "stale-revision" };
    }

    if (update.active) {
      for (const session of this.#sessions.values()) {
        session.active = false;
      }
    }

    const session: MutableEditorSession = {
      sessionId,
      ...update,
      updatedAt: now,
    };
    this.#sessions.set(sessionId, session);
    return { ok: true, state: this.#toState(session) };
  }

  remove(sessionId: string): void {
    this.#sessions.delete(sessionId);
  }

  currentContext(): TransformationContext {
    const now = this.#now();
    this.#removeExpired(now);
    const activeSessions = [...this.#sessions.values()].filter(
      (session) => session.active,
    );
    if (activeSessions.length !== 1) {
      return EXTERNAL_TRANSFORMATION_CONTEXT;
    }

    const session = activeSessions[0];
    if (session === undefined) {
      return EXTERNAL_TRANSFORMATION_CONTEXT;
    }

    return {
      source: TRANSFORMATION_SOURCES.voiceEditor,
      editor: {
        sessionId: session.sessionId,
        documentRevision: session.documentRevision,
        selectionStart: session.selectionStart,
        selectionEnd: session.selectionEnd,
        sentencePosition: detectSentencePosition(
          session.documentText,
          session.selectionStart,
        ),
      },
    };
  }

  #removeExpired(now: number): void {
    for (const [sessionId, session] of this.#sessions) {
      if (now - session.updatedAt >= this.#ttlMs) {
        this.#sessions.delete(sessionId);
      }
    }
  }

  #toState(session: MutableEditorSession): EditorSessionState {
    return {
      sessionId: session.sessionId,
      documentRevision: session.documentRevision,
      selectionStart: session.selectionStart,
      selectionEnd: session.selectionEnd,
      active: session.active,
      expiresAt: session.updatedAt + this.#ttlMs,
      sentencePosition: detectSentencePosition(
        session.documentText,
        session.selectionStart,
      ),
    };
  }
}

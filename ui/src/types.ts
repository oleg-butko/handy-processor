export interface AppliedRuleRecord {
  readonly id: string;
  readonly version: number;
  readonly changed: boolean;
  readonly outcome: "changed" | "unchanged" | "skipped";
  readonly reason?: string;
}

export interface TransformationRecord {
  readonly id: number;
  readonly originalText: string;
  readonly transformedText: string;
  readonly createdAt: string;
  readonly appliedRules: readonly AppliedRuleRecord[] | null;
  readonly source: "external" | "voice-editor";
  readonly context: {
    readonly schemaVersion: 1;
    readonly editor: {
      readonly sentencePosition: "start" | "middle" | "unknown";
    };
  } | null;
}

export interface HistoryResponse {
  readonly items: readonly TransformationRecord[];
  readonly pagination: {
    readonly page: number;
    readonly pageSize: number;
    readonly total: number;
    readonly totalPages: number;
  };
}

export interface TransformationRuleState {
  readonly id: string;
  readonly version: number;
  readonly title: string;
  readonly description: string;
  readonly defaultEnabled: boolean;
  readonly enabled: boolean;
  readonly category: "common" | "voice-editor";
  readonly sources: readonly ("external" | "voice-editor")[];
  readonly example: {
    readonly input: string;
    readonly output: string;
  };
}

export interface RulesResponse {
  readonly items: readonly TransformationRuleState[];
}

export interface EditorDraft {
  readonly content: string;
  readonly state: unknown | null;
  readonly currentNoteId: number | null;
  readonly revision: number;
  readonly updatedAt: string | null;
}

export interface EditorNoteSummary {
  readonly id: number;
  readonly name: string;
  readonly updatedAt: string;
}

export interface EditorNote extends EditorNoteSummary {
  readonly content: string;
  readonly state: unknown | null;
  readonly revision: number;
  readonly createdAt: string;
}

export interface EditorNotesResponse {
  readonly items: readonly EditorNoteSummary[];
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

export type FontSizePreference = "small" | "medium" | "large";
export type EditorFontSize = FontSizePreference;
export type UiFontSize = FontSizePreference;

export interface CorrectionCandidate {
  readonly canonical: string;
  readonly alias: string;
  readonly set: string;
  readonly term: string;
  readonly score: number;
  readonly distance: number;
  readonly eligible: boolean;
  readonly gate: string;
  readonly parts: Readonly<Record<string, number>>;
  readonly context_hits: readonly string[];
  readonly context_pattern_hits?: readonly string[];
  readonly context_exclusion_hits?: readonly string[];
  readonly alias_origin?: string;
  readonly source_penalty_hits?: readonly string[];
}

export interface CorrectionDecision {
  readonly original: string;
  readonly selected: string | null;
  readonly reason: string;
  readonly runner_up_margin: number | null;
  readonly byte_start: number;
  readonly byte_end: number;
  readonly candidates: readonly CorrectionCandidate[];
}

export interface CorrectionTrace {
  readonly schemaVersion: 1;
  readonly profile: string;
  readonly complete: boolean;
  readonly minScore: number;
  readonly minMargin: number;
  readonly decisions: readonly CorrectionDecision[];
  readonly omittedDecisions: number;
  readonly omittedCandidates: number;
}

export interface CorrectionOutput {
  readonly text: string;
  readonly trace: CorrectionTrace;
}

export interface TranscriptionCorrector {
  analyze(text: string): Promise<CorrectionOutput>;
}

/** Validate the bounded wire snapshot before it is persisted or rendered. */
export function isCorrectionOutput(value: unknown, profile: string): value is CorrectionOutput {
  const object = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
  const finite = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
  const text = (v: unknown): v is string => typeof v === "string" && v.length <= 32768;
  const strings = (v: unknown) => Array.isArray(v) && v.length <= 128 && v.every(text);
  const count = (v: unknown) => Number.isSafeInteger(v) && (v as number) >= 0;
  if (!object(value) || !text(value.text) || !object(value.trace)) return false;
  const t = value.trace;
  if (t.schemaVersion !== 1 || t.profile !== profile || typeof t.complete !== "boolean" ||
      !finite(t.minScore) || !finite(t.minMargin) || !count(t.omittedDecisions) || !count(t.omittedCandidates) ||
      !Array.isArray(t.decisions) || t.decisions.length > 32) return false;
  return t.decisions.every(d => object(d) && text(d.original) && (d.selected === null || text(d.selected)) &&
    text(d.reason) && (d.runner_up_margin === null || finite(d.runner_up_margin)) &&
    count(d.byte_start) && count(d.byte_end) && (d.byte_end as number) >= (d.byte_start as number) &&
    Array.isArray(d.candidates) && d.candidates.length <= 6 && d.candidates.every(c =>
      object(c) && text(c.canonical) && text(c.alias) && text(c.set) && text(c.term) && text(c.gate) &&
      typeof c.eligible === "boolean" && finite(c.score) && count(c.distance) &&
      object(c.parts) && Object.keys(c.parts).length <= 16 && Object.values(c.parts).every(finite) &&
      Math.abs(Object.values(c.parts).reduce<number>((sum, part) => sum + (part as number), 0) - c.score) < 1e-8 &&
      strings(c.context_hits) && [c.context_pattern_hits, c.context_exclusion_hits, c.source_penalty_hits].every(v => v === undefined || strings(v)) &&
      (c.alias_origin === undefined || text(c.alias_origin))));
}

import {
  applyTransformationRules,
  type TransformationResult,
  type TransformationRuleId,
  DEFAULT_ENABLED_RULE_IDS,
  TRANSFORMATION_RULE_IDS,
} from "./transformation-rules.js";
import type { TranscriptionCorrector } from "./correction-types.js";
import type { TransformationContext } from "./transformation-context.js";

/**
 * Apply enabled local transcription rules in registry order.
 */
export function transformTranscription(
  text: string,
  enabledRuleIds?: ReadonlySet<TransformationRuleId>,
  context?: TransformationContext,
): TransformationResult {
  return applyTransformationRules(text, enabledRuleIds, context);
}

/** Correction sees the original text; formatting never removes its protections. */
export async function transformTranscriptionAsync(
  text: string,
  enabledRuleIds: ReadonlySet<TransformationRuleId> = DEFAULT_ENABLED_RULE_IDS,
  context?: TransformationContext,
  corrector?: TranscriptionCorrector,
): Promise<TransformationResult> {
  const id = TRANSFORMATION_RULE_IDS.correction;
  if (!enabledRuleIds.has(id) || !corrector) return transformTranscription(text, enabledRuleIds, context);
  const remaining = new Set(enabledRuleIds);
  remaining.delete(id);
  let corrected = text;
  let applied: TransformationResult["appliedRules"][number];
  try {
    const output = await corrector.analyze(text);
    if (output.trace.complete) corrected = output.text;
    const changed = corrected !== text;
    applied = { id, version: 1, outcome: output.trace.complete ? changed ? "changed" : "unchanged" : "skipped",
      changed, correction: output.trace,
      ...(!output.trace.complete ? { reason: "correction-incomplete" } : {}) };
  } catch (error) {
    applied = { id, version: 1, outcome: "skipped", changed: false,
      reason: "correction-unavailable", error: error instanceof Error ? error.message : String(error) };
  }
  const result = transformTranscription(corrected, remaining, context);
  return { text: result.text, appliedRules: [applied, ...result.appliedRules] };
}

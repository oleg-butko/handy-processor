import {
  applyTransformationRules,
  type TransformationResult,
  type TransformationRuleId,
} from "./transformation-rules.js";

/**
 * Apply enabled local transcription rules in registry order.
 */
export function transformTranscription(
  text: string,
  enabledRuleIds?: ReadonlySet<TransformationRuleId>,
): TransformationResult {
  return applyTransformationRules(text, enabledRuleIds);
}

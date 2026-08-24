import {
  applyTransformationRules,
  type TransformationResult,
  type TransformationRuleId,
} from "./transformation-rules.js";
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

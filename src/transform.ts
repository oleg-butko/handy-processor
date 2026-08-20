import { normalizeRussianNumerals } from "./russian-numerals.js";

/**
 * Apply the local transcription transformations.
 *
 * Only a period at the end of the complete input is removed. Periods between
 * sentences and other final punctuation are preserved.
 */
export function transformTranscription(text: string): string {
  const lowercased = text.toLowerCase();
  return normalizeRussianNumerals(lowercased).replace(/\.(\s*)$/u, "$1");
}

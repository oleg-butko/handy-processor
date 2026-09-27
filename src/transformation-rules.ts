import { normalizeRussianNumerals } from "./russian-numerals.js";
import type { CorrectionTrace } from "./correction-types.js";
import {
  EXTERNAL_TRANSFORMATION_CONTEXT,
  TRANSFORMATION_SOURCES,
  type TransformationContext,
  type TransformationSource,
} from "./transformation-context.js";

export const TRANSFORMATION_RULE_IDS = {
  correction: "recognition.terms-and-phrases",
  lowercase: "case.lowercase",
  editorLowercaseInsideSentence: "editor.case.lowercase-inside-sentence",
  russianNumerals: "numbers.ru-cardinals-to-digits",
  removeFinalPeriod: "punctuation.remove-final-period",
  removeAllCommas: "punctuation.remove-all-commas",
} as const;

export type TransformationRuleId =
  (typeof TRANSFORMATION_RULE_IDS)[keyof typeof TRANSFORMATION_RULE_IDS];

export interface TransformationRuleExample {
  readonly input: string;
  readonly output: string;
}

export interface TransformationRule {
  readonly id: TransformationRuleId;
  readonly version: number;
  readonly title: string;
  readonly description: string;
  readonly defaultEnabled: boolean;
  readonly category: "common" | "voice-editor";
  readonly sources: readonly TransformationSource[];
  readonly example: TransformationRuleExample;
  readonly when?: (context: TransformationContext) =>
    | { readonly apply: true }
    | { readonly apply: false; readonly reason: string };
  apply(text: string, context: TransformationContext): string;
}

export type TransformationRuleOutcome = "changed" | "unchanged" | "skipped";

export interface AppliedTransformationRule {
  readonly id: TransformationRuleId;
  readonly version: number;
  readonly outcome: TransformationRuleOutcome;
  /** Kept in the public response while older UI versions still consume it. */
  readonly changed: boolean;
  readonly reason?: string;
  readonly error?: string;
  readonly correction?: CorrectionTrace;
}

export interface TransformationResult {
  readonly text: string;
  readonly appliedRules: readonly AppliedTransformationRule[];
}

/**
 * Registry order is the pipeline order. Rule ids are persisted, so they must
 * remain stable even if a user-facing title changes.
 */
export const TRANSFORMATION_RULES: readonly TransformationRule[] = [
  {
    id: TRANSFORMATION_RULE_IDS.correction,
    version: 1,
    title: "Коррекция терминов и фраз",
    description: "Исправляет варианты терминов, словоформы, произношения аббревиатур и слитные фразы. Учитывает контекст и веса; при конфликте сохраняет исходник. Выполняется до изменения регистра и чисел.",
    defaultEnabled: false,
    category: "common",
    sources: [TRANSFORMATION_SOURCES.external, TRANSFORMATION_SOURCES.voiceEditor],
    example: { input: "без снятия сваты", output: "без снятия слота" },
    // Executed by the asynchronous adapter; a synchronous call must report a skip.
    when: () => ({ apply: false, reason: "correction-unavailable" }),
    apply: (text) => text,
  },
  {
    id: TRANSFORMATION_RULE_IDS.lowercase,
    version: 1,
    title: "Нижний регистр для внешнего ввода",
    description:
      "Переводит все буквы текста в нижний регистр при вставке вне голосового редактора.",
    defaultEnabled: true,
    category: "common",
    sources: [TRANSFORMATION_SOURCES.external],
    example: {
      input: "Привет, МИР.",
      output: "привет, мир.",
    },
    apply: (text) => text.toLowerCase(),
  },
  {
    id: TRANSFORMATION_RULE_IDS.editorLowercaseInsideSentence,
    version: 1,
    title: "Нижний регистр внутри предложения",
    description:
      "В голосовом редакторе переводит вставляемый текст в нижний регистр только в середине предложения.",
    defaultEnabled: true,
    category: "voice-editor",
    sources: [TRANSFORMATION_SOURCES.voiceEditor],
    example: {
      input: "НОВАЯ ВСТАВКА",
      output: "новая вставка",
    },
    when: (context) => {
      if (context.source !== TRANSFORMATION_SOURCES.voiceEditor) {
        return { apply: false, reason: "source-not-supported" };
      }
      if (context.editor.sentencePosition === "start") {
        return { apply: false, reason: "sentence-start" };
      }
      if (context.editor.sentencePosition === "unknown") {
        return { apply: false, reason: "context-unknown" };
      }
      return { apply: true };
    },
    apply: (text) => text.toLowerCase(),
  },
  {
    id: TRANSFORMATION_RULE_IDS.russianNumerals,
    version: 1,
    title: "Русские числительные → цифры",
    description:
      "Заменяет русские количественные числительные цифрами, учитывая поддерживаемые падежи и небольшие ошибки распознавания.",
    defaultEnabled: true,
    category: "common",
    sources: [
      TRANSFORMATION_SOURCES.external,
      TRANSFORMATION_SOURCES.voiceEditor,
    ],
    example: {
      input: "Мне нужно двадцать два файла",
      output: "Мне нужно 22 файла",
    },
    apply: normalizeRussianNumerals,
  },
  {
    id: TRANSFORMATION_RULE_IDS.removeFinalPeriod,
    version: 1,
    title: "Удаление конечной точки",
    description:
      "Удаляет точку только в конце текста, сохраняя остальные знаки препинания.",
    defaultEnabled: true,
    category: "common",
    sources: [
      TRANSFORMATION_SOURCES.external,
      TRANSFORMATION_SOURCES.voiceEditor,
    ],
    example: {
      input: "Текст готов.",
      output: "Текст готов",
    },
    apply: (text) => text.replace(/\.(\s*)$/u, "$1"),
  },
  {
    id: TRANSFORMATION_RULE_IDS.removeAllCommas,
    version: 1,
    title: "Удаление всех запятых",
    description: "Удаляет все запятые в тексте, сохраняя остальные знаки препинания.",
    defaultEnabled: false,
    category: "common",
    sources: [TRANSFORMATION_SOURCES.external, TRANSFORMATION_SOURCES.voiceEditor],
    example: {
      input: "Первый, второй, третий.",
      output: "Первый второй третий.",
    },
    apply: (text) => text.replace(/,/gu, ""),
  },
];

export const DEFAULT_ENABLED_RULE_IDS: ReadonlySet<TransformationRuleId> =
  new Set(
    TRANSFORMATION_RULES.filter((rule) => rule.defaultEnabled).map(
      (rule) => rule.id,
    ),
  );

export function applyTransformationRules(
  text: string,
  enabledRuleIds: ReadonlySet<TransformationRuleId> =
    DEFAULT_ENABLED_RULE_IDS,
  context: TransformationContext = EXTERNAL_TRANSFORMATION_CONTEXT,
): TransformationResult {
  let transformedText = text;
  const appliedRules: AppliedTransformationRule[] = [];

  for (const rule of TRANSFORMATION_RULES) {
    if (!enabledRuleIds.has(rule.id)) {
      continue;
    }

    if (!rule.sources.includes(context.source)) {
      continue;
    }

    const condition = rule.when?.(context);
    if (condition?.apply === false) {
      appliedRules.push({
        id: rule.id,
        version: rule.version,
        outcome: "skipped",
        changed: false,
        reason: condition.reason,
      });
      continue;
    }

    const previousText = transformedText;
    transformedText = rule.apply(previousText, context);
    const changed = transformedText !== previousText;
    appliedRules.push({
      id: rule.id,
      version: rule.version,
      outcome: changed ? "changed" : "unchanged",
      changed,
    });
  }

  return { text: transformedText, appliedRules };
}

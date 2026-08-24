import { normalizeRussianNumerals } from "./russian-numerals.js";

export const TRANSFORMATION_RULE_IDS = {
  lowercase: "case.lowercase",
  russianNumerals: "numbers.ru-cardinals-to-digits",
  removeFinalPeriod: "punctuation.remove-final-period",
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
  readonly example: TransformationRuleExample;
  apply(text: string): string;
}

export interface AppliedTransformationRule {
  readonly id: TransformationRuleId;
  readonly version: number;
  readonly changed: boolean;
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
    id: TRANSFORMATION_RULE_IDS.lowercase,
    version: 1,
    title: "Нижний регистр",
    description: "Переводит все буквы текста в нижний регистр.",
    defaultEnabled: true,
    example: {
      input: "Привет, МИР.",
      output: "привет, мир.",
    },
    apply: (text) => text.toLowerCase(),
  },
  {
    id: TRANSFORMATION_RULE_IDS.russianNumerals,
    version: 1,
    title: "Русские числительные",
    description:
      "Заменяет поддерживаемые русские количественные числительные цифрами.",
    defaultEnabled: true,
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
    example: {
      input: "Текст готов.",
      output: "Текст готов",
    },
    apply: (text) => text.replace(/\.(\s*)$/u, "$1"),
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
): TransformationResult {
  let transformedText = text;
  const appliedRules: AppliedTransformationRule[] = [];

  for (const rule of TRANSFORMATION_RULES) {
    if (!enabledRuleIds.has(rule.id)) {
      continue;
    }

    const previousText = transformedText;
    transformedText = rule.apply(previousText);
    appliedRules.push({
      id: rule.id,
      version: rule.version,
      changed: transformedText !== previousText,
    });
  }

  return { text: transformedText, appliedRules };
}

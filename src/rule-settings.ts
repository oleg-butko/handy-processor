import {
  TRANSFORMATION_RULES,
  type TransformationRule,
  type TransformationRuleExample,
  type TransformationRuleId,
} from "./transformation-rules.js";

export interface StoredRuleSetting {
  readonly ruleId: string;
  readonly enabled: boolean;
}

export interface RuleSettingsStorage {
  listRuleSettings(): readonly StoredRuleSetting[];
  setRuleEnabled(ruleId: string, enabled: boolean): void;
}

export interface TransformationRuleState {
  readonly id: TransformationRuleId;
  readonly version: number;
  readonly title: string;
  readonly description: string;
  readonly defaultEnabled: boolean;
  readonly enabled: boolean;
  readonly example: TransformationRuleExample;
}

function toState(
  rule: TransformationRule,
  overrides: ReadonlyMap<string, boolean>,
): TransformationRuleState {
  return {
    id: rule.id,
    version: rule.version,
    title: rule.title,
    description: rule.description,
    defaultEnabled: rule.defaultEnabled,
    enabled: overrides.get(rule.id) ?? rule.defaultEnabled,
    example: rule.example,
  };
}

export class RuleSettings {
  readonly #storage: RuleSettingsStorage;
  readonly #overrides: Map<string, boolean>;
  readonly #rulesById = new Map(
    TRANSFORMATION_RULES.map((rule) => [rule.id, rule]),
  );

  constructor(storage: RuleSettingsStorage) {
    this.#storage = storage;
    this.#overrides = new Map(
      storage
        .listRuleSettings()
        .map((setting) => [setting.ruleId, setting.enabled]),
    );
  }

  list(): readonly TransformationRuleState[] {
    return TRANSFORMATION_RULES.map((rule) =>
      toState(rule, this.#overrides),
    );
  }

  enabledRuleIds(): ReadonlySet<TransformationRuleId> {
    return new Set(
      this.list()
        .filter((rule) => rule.enabled)
        .map((rule) => rule.id),
    );
  }

  setEnabled(
    ruleId: string,
    enabled: boolean,
  ): TransformationRuleState | undefined {
    const rule = this.#rulesById.get(ruleId as TransformationRuleId);
    if (rule === undefined) {
      return undefined;
    }

    this.#storage.setRuleEnabled(rule.id, enabled);
    this.#overrides.set(rule.id, enabled);
    return toState(rule, this.#overrides);
  }
}

export interface AppliedRuleRecord {
  readonly id: string;
  readonly version: number;
  readonly changed: boolean;
}

export interface TransformationRecord {
  readonly id: number;
  readonly originalText: string;
  readonly transformedText: string;
  readonly createdAt: string;
  readonly appliedRules: readonly AppliedRuleRecord[] | null;
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
  readonly example: {
    readonly input: string;
    readonly output: string;
  };
}

export interface RulesResponse {
  readonly items: readonly TransformationRuleState[];
}

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { RuleSettings } from "./rule-settings.js";
import { TRANSFORMATION_RULE_IDS } from "./transformation-rules.js";
import { TransformationLog } from "./transformation-log.js";

describe("RuleSettings", () => {
  it("uses registry defaults and persists overrides", () => {
    const storage = new TransformationLog(":memory:");
    try {
      const settings = new RuleSettings(storage);
      assert.equal(settings.list().every((rule) => rule.enabled === rule.defaultEnabled), true);
      assert.equal(settings.enabledRuleIds().has(TRANSFORMATION_RULE_IDS.correction), false);

      const updated = settings.setEnabled(
        TRANSFORMATION_RULE_IDS.lowercase,
        false,
      );
      assert.equal(updated?.enabled, false);
      assert.equal(
        settings.enabledRuleIds().has(TRANSFORMATION_RULE_IDS.lowercase),
        false,
      );

      const reloaded = new RuleSettings(storage);
      assert.equal(
        reloaded
          .list()
          .find((rule) => rule.id === TRANSFORMATION_RULE_IDS.lowercase)
          ?.enabled,
        false,
      );
      assert.equal(reloaded.setEnabled("unknown.rule", true), undefined);
    } finally {
      storage.close();
    }
  });
});

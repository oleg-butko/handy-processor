// Fresh, explicitly accepted synthetic reviews for the current compiled producer.
// Historical exports are immutable and intentionally rejected after runtime changes.
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { pack } from "../store.mjs";

export function currentFixtures(root) {
  const directory = mkdtempSync(join(tmpdir(), "rechprav-publication-fixture-"));
  const old = join(root, "experiments/017-profile-composition-v3");
  const empty = join(directory, "empty.jsonl"); writeFileSync(empty, "");
  const result = {};
  try {
    for (const name of ["composed", "global-off"]) {
      const compose = (decisions, output) => {
        const process = spawnSync(join(root, "target/release/examples/feedback_compose"), [
          join(old, "inputs", `${name}.jsonl`), "--config", join(old, "inputs", `${name}.toml`),
          "--bindings", join(old, "inputs", `${name}-bindings.json`), "--decisions", decisions,
          "--scope", "technical", "--output-dir", output], { encoding: "utf8", timeout: 60_000 });
        if (process.status !== 0) throw new Error(`fixture composition failed: ${process.stderr}`);
      };
      const review = join(directory, `${name}-review`); compose(empty, review);
      const tokens = JSON.parse(readFileSync(join(review, "report.json"))).registry.review_tokens;
      const decisions = join(directory, `${name}-decisions.jsonl`);
      writeFileSync(decisions, tokens.map((review, i) => JSON.stringify({ schema_version: 1,
        decision_id: `accept-${i}`, action: "accept", review })).join("\n") + "\n");
      const output = join(directory, name); compose(decisions, output);
      result[name === "composed" ? "composed" : "disabled"] = pack(output, join(old, "inputs", `${name}.toml`));
    }
    return { directory, ...result };
  } catch (error) { rmSync(directory, { recursive: true, force: true }); throw error; }
}

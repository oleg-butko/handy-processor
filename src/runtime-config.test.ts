import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import {
  CONFIG_FILE_NAME,
  loadRuntimeConfig,
} from "./runtime-config.js";

describe("runtime config", () => {
  it("creates a default config and reads a custom port", () => {
    const directory = mkdtempSync(join(tmpdir(), "handy-processor-config-"));
    const configPath = join(directory, CONFIG_FILE_NAME);

    try {
      assert.deepEqual(loadRuntimeConfig(directory), { port: 11434 });
      assert.deepEqual(JSON.parse(readFileSync(configPath, "utf8")), {
        port: 11434,
      });

      writeFileSync(configPath, '{"port": 12434}\n', "utf8");
      assert.deepEqual(loadRuntimeConfig(directory), { port: 12434 });
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("rejects an invalid port", () => {
    const directory = mkdtempSync(join(tmpdir(), "handy-processor-config-"));
    const configPath = join(directory, CONFIG_FILE_NAME);

    try {
      writeFileSync(configPath, '{"port": 70000}\n', "utf8");
      assert.throws(() => loadRuntimeConfig(directory), /port must be/u);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});

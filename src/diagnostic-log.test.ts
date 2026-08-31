import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import {
  DIAGNOSTIC_LOG_FILE_NAME,
  FileDiagnosticLog,
} from "./diagnostic-log.js";

describe("FileDiagnosticLog", () => {
  it("serializes concurrent diagnostic writes as JSON lines", async () => {
    const directory = mkdtempSync(join(tmpdir(), "handy-processor-log-"));
    try {
      const log = new FileDiagnosticLog(directory);
      const entry = {
        event: "history-export" as const,
        message: "invoke failed",
        origin: "http://127.0.0.1:11434",
        userAgent: "test-webview",
        launcherBridgeDetected: true,
        tauriNamespaces: ["core"],
      };

      await Promise.all([
        log.appendClientError(entry),
        log.appendClientError({ ...entry, message: "second failure" }),
      ]);

      assert.equal(log.path, join(directory, DIAGNOSTIC_LOG_FILE_NAME));
      const records = readFileSync(log.path, "utf8")
        .trimEnd()
        .split("\n")
        .map((line) => JSON.parse(line) as Record<string, unknown>);
      assert.equal(records.length, 2);
      assert.equal(records[0]?.message, "invoke failed");
      assert.equal(records[1]?.message, "second failure");
      assert.equal(records[0]?.source, "web-ui");
      assert.equal(records[0]?.level, "error");
      assert.match(String(records[0]?.timestamp), /^\d{4}-\d{2}-\d{2}T/u);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});

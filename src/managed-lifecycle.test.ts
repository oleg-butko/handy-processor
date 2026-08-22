import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { describe, it } from "node:test";
import {
  isManagedProcess,
  monitorManagedInput,
} from "./managed-lifecycle.js";

describe("managed server lifecycle", () => {
  it("recognizes only the explicit managed flag", () => {
    assert.equal(isManagedProcess([]), false);
    assert.equal(isManagedProcess(["--managed"]), true);
    assert.equal(isManagedProcess(["--not-managed"]), false);
  });

  it("requests shutdown once when commanded", () => {
    const input = new PassThrough();
    const reasons: string[] = [];
    const monitor = monitorManagedInput(input, (reason) => reasons.push(reason));

    input.write("unknown\nshut");
    input.write("down\nshutdown\n");

    assert.deepEqual(reasons, ["launcher requested shutdown"]);
    monitor.stop();
    input.destroy();
  });

  it("requests shutdown when the launcher closes stdin", async () => {
    const input = new PassThrough();
    const reason = new Promise<string>((resolve) => {
      monitorManagedInput(input, resolve);
    });

    input.end();

    assert.equal(await reason, "launcher disconnected");
  });
});

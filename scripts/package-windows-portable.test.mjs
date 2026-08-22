import assert from "node:assert/strict";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { packageWindowsPortable } from "./package-windows-portable.mjs";

function withTemporaryDirectory(callback) {
  const directory = mkdtempSync(join(tmpdir(), "handy-processor-package-"));
  try {
    callback(directory);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test("creates a portable directory with exactly the required build artifacts", () => {
  withTemporaryDirectory((directory) => {
    const launcher = join(directory, "launcher.exe");
    const server = join(directory, "server.exe");
    const config = join(directory, "source-config.json");
    const destination = join(directory, "release");
    writeFileSync(launcher, Buffer.from("MZlauncher"));
    writeFileSync(server, Buffer.from("MZserver"));
    writeFileSync(config, '{"port":11434}\n');

    const files = packageWindowsPortable({
      launcher,
      server,
      config,
      destination,
    });

    assert.deepEqual(
      files.map((path) => path.slice(destination.length + 1)),
      ["HandyProcessor.exe", "handy-processor-server.exe", "config.json"],
    );
    assert.equal(
      readFileSync(join(destination, "handy-processor-server.exe"), "utf8"),
      "MZserver",
    );
  });
});

test("rejects a non-Windows server artifact", () => {
  withTemporaryDirectory((directory) => {
    const launcher = join(directory, "launcher.exe");
    const server = join(directory, "server.exe");
    const config = join(directory, "config.json");
    writeFileSync(launcher, Buffer.from("MZlauncher"));
    writeFileSync(server, Buffer.from("ELF server"));
    writeFileSync(config, '{"port":11434}\n');

    assert.throws(
      () =>
        packageWindowsPortable({
          launcher,
          server,
          config,
          destination: join(directory, "release"),
        }),
      /not a Windows PE executable/u,
    );
  });
});

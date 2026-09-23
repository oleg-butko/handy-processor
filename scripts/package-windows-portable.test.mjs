import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  existsSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import { packageWindowsPortable } from "./package-windows-portable.mjs";
import { correctionRuntimeFiles, copyCorrectionRuntime } from "./correction-runtime.mjs";

function correctionFixture(directory) {
  const files = {
    "rechprav/target/release/examples/correction_worker.exe": "MZworker",
    "rechprav/handy-terms.toml": 'baseline_config="handy-dictionary.toml"',
    "rechprav/handy-dictionary.toml": 'dictionary="artifacts/opencorpora-wordfreq/dictionary.rpt"',
    "rechprav/artifacts/opencorpora-wordfreq/dictionary.rpt": "test dictionary bytes",
    "rechprav/DICTIONARY-NOTICE.md": "Dictionary attribution",
  };
  for (const [relative, content] of Object.entries(files)) {
    const path = join(directory, relative);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, content);
  }
  return files;
}

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
    const templatesExample = join(directory, "templates.example.txt");
    writeFileSync(launcher, Buffer.from("MZlauncher"));
    writeFileSync(server, Buffer.from("MZserver"));
    writeFileSync(config, '{"port":11434}\n');
    writeFileSync(templatesExample, "Категория > Тема > Пример\n");
    const correction = correctionFixture(directory);

    const files = packageWindowsPortable({
      launcher,
      server,
      config,
      destination,
      templatesExample,
      correctionRoot: directory,
    });

    assert.deepEqual(
      files.map((path) => path.slice(destination.length + 1)),
      ["HandyProcessor.exe", "handy-processor-server.exe", "config.json", "templates.example.txt",
        ...Object.keys(correction).map(relative => join(...relative.split("/")))],
    );
    assert.equal(
      readFileSync(join(destination, "handy-processor-server.exe"), "utf8"),
      "MZserver",
    );
    writeFileSync(join(destination, "templates.txt"), "Личные > Шаблоны > Мой текст\n");
    writeFileSync(join(destination, "template-hotkey.json"), '{"enabled":false}');
    for (const [relative, content] of Object.entries(correction)) {
      assert.equal(readFileSync(join(destination, relative), "utf8"), content);
    }
    writeFileSync(join(destination, "rechprav/handy-terms.toml"), "# user terms");
    writeFileSync(join(destination, "rechprav/handy-dictionary.toml"), "# user dictionary");
    packageWindowsPortable({ launcher, server, config, destination, templatesExample, correctionRoot: directory });
    assert.equal(readFileSync(join(destination, "templates.txt"), "utf8"), "Личные > Шаблоны > Мой текст\n");
    assert.equal(readFileSync(join(destination, "template-hotkey.json"), "utf8"), '{"enabled":false}');
    assert.equal(readFileSync(join(destination, "rechprav/handy-terms.toml"), "utf8"), "# user terms");
    assert.equal(readFileSync(join(destination, "rechprav/handy-dictionary.toml"), "utf8"), "# user dictionary");
  });
});

test("missing correction resources and a non-Windows worker fail before changing the release", () => {
  withTemporaryDirectory(directory => {
    const launcher = join(directory, "launcher.exe");
    const server = join(directory, "server.exe");
    const config = join(directory, "config.json");
    const destination = join(directory, "release");
    writeFileSync(launcher, "MZlauncher");
    writeFileSync(server, "MZserver");
    writeFileSync(config, '{"port":11434}');
    const options = { launcher, server, config, destination, correctionRoot: directory };
    const files = correctionFixture(directory);
    const worker = join(directory, "rechprav/target/release/examples/correction_worker.exe");
    writeFileSync(worker, "ELF worker");
    assert.throws(() => packageWindowsPortable(options), /Correction worker is not a Windows PE/);
    assert.equal(existsSync(destination), false);
    for (const relative of Object.keys(files)) {
      correctionFixture(directory);
      rmSync(join(directory, relative));
      assert.throws(() => packageWindowsPortable(options), /Correction runtime file missing/);
      assert.equal(existsSync(destination), false);
    }
    correctionFixture(directory);
    packageWindowsPortable(options);
    writeFileSync(join(destination, "config.json"), '{"port":12345}');
    rmSync(worker);
    assert.throws(() => packageWindowsPortable(options), /Correction runtime file missing/);
    assert.equal(readFileSync(join(destination, "config.json"), "utf8"), '{"port":12345}');
  });
});

test("native runtime staging preserves the executable and directory layout", () => {
  withTemporaryDirectory(directory => {
    correctionFixture(directory);
    const binary = join(directory, "rechprav/target/release/examples/correction_worker");
    writeFileSync(binary, "native-worker", { mode: 0o755 });
    const destination = join(directory, "native-runtime");
    const files = copyCorrectionRuntime(correctionRuntimeFiles(directory, "linux"), destination);
    assert.equal(readFileSync(files[0], "utf8"), "native-worker");
    assert.equal(readFileSync(join(destination, "rechprav/handy-terms.toml"), "utf8"), 'baseline_config="handy-dictionary.toml"');
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

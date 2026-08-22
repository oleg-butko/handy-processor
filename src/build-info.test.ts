import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { it } from "node:test";
import { APP_VERSION } from "./build-info.js";

it("keeps server and launcher versions in sync", () => {
  const packageMetadata = JSON.parse(
    readFileSync(join(process.cwd(), "package.json"), "utf8"),
  ) as { version: string };
  const launcherConfig = JSON.parse(
    readFileSync(
      join(process.cwd(), "launcher", "src-tauri", "tauri.conf.json"),
      "utf8",
    ),
  ) as { version: string };
  const launcherManifest = readFileSync(
    join(process.cwd(), "launcher", "src-tauri", "Cargo.toml"),
    "utf8",
  );
  const launcherManifestVersion = launcherManifest.match(
    /^version = "([^"]+)"$/mu,
  )?.[1];

  assert.equal(APP_VERSION, packageMetadata.version);
  assert.equal(launcherConfig.version, APP_VERSION);
  assert.equal(launcherManifestVersion, APP_VERSION);
});

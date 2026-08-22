import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { it } from "node:test";
import { APP_VERSION } from "./build-info.js";

it("keeps the runtime version in sync with package.json", () => {
  const packageMetadata = JSON.parse(
    readFileSync(join(process.cwd(), "package.json"), "utf8"),
  ) as { version: string };

  assert.equal(APP_VERSION, packageMetadata.version);
});

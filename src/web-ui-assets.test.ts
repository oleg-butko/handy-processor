import assert from "node:assert/strict";
import { join } from "node:path";
import { describe, it } from "node:test";
import { loadWebUiAssets } from "./web-ui-assets.js";

describe("Web UI assets", () => {
  it("loads the Vite build and supports future client-side routes", () => {
    const assets = loadWebUiAssets(join(process.cwd(), "web-dist"));

    assert.match(assets.get("/")?.body.toString("utf8") ?? "", /Handy Processor/u);
    assert.ok((assets.get("/assets/app.js")?.body.byteLength ?? 0) > 1_000);
    assert.equal(
      assets.get("/history")?.body.toString("utf8"),
      assets.get("/")?.body.toString("utf8"),
    );
  });
});

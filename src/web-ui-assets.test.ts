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

  it("keeps CodeMirror selection colors valid after CSS minification", () => {
    const assets = loadWebUiAssets(join(process.cwd(), "web-dist"));
    const css = assets.get("/assets/style.css")?.body.toString("utf8") ?? "";

    assert.match(
      css,
      /\.voice-editor-field \.cm-selectionBackground\{background-color:[^}]+!important\}/u,
    );
    assert.match(
      css,
      /\.cm-selectionLayer \.cm-selectionBackground\{background-color:[^}]+!important\}/u,
    );
  });

  it("does not emit unresolved Lightning CSS color-scheme variables", () => {
    const assets = loadWebUiAssets(join(process.cwd(), "web-dist"));
    const css = assets.get("/assets/style.css")?.body.toString("utf8") ?? "";

    assert.doesNotMatch(css, /--lightningcss-(?:light|dark)/u);
  });

  it("keeps dedicated surfaces for both intermediate themes", () => {
    const assets = loadWebUiAssets(join(process.cwd(), "web-dist"));
    const css = assets.get("/assets/style.css")?.body.toString("utf8") ?? "";

    assert.match(css, /data-hp-theme=soft-light[^}]+--hp-surface:/u);
    assert.match(css, /data-hp-theme=soft-dark[^}]+--hp-surface:/u);
    assert.match(css, /\.hp-surface\{background:var\(--hp-surface\)\}/u);
    assert.match(
      css,
      /\.voice-editor-field \.cm-editor\{[^}]*background:var\(--hp-editor-surface\)/u,
    );
  });
});

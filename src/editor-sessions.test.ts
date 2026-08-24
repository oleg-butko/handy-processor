import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { EditorSessionRegistry } from "./editor-sessions.js";
import { TRANSFORMATION_SOURCES } from "./transformation-context.js";

function update(
  registry: EditorSessionRegistry,
  overrides: Partial<Parameters<EditorSessionRegistry["update"]>[1]> = {},
) {
  return registry.update("session-1", {
    documentText: "Первое. ",
    documentRevision: 1,
    selectionStart: 8,
    selectionEnd: 8,
    active: true,
    ...overrides,
  });
}

describe("EditorSessionRegistry", () => {
  it("provides context for one active fresh editor", () => {
    const registry = new EditorSessionRegistry();
    const result = update(registry);

    assert.equal(result.ok, true);
    assert.deepEqual(registry.currentContext(), {
      source: TRANSFORMATION_SOURCES.voiceEditor,
      editor: {
        sessionId: "session-1",
        documentRevision: 1,
        selectionStart: 8,
        selectionEnd: 8,
        sentencePosition: "start",
      },
    });
  });

  it("expires context and falls back to external input", () => {
    let now = 1_000;
    const registry = new EditorSessionRegistry(100, () => now);
    update(registry);
    now += 100;

    assert.deepEqual(registry.currentContext(), {
      source: TRANSFORMATION_SOURCES.external,
    });
  });

  it("makes the most recently activated session exclusive", () => {
    const registry = new EditorSessionRegistry();
    update(registry);
    registry.update("session-2", {
      documentText: "Вторая строка",
      documentRevision: 1,
      selectionStart: 6,
      selectionEnd: 6,
      active: true,
    });

    assert.equal(
      registry.currentContext().source,
      TRANSFORMATION_SOURCES.voiceEditor,
    );
    const context = registry.currentContext();
    assert.equal(
      context.source === TRANSFORMATION_SOURCES.voiceEditor
        ? context.editor.sessionId
        : undefined,
      "session-2",
    );
  });

  it("rejects reordered state and accepts a same-revision heartbeat", () => {
    const registry = new EditorSessionRegistry();
    assert.equal(update(registry).ok, true);
    assert.equal(update(registry, { active: false }).ok, true);
    assert.deepEqual(registry.currentContext(), {
      source: TRANSFORMATION_SOURCES.external,
    });
    assert.deepEqual(
      update(registry, { documentRevision: 0 }),
      { ok: false, reason: "stale-revision" },
    );
    assert.deepEqual(
      update(registry, { documentText: "Другое" }),
      { ok: false, reason: "stale-revision" },
    );
  });

  it("removes a session explicitly", () => {
    const registry = new EditorSessionRegistry();
    update(registry);
    registry.remove("session-1");
    assert.deepEqual(registry.currentContext(), {
      source: TRANSFORMATION_SOURCES.external,
    });
    assert.deepEqual(update(registry), {
      ok: false,
      reason: "stale-revision",
    });
    assert.equal(update(registry, { documentRevision: 2 }).ok, true);
  });

  it("rejects a delayed active update after a newer blur update", () => {
    const registry = new EditorSessionRegistry();
    assert.equal(update(registry).ok, true);
    assert.equal(
      update(registry, { documentRevision: 2, active: false }).ok,
      true,
    );
    assert.deepEqual(update(registry), {
      ok: false,
      reason: "stale-revision",
    });
    assert.deepEqual(registry.currentContext(), {
      source: TRANSFORMATION_SOURCES.external,
    });
  });

  it("uses a client revision when deletion arrives before an update", () => {
    const registry = new EditorSessionRegistry();
    registry.remove("session-1", 3);
    assert.deepEqual(update(registry, { documentRevision: 3 }), {
      ok: false,
      reason: "stale-revision",
    });
    assert.equal(update(registry, { documentRevision: 4 }).ok, true);
  });
});

import assert from "node:assert/strict";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, it } from "node:test";
import { createRequestHandler, MODEL_ID } from "./app.js";
import { APP_VERSION, SERVICE_ID } from "./build-info.js";
import { EditorSessionRegistry } from "./editor-sessions.js";
import { RuleSettings } from "./rule-settings.js";
import { TRANSFORMATION_RULE_IDS } from "./transformation-rules.js";
import { TransformationLog } from "./transformation-log.js";
import type { WebUiAssets } from "./web-ui-assets.js";

const transformationLog = new TransformationLog(":memory:");
const ruleSettings = new RuleSettings(transformationLog);
const editorSessions = new EditorSessionRegistry();
const webUiAssets: WebUiAssets = {
  get(path) {
    return path === "/"
      ? {
          body: Buffer.from("<!doctype html><title>Handy Processor</title>"),
          contentType: "text/html; charset=utf-8",
        }
      : undefined;
  },
};
const handleRequest = createRequestHandler({
  transformationHistory: transformationLog,
  ruleSettings,
  editorSessions,
  editorDraftStorage: transformationLog,
  webUiAssets,
});
const server = createServer((request, response) => {
  void handleRequest(request, response);
});

let baseUrl = "";

before(async () => {
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address() as AddressInfo;
  baseUrl = `http://127.0.0.1:${address.port}`;
});

after(async () => {
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  transformationLog.close();
});

describe("OpenAI-compatible API", () => {
  it("identifies the running service", async () => {
    const response = await fetch(`${baseUrl}/health`);
    const body = (await response.json()) as Record<string, unknown>;

    assert.equal(response.status, 200);
    assert.deepEqual(body, {
      status: "ok",
      service: SERVICE_ID,
      version: APP_VERSION,
      pid: process.pid,
    });
  });

  it("serves the browser UI", async () => {
    const response = await fetch(`${baseUrl}/`);

    assert.equal(response.status, 200);
    assert.match(await response.text(), /Handy Processor/u);
  });

  it("lists the local model", async () => {
    const response = await fetch(`${baseUrl}/v1/models`);
    const body = (await response.json()) as {
      data: Array<{ id: string }>;
    };

    assert.equal(response.status, 200);
    assert.equal(body.data[0]?.id, MODEL_ID);
  });

  it("processes and logs the last user message", async () => {
    const originalText = "МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.";
    const response = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model: MODEL_ID,
        messages: [{ role: "user", content: originalText }],
        stream: false,
        reasoning_effort: "none",
      }),
    });
    const body = (await response.json()) as {
      choices: Array<{ message: { content: string } }>;
    };

    assert.equal(response.status, 200);
    assert.equal(body.choices[0]?.message.content, "мне нужно 22 файла");

    const historyResponse = await fetch(
      `${baseUrl}/api/transformations?page=1&pageSize=1`,
    );
    const history = (await historyResponse.json()) as {
      items: Array<{
        originalText: string;
        transformedText: string;
        source: string;
        context: unknown;
        appliedRules: Array<{ id: string; changed: boolean }>;
      }>;
      pagination: { total: number };
    };

    assert.equal(historyResponse.status, 200);
    assert.equal(history.items[0]?.originalText, originalText);
    assert.equal(history.items[0]?.transformedText, "мне нужно 22 файла");
    assert.equal(history.items[0]?.source, "external");
    assert.equal(history.items[0]?.context, null);
    assert.deepEqual(
      history.items[0]?.appliedRules.map((rule) => rule.id),
      [
        TRANSFORMATION_RULE_IDS.lowercase,
        TRANSFORMATION_RULE_IDS.russianNumerals,
        TRANSFORMATION_RULE_IDS.removeFinalPeriod,
      ],
    );
    assert.equal(history.pagination.total, 1);
  });

  it("lists and updates rules used by subsequent transformations", async () => {
    const rulesResponse = await fetch(`${baseUrl}/api/rules`);
    const rules = (await rulesResponse.json()) as {
      items: Array<{ id: string; enabled: boolean }>;
    };
    assert.equal(rulesResponse.status, 200);
    assert.equal(rules.items.length, 4);

    const updateResponse = await fetch(
      `${baseUrl}/api/rules/${TRANSFORMATION_RULE_IDS.lowercase}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ enabled: false }),
      },
    );
    assert.equal(updateResponse.status, 200);

    const transformResponse = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        messages: [
          {
            role: "user",
            content: "МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.",
          },
        ],
      }),
    });
    const transformation = (await transformResponse.json()) as {
      choices: Array<{ message: { content: string } }>;
    };
    assert.equal(
      transformation.choices[0]?.message.content,
      "МНЕ НУЖНО 22 ФАЙЛА",
    );

    const historyResponse = await fetch(
      `${baseUrl}/api/transformations?page=1&pageSize=1`,
    );
    const history = (await historyResponse.json()) as {
      items: Array<{ appliedRules: Array<{ id: string }> }>;
    };
    assert.deepEqual(
      history.items[0]?.appliedRules.map((rule) => rule.id),
      [
        TRANSFORMATION_RULE_IDS.russianNumerals,
        TRANSFORMATION_RULE_IDS.removeFinalPeriod,
      ],
    );
  });

  it("validates rule updates", async () => {
    const invalidValue = await fetch(
      `${baseUrl}/api/rules/${TRANSFORMATION_RULE_IDS.lowercase}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ enabled: "yes" }),
      },
    );
    assert.equal(invalidValue.status, 400);

    const unknownRule = await fetch(`${baseUrl}/api/rules/unknown.rule`, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ enabled: true }),
    });
    assert.equal(unknownRule.status, 404);
  });

  it("validates history pagination", async () => {
    const response = await fetch(`${baseUrl}/api/transformations?page=0`);
    assert.equal(response.status, 400);
  });

  it("uses a fresh active editor session as transformation context", async () => {
    const sessionResponse = await fetch(
      `${baseUrl}/api/editor/sessions/test-session`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          documentText: "Первое предложение. ",
          documentRevision: 1,
          selectionStart: 20,
          selectionEnd: 20,
          active: true,
        }),
      },
    );
    assert.equal(sessionResponse.status, 200);
    const session = (await sessionResponse.json()) as {
      sentencePosition: string;
    };
    assert.equal(session.sentencePosition, "start");

    const transformResponse = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        messages: [{ role: "user", content: "НОВЫЙ ТЕКСТ." }],
      }),
    });
    assert.equal(transformResponse.status, 200);
    const transformation = (await transformResponse.json()) as {
      choices: Array<{ message: { content: string } }>;
    };
    assert.equal(transformation.choices[0]?.message.content, "НОВЫЙ ТЕКСТ");

    const historyResponse = await fetch(
      `${baseUrl}/api/transformations?page=1&pageSize=1`,
    );
    const history = (await historyResponse.json()) as {
      items: Array<{
        source: string;
        context: { editor: { sentencePosition: string } };
      }>;
    };
    assert.equal(history.items[0]?.source, "voice-editor");
    assert.equal(history.items[0]?.context.editor.sentencePosition, "start");

    const middleSessionResponse = await fetch(
      `${baseUrl}/api/editor/sessions/test-session`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          documentText: "Первое предложение",
          documentRevision: 2,
          selectionStart: 7,
          selectionEnd: 7,
          active: true,
        }),
      },
    );
    assert.equal(middleSessionResponse.status, 200);

    const middleTransformResponse = await fetch(
      `${baseUrl}/v1/chat/completions`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          messages: [{ role: "user", content: "НОВЫЙ ТЕКСТ." }],
        }),
      },
    );
    const middleTransformation = (await middleTransformResponse.json()) as {
      choices: Array<{ message: { content: string } }>;
    };
    assert.equal(middleTransformation.choices[0]?.message.content, "новый текст");

    const deleteResponse = await fetch(
      `${baseUrl}/api/editor/sessions/test-session`,
      { method: "DELETE" },
    );
    assert.equal(deleteResponse.status, 204);
  });

  it("validates editor session state", async () => {
    const invalidSelection = await fetch(
      `${baseUrl}/api/editor/sessions/test-session`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          documentText: "Короткий текст",
          documentRevision: 2,
          selectionStart: 0,
          selectionEnd: 100,
          active: true,
        }),
      },
    );
    assert.equal(invalidSelection.status, 400);
  });

  it("loads and saves the voice editor draft", async () => {
    const initialResponse = await fetch(`${baseUrl}/api/editor/draft`);
    const initial = (await initialResponse.json()) as {
      content: string;
      state: unknown;
      currentNoteId: number | null;
      revision: number;
    };
    assert.equal(initialResponse.status, 200);
    assert.equal(initial.content, "");
    assert.equal(initial.revision, 0);
    assert.equal(initial.state, null);
    assert.equal(initial.currentNoteId, null);

    const saveResponse = await fetch(`${baseUrl}/api/editor/draft`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ content: "Сохранённый текст" }),
    });
    const saved = (await saveResponse.json()) as {
      content: string;
      revision: number;
      updatedAt: string;
    };
    assert.equal(saveResponse.status, 200);
    assert.equal(saved.content, "Сохранённый текст");
    assert.equal(saved.revision, 1);
    assert.match(saved.updatedAt, /^\d{4}-\d{2}-\d{2}T/u);

    const reloadResponse = await fetch(`${baseUrl}/api/editor/draft`);
    assert.deepEqual(await reloadResponse.json(), saved);
  });

  it("creates and updates voice editor notes", async () => {
    const state = { schemaVersion: 1, editor: { doc: "Текст заметки" } };
    const createResponse = await fetch(`${baseUrl}/api/editor/notes`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name: "  Рабочая заметка  ",
        content: "Текст заметки",
        state,
      }),
    });
    const created = (await createResponse.json()) as {
      id: number;
      name: string;
      revision: number;
    };
    assert.equal(createResponse.status, 201);
    assert.equal(created.name, "Рабочая заметка");
    assert.equal(created.revision, 1);

    const listResponse = await fetch(`${baseUrl}/api/editor/notes`);
    const list = (await listResponse.json()) as {
      items: Array<{ id: number; name: string }>;
    };
    assert.equal(listResponse.status, 200);
    assert.equal(list.items[0]?.id, created.id);

    const updateResponse = await fetch(
      `${baseUrl}/api/editor/notes/${created.id}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ content: "Новый текст", state: null }),
      },
    );
    const updated = (await updateResponse.json()) as {
      id: number;
      name: string;
      content: string;
      state: unknown;
      revision: number;
      createdAt: string;
      updatedAt: string;
    };
    assert.equal(updateResponse.status, 200);
    assert.equal(updated.content, "Новый текст");
    assert.equal(updated.revision, 2);

    const renameResponse = await fetch(
      `${baseUrl}/api/editor/notes/${created.id}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "  Переименованная заметка  " }),
      },
    );
    const renamed = (await renameResponse.json()) as {
      id: number;
      name: string;
      content: string;
      revision: number;
      updatedAt: string;
    };
    assert.equal(renameResponse.status, 200);
    assert.equal(renamed.name, "Переименованная заметка");
    assert.equal(renamed.content, "Новый текст");
    assert.equal(renamed.revision, 2);

    const loadResponse = await fetch(
      `${baseUrl}/api/editor/notes/${created.id}`,
    );
    assert.deepEqual(await loadResponse.json(), {
      ...updated,
      name: "Переименованная заметка",
      updatedAt: renamed.updatedAt,
    });

    const invalidRenameResponse = await fetch(
      `${baseUrl}/api/editor/notes/${created.id}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "   " }),
      },
    );
    assert.equal(invalidRenameResponse.status, 400);

    const deleteResponse = await fetch(
      `${baseUrl}/api/editor/notes/${created.id}`,
      { method: "DELETE" },
    );
    assert.equal(deleteResponse.status, 204);

    const deletedResponse = await fetch(
      `${baseUrl}/api/editor/notes/${created.id}`,
    );
    assert.equal(deletedResponse.status, 404);

    const missingResponse = await fetch(`${baseUrl}/api/editor/notes/999999`);
    assert.equal(missingResponse.status, 404);
    const missingDeleteResponse = await fetch(
      `${baseUrl}/api/editor/notes/999999`,
      { method: "DELETE" },
    );
    assert.equal(missingDeleteResponse.status, 404);
  });

  it("does not serve the SPA for unknown API routes", async () => {
    const response = await fetch(`${baseUrl}/v1/unknown`);
    assert.equal(response.status, 404);
    assert.match(response.headers.get("content-type") ?? "", /application\/json/u);
  });

  it("returns an OpenAI-style validation error", async () => {
    const response = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: MODEL_ID, messages: [] }),
    });
    const body = (await response.json()) as {
      error: { type: string };
    };

    assert.equal(response.status, 400);
    assert.equal(body.error.type, "invalid_request_error");
  });
});

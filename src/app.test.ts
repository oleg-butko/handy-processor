import assert from "node:assert/strict";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, it } from "node:test";
import { createRequestHandler, MODEL_ID } from "./app.js";
import { APP_VERSION, SERVICE_ID } from "./build-info.js";
import { RuleSettings } from "./rule-settings.js";
import { TRANSFORMATION_RULE_IDS } from "./transformation-rules.js";
import { TransformationLog } from "./transformation-log.js";
import type { WebUiAssets } from "./web-ui-assets.js";

const transformationLog = new TransformationLog(":memory:");
const ruleSettings = new RuleSettings(transformationLog);
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
        appliedRules: Array<{ id: string; changed: boolean }>;
      }>;
      pagination: { total: number };
    };

    assert.equal(historyResponse.status, 200);
    assert.equal(history.items[0]?.originalText, originalText);
    assert.equal(history.items[0]?.transformedText, "мне нужно 22 файла");
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
    assert.equal(rules.items.length, 3);

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

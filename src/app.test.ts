import assert from "node:assert/strict";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, it } from "node:test";
import { handleRequest, MODEL_ID } from "./app.js";

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
});

describe("OpenAI-compatible API", () => {
  it("lists the local model", async () => {
    const response = await fetch(`${baseUrl}/v1/models`);
    const body = (await response.json()) as {
      data: Array<{ id: string }>;
    };

    assert.equal(response.status, 200);
    assert.equal(body.data[0]?.id, MODEL_ID);
  });

  it("processes the last user message", async () => {
    const response = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model: MODEL_ID,
        messages: [{ role: "user", content: "THIS IS A TEST." }],
        stream: false,
        reasoning_effort: "none",
      }),
    });
    const body = (await response.json()) as {
      choices: Array<{ message: { content: string } }>;
    };

    assert.equal(response.status, 200);
    assert.equal(body.choices[0]?.message.content, "this is a test");
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

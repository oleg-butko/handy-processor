import type { IncomingMessage, ServerResponse } from "node:http";
import { transformTranscription } from "./transform.js";

export const MODEL_ID = "handy-basic-transformer";

const MAX_REQUEST_BYTES = 1024 * 1024;

type JsonObject = Record<string, unknown>;

class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

function sendJson(
  response: ServerResponse,
  status: number,
  body: unknown,
): void {
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
  });
  response.end(JSON.stringify(body));
}

function sendError(response: ServerResponse, status: number, message: string) {
  sendJson(response, status, {
    error: {
      message,
      type: status === 404 ? "not_found_error" : "invalid_request_error",
    },
  });
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

async function readJson(request: IncomingMessage): Promise<JsonObject> {
  let size = 0;
  const chunks: Buffer[] = [];

  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += buffer.byteLength;

    if (size > MAX_REQUEST_BYTES) {
      throw new HttpError(413, "Request body is too large");
    }

    chunks.push(buffer);
  }

  if (chunks.length === 0) {
    throw new HttpError(400, "Request body must be JSON");
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new HttpError(400, "Request body contains invalid JSON");
  }

  if (!isObject(parsed)) {
    throw new HttpError(400, "Request body must be a JSON object");
  }

  return parsed;
}

function getLastUserMessage(body: JsonObject): string {
  if (!Array.isArray(body.messages)) {
    throw new HttpError(400, "The 'messages' field must be an array");
  }

  for (let index = body.messages.length - 1; index >= 0; index -= 1) {
    const message = body.messages[index];
    if (
      isObject(message) &&
      message.role === "user" &&
      typeof message.content === "string"
    ) {
      return message.content;
    }
  }

  throw new HttpError(
    400,
    "A user message with string content is required",
  );
}

function getPath(request: IncomingMessage): string {
  try {
    return new URL(request.url ?? "/", "http://localhost").pathname.replace(
      /\/$/u,
      "",
    );
  } catch {
    return request.url ?? "/";
  }
}

export async function handleRequest(
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  const path = getPath(request) || "/";

  try {
    if (request.method === "GET" && path === "/health") {
      sendJson(response, 200, { status: "ok" });
      return;
    }

    if (request.method === "GET" && path === "/v1/models") {
      sendJson(response, 200, {
        object: "list",
        data: [
          {
            id: MODEL_ID,
            object: "model",
            created: 0,
            owned_by: "local",
          },
        ],
      });
      return;
    }

    if (request.method === "POST" && path === "/v1/chat/completions") {
      const body = await readJson(request);
      const input = getLastUserMessage(body);
      const content = transformTranscription(input);
      const model = typeof body.model === "string" ? body.model : MODEL_ID;

      sendJson(response, 200, {
        id: `chatcmpl-local-${Date.now()}`,
        object: "chat.completion",
        created: Math.floor(Date.now() / 1000),
        model,
        choices: [
          {
            index: 0,
            message: { role: "assistant", content },
            finish_reason: "stop",
          },
        ],
        usage: {
          prompt_tokens: 0,
          completion_tokens: 0,
          total_tokens: 0,
        },
      });
      return;
    }

    sendError(response, 404, "Endpoint not found");
  } catch (error) {
    if (error instanceof HttpError) {
      sendError(response, error.status, error.message);
      return;
    }

    console.error("Unexpected request error", error);
    sendError(response, 500, "Internal server error");
  }
}

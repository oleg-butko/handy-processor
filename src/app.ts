import type { IncomingMessage, ServerResponse } from "node:http";
import { APP_VERSION, SERVICE_ID } from "./build-info.js";
import type { EditorSessionRegistry } from "./editor-sessions.js";
import type { RuleSettings } from "./rule-settings.js";
import type { TransformationContext } from "./transformation-context.js";
import { transformTranscription } from "./transform.js";
import type { AppliedTransformationRule } from "./transformation-rules.js";
import type {
  EditorDraft,
  TransformationPage,
} from "./transformation-log.js";
import type { WebAsset, WebUiAssets } from "./web-ui-assets.js";

export const MODEL_ID = "handy-basic-transformer";

const MAX_REQUEST_BYTES = 1024 * 1024;
const DEFAULT_PAGE_SIZE = 10;
const MAX_PAGE_SIZE = 100;

type JsonObject = Record<string, unknown>;

export interface TransformationHistory {
  add(
    originalText: string,
    transformedText: string,
    appliedRules: readonly AppliedTransformationRule[],
    context: TransformationContext,
  ): void;
  list(page: number, pageSize: number): TransformationPage;
}

export interface EditorDraftStorage {
  loadEditorDraft(): EditorDraft;
  saveEditorDraft(content: string): EditorDraft;
}

export interface AppDependencies {
  readonly transformationHistory: TransformationHistory;
  readonly ruleSettings: RuleSettings;
  readonly editorSessions: EditorSessionRegistry;
  readonly editorDraftStorage: EditorDraftStorage;
  readonly webUiAssets: WebUiAssets;
}

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

function sendAsset(response: ServerResponse, asset: WebAsset): void {
  response.writeHead(200, {
    "content-type": asset.contentType,
    "content-length": asset.body.byteLength,
    "cache-control": "no-cache",
    "x-content-type-options": "nosniff",
  });
  response.end(asset.body);
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

function getRequestUrl(request: IncomingMessage): URL {
  try {
    return new URL(request.url ?? "/", "http://localhost");
  } catch {
    throw new HttpError(400, "Request URL is invalid");
  }
}

function normalizePath(pathname: string): string {
  return pathname.replace(/\/$/u, "") || "/";
}

function decodeRuleId(path: string): string {
  try {
    return decodeURIComponent(path.slice("/api/rules/".length));
  } catch {
    throw new HttpError(400, "Transformation rule id is invalid");
  }
}

function decodeEditorSessionId(path: string): string {
  try {
    return decodeURIComponent(path.slice("/api/editor/sessions/".length));
  } catch {
    throw new HttpError(400, "Editor session id is invalid");
  }
}

function validateEditorSessionId(sessionId: string): void {
  if (!/^[A-Za-z0-9_-]{1,128}$/u.test(sessionId)) {
    throw new HttpError(400, "Editor session id is invalid");
  }
}

function readEditorOffset(
  body: JsonObject,
  name: "selectionStart" | "selectionEnd" | "documentRevision",
): number {
  const value = body[name];
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new HttpError(400, `The '${name}' field must be a non-negative integer`);
  }
  return value as number;
}

function isApiPath(path: string): boolean {
  return (
    path === "/api" ||
    path.startsWith("/api/") ||
    path === "/v1" ||
    path.startsWith("/v1/")
  );
}

function readPositiveInteger(
  parameters: URLSearchParams,
  name: string,
  defaultValue: number,
  maximum?: number,
): number {
  const rawValue = parameters.get(name);
  if (rawValue === null) {
    return defaultValue;
  }

  const value = Number(rawValue);
  if (
    !Number.isSafeInteger(value) ||
    value < 1 ||
    (maximum !== undefined && value > maximum)
  ) {
    const range =
      maximum === undefined ? "a positive integer" : `from 1 to ${maximum}`;
    throw new HttpError(400, `'${name}' must be ${range}`);
  }

  return value;
}

function readOptionalNonNegativeInteger(
  parameters: URLSearchParams,
  name: string,
): number | undefined {
  const rawValue = parameters.get(name);
  if (rawValue === null) {
    return undefined;
  }
  const value = Number(rawValue);
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new HttpError(400, `'${name}' must be a non-negative integer`);
  }
  return value;
}

function getTransformationPage(
  history: TransformationHistory,
  parameters: URLSearchParams,
): object {
  const page = readPositiveInteger(parameters, "page", 1);
  const pageSize = readPositiveInteger(
    parameters,
    "pageSize",
    DEFAULT_PAGE_SIZE,
    MAX_PAGE_SIZE,
  );
  const result = history.list(page, pageSize);

  return {
    items: result.items,
    pagination: {
      page,
      pageSize,
      total: result.total,
      totalPages: Math.ceil(result.total / pageSize),
    },
  };
}

export function createRequestHandler({
  transformationHistory,
  ruleSettings,
  editorSessions,
  editorDraftStorage,
  webUiAssets,
}: AppDependencies) {
  return async function handleRequest(
    request: IncomingMessage,
    response: ServerResponse,
  ): Promise<void> {
    try {
      const url = getRequestUrl(request);
      const path = normalizePath(url.pathname);

      if (request.method === "GET" && path === "/health") {
        sendJson(response, 200, {
          status: "ok",
          service: SERVICE_ID,
          version: APP_VERSION,
          pid: process.pid,
        });
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
        const context = editorSessions.currentContext();
        const transformation = transformTranscription(
          input,
          ruleSettings.enabledRuleIds(),
          context,
        );
        const content = transformation.text;
        const model = typeof body.model === "string" ? body.model : MODEL_ID;

        transformationHistory.add(
          input,
          content,
          transformation.appliedRules,
          context,
        );

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

      if (request.method === "GET" && path === "/api/transformations") {
        sendJson(
          response,
          200,
          getTransformationPage(transformationHistory, url.searchParams),
        );
        return;
      }

      if (request.method === "GET" && path === "/api/rules") {
        sendJson(response, 200, { items: ruleSettings.list() });
        return;
      }

      if (request.method === "GET" && path === "/api/editor/draft") {
        sendJson(response, 200, editorDraftStorage.loadEditorDraft());
        return;
      }

      if (request.method === "PUT" && path === "/api/editor/draft") {
        const body = await readJson(request);
        if (typeof body.content !== "string") {
          throw new HttpError(400, "The 'content' field must be a string");
        }
        sendJson(response, 200, editorDraftStorage.saveEditorDraft(body.content));
        return;
      }

      if (
        request.method === "PUT" &&
        path.startsWith("/api/editor/sessions/")
      ) {
        const sessionId = decodeEditorSessionId(path);
        validateEditorSessionId(sessionId);
        const body = await readJson(request);
        if (typeof body.documentText !== "string") {
          throw new HttpError(400, "The 'documentText' field must be a string");
        }
        if (typeof body.active !== "boolean") {
          throw new HttpError(400, "The 'active' field must be a boolean");
        }
        const documentRevision = readEditorOffset(body, "documentRevision");
        const selectionStart = readEditorOffset(body, "selectionStart");
        const selectionEnd = readEditorOffset(body, "selectionEnd");
        if (
          selectionStart > selectionEnd ||
          selectionEnd > body.documentText.length
        ) {
          throw new HttpError(400, "Editor selection is outside the document");
        }

        const result = editorSessions.update(sessionId, {
          documentText: body.documentText,
          documentRevision,
          selectionStart,
          selectionEnd,
          active: body.active,
        });
        if (!result.ok) {
          throw new HttpError(409, "Editor session revision is stale");
        }
        sendJson(response, 200, result.state);
        return;
      }

      if (
        request.method === "DELETE" &&
        path.startsWith("/api/editor/sessions/")
      ) {
        const sessionId = decodeEditorSessionId(path);
        validateEditorSessionId(sessionId);
        editorSessions.remove(
          sessionId,
          readOptionalNonNegativeInteger(
            url.searchParams,
            "documentRevision",
          ),
        );
        response.writeHead(204, { "cache-control": "no-store" });
        response.end();
        return;
      }

      if (request.method === "PATCH" && path.startsWith("/api/rules/")) {
        const body = await readJson(request);
        if (typeof body.enabled !== "boolean") {
          throw new HttpError(400, "The 'enabled' field must be a boolean");
        }

        const rule = ruleSettings.setEnabled(decodeRuleId(path), body.enabled);
        if (rule === undefined) {
          throw new HttpError(404, "Transformation rule not found");
        }

        sendJson(response, 200, rule);
        return;
      }

      if (request.method === "GET" && !isApiPath(path)) {
        const asset = webUiAssets.get(path);
        if (asset !== undefined) {
          sendAsset(response, asset);
          return;
        }
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
  };
}

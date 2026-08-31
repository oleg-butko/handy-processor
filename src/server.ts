import { createServer } from "node:http";
import { join } from "node:path";
import { createRequestHandler, MODEL_ID } from "./app.js";
import {
  DIAGNOSTIC_LOG_FILE_NAME,
  FileDiagnosticLog,
} from "./diagnostic-log.js";
import { EditorSessionRegistry } from "./editor-sessions.js";
import {
  CONFIG_FILE_NAME,
  DATABASE_FILE_NAME,
  getDatabasePath,
  loadRuntimeConfig,
  resolveRuntimeDirectory,
} from "./runtime-config.js";
import {
  isManagedProcess,
  monitorManagedInput,
  type ManagedInputMonitor,
} from "./managed-lifecycle.js";
import { RuleSettings } from "./rule-settings.js";
import { TransformationLog } from "./transformation-log.js";
import { loadWebUiAssets } from "./web-ui-assets.js";

const host = "127.0.0.1";
const runtimeDirectory = resolveRuntimeDirectory();
const config = loadRuntimeConfig(runtimeDirectory);
const transformationLog = new TransformationLog(
  getDatabasePath(runtimeDirectory),
);
const ruleSettings = new RuleSettings(transformationLog);
const editorSessions = new EditorSessionRegistry();
const diagnosticLog = new FileDiagnosticLog(runtimeDirectory);
const webUiAssets = loadWebUiAssets(join(process.cwd(), "web-dist"));
const handleRequest = createRequestHandler({
  transformationHistory: transformationLog,
  ruleSettings,
  editorSessions,
  editorDraftStorage: transformationLog,
  diagnosticLog,
  webUiAssets,
});

const server = createServer((request, response) => {
  void handleRequest(request, response);
});

let managedInput: ManagedInputMonitor | undefined;
let stopping = false;
let storageClosed = false;

function closeStorage(): void {
  if (storageClosed) {
    return;
  }

  storageClosed = true;
  transformationLog.close();
}

function requestShutdown(reason: string): void {
  if (stopping) {
    return;
  }

  stopping = true;
  managedInput?.stop();
  console.log(`Stopping Handy Processor: ${reason}`);

  server.close((error) => {
    closeStorage();
    if (error) {
      console.error("Failed to stop server cleanly", error);
      process.exitCode = 1;
    }
  });
}

if (isManagedProcess()) {
  managedInput = monitorManagedInput(process.stdin, requestShutdown);
}

server.listen(config.port, host, () => {
  console.log(`Handy Processor is running.`);
  console.log(`Web UI: http://localhost:${config.port}/`);
  console.log(`Handy API: http://${host}:${config.port}/v1`);
  console.log(`Model: ${MODEL_ID}`);
  console.log(`Config: ${join(runtimeDirectory, CONFIG_FILE_NAME)}`);
  console.log(`Database: ${join(runtimeDirectory, DATABASE_FILE_NAME)}`);
  console.log(
    `Diagnostic log: ${join(runtimeDirectory, DIAGNOSTIC_LOG_FILE_NAME)}`,
  );
  console.log(`Mode: ${managedInput === undefined ? "standalone" : "managed"}`);
});

server.once("error", (error) => {
  managedInput?.stop();
  closeStorage();
  console.error("Failed to start Handy Processor", error);
  process.exitCode = 1;
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    requestShutdown(`received ${signal}`);
  });
}

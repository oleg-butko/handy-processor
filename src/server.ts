import { createServer } from "node:http";
import { join } from "node:path";
import { createRequestHandler, MODEL_ID } from "./app.js";
import {
  CONFIG_FILE_NAME,
  DATABASE_FILE_NAME,
  getDatabasePath,
  loadRuntimeConfig,
  resolveRuntimeDirectory,
} from "./runtime-config.js";
import { TransformationLog } from "./transformation-log.js";
import { loadWebUiAssets } from "./web-ui-assets.js";

const host = "127.0.0.1";
const runtimeDirectory = resolveRuntimeDirectory();
const config = loadRuntimeConfig(runtimeDirectory);
const transformationLog = new TransformationLog(
  getDatabasePath(runtimeDirectory),
);
const webUiAssets = loadWebUiAssets(join(process.cwd(), "web-dist"));
const handleRequest = createRequestHandler({
  transformationHistory: transformationLog,
  webUiAssets,
});

const server = createServer((request, response) => {
  void handleRequest(request, response);
});

server.listen(config.port, host, () => {
  console.log(`Handy Processor is running.`);
  console.log(`Web UI: http://localhost:${config.port}/`);
  console.log(`Handy API: http://${host}:${config.port}/v1`);
  console.log(`Model: ${MODEL_ID}`);
  console.log(`Config: ${join(runtimeDirectory, CONFIG_FILE_NAME)}`);
  console.log(`Database: ${join(runtimeDirectory, DATABASE_FILE_NAME)}`);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    server.close((error) => {
      transformationLog.close();
      if (error) {
        console.error("Failed to stop server cleanly", error);
        process.exitCode = 1;
      }
    });
  });
}

import { createServer } from "node:http";
import { handleRequest, MODEL_ID } from "./app.js";

function readPort(value: string | undefined): number {
  const port = Number(value ?? "11434");
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`Invalid PORT: ${value}`);
  }
  return port;
}

const host = process.env.HOST?.trim() || "127.0.0.1";
const port = readPort(process.env.PORT);

const server = createServer((request, response) => {
  void handleRequest(request, response);
});

server.listen(port, host, () => {
  console.log(`Handy processor listening at http://${host}:${port}/v1`);
  console.log(`Model: ${MODEL_ID}`);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    server.close((error) => {
      if (error) {
        console.error("Failed to stop server cleanly", error);
        process.exitCode = 1;
      }
    });
  });
}

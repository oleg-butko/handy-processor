import assert from "node:assert/strict";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { once } from "node:events";
import { describe, it } from "node:test";
import { APP_VERSION, SERVICE_ID } from "./build-info.js";

interface ManagedServer {
  readonly child: ChildProcessWithoutNullStreams;
  readonly directory: string;
  readonly url: string;
  readonly output: string[];
}

async function reservePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const port = address.port;
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  return port;
}

async function startManagedServer(stallCorrection = false): Promise<ManagedServer> {
  const directory = mkdtempSync(join(tmpdir(), "handy-processor-managed-"));
  const port = await reservePort();
  writeFileSync(
    join(directory, "config.json"),
    `${JSON.stringify({ port })}\n`,
    "utf8",
  );
  const stalledWorker = join(directory, "stalled-worker.mjs");
  if (stallCorrection) writeFileSync(stalledWorker, `
    import { writeFileSync } from 'node:fs';
    writeFileSync(${JSON.stringify(join(directory, "worker-started"))}, 'started');
    process.stdin.resume(); process.stdin.on('end', () => process.exit(0));
    setInterval(() => {}, 1000);
  `);

  const child = spawn(
    process.execPath,
    [join(process.cwd(), "dist/server.js"), "--managed"],
    {
      cwd: process.cwd(),
      env: {
        ...process.env,
        HANDY_PROCESSOR_RUNTIME_DIR: directory,
        ...(stallCorrection ? {
          HANDY_PROCESSOR_CORRECTION_BINARY: process.execPath,
          HANDY_PROCESSOR_CORRECTION_CONFIG: stalledWorker,
        } : {}),
      },
      stdio: ["pipe", "pipe", "pipe"],
    },
  );
  const output: string[] = [];
  child.stdout.on("data", (chunk: Buffer) => output.push(chunk.toString()));
  child.stderr.on("data", (chunk: Buffer) => output.push(chunk.toString()));

  const url = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`Managed server exited early:\n${output.join("")}`);
    }

    try {
      const response = await fetch(`${url}/health`);
      if (response.ok) {
        return { child, directory, url, output };
      }
    } catch {
      // The server has not bound the port yet.
    }

    await new Promise((resolve) => setTimeout(resolve, 25));
  }

  child.kill();
  throw new Error(`Timed out waiting for managed server:\n${output.join("")}`);
}

async function waitForExit(server: ManagedServer): Promise<number | null> {
  if (server.child.exitCode !== null || server.child.signalCode !== null) {
    return server.child.exitCode;
  }

  const [code] = (await Promise.race([
    once(server.child, "exit"),
    new Promise<never>((_, reject) => {
      setTimeout(
        () => reject(new Error(`Timed out waiting for exit:\n${server.output.join("")}`)),
        5_000,
      );
    }),
  ])) as [number | null];
  return code;
}

async function exerciseManagedShutdown(mode: "command" | "eof"): Promise<void> {
  const server = await startManagedServer();
  try {
    const health = (await (await fetch(`${server.url}/health`)).json()) as {
      service: string;
      version: string;
    };
    assert.equal(health.service, SERVICE_ID);
    assert.equal(health.version, APP_VERSION);

    if (mode === "command") {
      server.child.stdin.write("shutdown\n");
    } else {
      server.child.stdin.end();
    }

    assert.equal(await waitForExit(server), 0, server.output.join(""));
    assert.equal(existsSync(join(server.directory, "handy-processor.sqlite3")), true);
  } finally {
    if (server.child.exitCode === null && server.child.signalCode === null) {
      server.child.kill();
      await waitForExit(server);
    }
    rmSync(server.directory, { recursive: true, force: true });
  }
}

describe("managed server process", () => {
  it("cancels a stalled correction before waiting for HTTP requests to drain", async () => {
    const server = await startManagedServer(true);
    let response: Promise<Response> | undefined;
    let timer: NodeJS.Timeout | undefined;
    try {
      await fetch(`${server.url}/api/rules/recognition.terms-and-phrases`, {
        method: "PATCH", headers: { "content-type": "application/json" }, body: '{"enabled":true}',
      });
      response = fetch(`${server.url}/v1/chat/completions`, {
        method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify({ messages: [{ role: "user", content: "калиборд" }] }),
      });
      void response.catch(() => {});
      const deadline = Date.now() + 2000;
      while (!existsSync(join(server.directory, "worker-started")) && Date.now() < deadline) {
        await new Promise(resolve => setTimeout(resolve, 20));
      }
      assert.ok(existsSync(join(server.directory, "worker-started")));
      const exit = once(server.child, "exit");
      server.child.stdin.write("shutdown\n");
      const result = await Promise.race([exit, new Promise(resolve => {
        timer = setTimeout(() => resolve("hung"), 2000);
      })]);
      assert.notEqual(result, "hung", server.output.join(""));
      assert.equal((result as [number | null])[0], 0);
      assert.equal((await (await response).json()).choices[0].message.content, "калиборд");
    } finally {
      clearTimeout(timer);
      if (server.child.exitCode === null && server.child.signalCode === null) {
        const exit = once(server.child, "exit");
        server.child.kill("SIGKILL");
        await exit;
      }
      await response?.catch(() => {});
      rmSync(server.directory, { recursive: true, force: true });
    }
  });

  it("shuts down on the launcher command", async () => {
    await exerciseManagedShutdown("command");
  });

  it("shuts down when the launcher pipe closes", async () => {
    await exerciseManagedShutdown("eof");
  });
});

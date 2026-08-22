import type { Readable } from "node:stream";

export const MANAGED_FLAG = "--managed";

export interface ManagedInputMonitor {
  stop(): void;
}

export function isManagedProcess(
  arguments_: readonly string[] = process.argv.slice(2),
): boolean {
  return arguments_.includes(MANAGED_FLAG);
}

export function monitorManagedInput(
  input: Readable,
  requestShutdown: (reason: string) => void,
): ManagedInputMonitor {
  let bufferedInput = "";
  let shutdownRequested = false;

  const requestOnce = (reason: string): void => {
    if (shutdownRequested) {
      return;
    }

    shutdownRequested = true;
    requestShutdown(reason);
  };

  const onData = (chunk: Buffer | string): void => {
    bufferedInput += chunk.toString();

    let newlineIndex = bufferedInput.indexOf("\n");
    while (newlineIndex >= 0) {
      const command = bufferedInput.slice(0, newlineIndex).trim();
      bufferedInput = bufferedInput.slice(newlineIndex + 1);
      if (command === "shutdown") {
        requestOnce("launcher requested shutdown");
        return;
      }
      newlineIndex = bufferedInput.indexOf("\n");
    }
  };

  const onEnd = (): void => requestOnce("launcher disconnected");

  input.on("data", onData);
  input.once("end", onEnd);
  input.resume();

  return {
    stop() {
      input.off("data", onData);
      input.off("end", onEnd);
      input.pause();
      input.destroy();
    },
  };
}

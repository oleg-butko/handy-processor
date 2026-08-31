import { appendFile } from "node:fs/promises";
import { join } from "node:path";

export const DIAGNOSTIC_LOG_FILE_NAME = "handy-processor.log";

export interface ClientDiagnosticEntry {
  readonly event: "history-export";
  readonly message: string;
  readonly origin: string;
  readonly userAgent: string;
  readonly launcherBridgeDetected: boolean;
  readonly tauriNamespaces: readonly string[];
}

export interface DiagnosticLog {
  readonly path: string;
  appendClientError(entry: ClientDiagnosticEntry): Promise<void>;
}

export class FileDiagnosticLog implements DiagnosticLog {
  readonly path: string;
  private pendingWrite = Promise.resolve();

  constructor(runtimeDirectory: string) {
    this.path = join(runtimeDirectory, DIAGNOSTIC_LOG_FILE_NAME);
  }

  appendClientError(entry: ClientDiagnosticEntry): Promise<void> {
    const line = `${JSON.stringify({
      timestamp: new Date().toISOString(),
      level: "error",
      source: "web-ui",
      ...entry,
    })}\n`;
    const write = this.pendingWrite.then(() =>
      appendFile(this.path, line, "utf8"),
    );
    this.pendingWrite = write.catch(() => undefined);
    return write;
  }
}

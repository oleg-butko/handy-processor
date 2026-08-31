interface TauriGlobal {
  readonly core?: {
    invoke<T>(command: string, arguments_?: Record<string, unknown>): Promise<T>;
  };
}

declare global {
  interface Window {
    readonly __TAURI__?: TauriGlobal;
  }
}

export function isLauncherWebView(): boolean {
  return typeof window.__TAURI__?.core?.invoke === "function";
}

export async function saveHistoryExportInLauncher(): Promise<string> {
  const core = window.__TAURI__?.core;
  if (core === undefined) {
    throw new Error("Launcher bridge недоступен");
  }
  return core.invoke<string>("save_history_export");
}

function describeError(error: unknown): string {
  if (error instanceof Error) {
    return error.stack ?? `${error.name}: ${error.message}`;
  }
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

export async function reportHistoryExportError(
  error: unknown,
): Promise<string | undefined> {
  const tauriGlobal = window.__TAURI__;
  const response = await fetch("/api/diagnostics/client-error", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      event: "history-export",
      message: describeError(error),
      origin: window.location.origin,
      userAgent: navigator.userAgent,
      launcherBridgeDetected: isLauncherWebView(),
      tauriNamespaces:
        tauriGlobal === undefined ? [] : Object.keys(tauriGlobal).sort(),
    }),
  });
  if (!response.ok) return undefined;
  const result = (await response.json()) as { logPath?: unknown };
  return typeof result.logPath === "string" ? result.logPath : undefined;
}

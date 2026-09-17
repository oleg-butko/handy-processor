interface TauriGlobal {
  readonly event?: {
    listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void>;
  };
  readonly core?: {
    invoke<T>(command: string, arguments_?: Record<string, unknown>): Promise<T>;
  };
}

export interface TemplateHotkeySettings {
  readonly enabled: boolean;
  readonly key: string;
  readonly intervalMs: number;
}

export interface TemplateDesktopState {
  readonly supported: boolean;
  readonly settings: TemplateHotkeySettings;
  readonly error: string | null;
  readonly canPaste: boolean;
  readonly activation: number | null;
}

export async function invokeLauncher<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = window.__TAURI__?.core;
  if (core === undefined) throw new Error("Нужен desktop-лаунчер");
  try {
    return await core.invoke<T>(command, args);
  } catch (error) {
    // Use HTTP: a denied native command cannot log through the same IPC bridge.
    // Never include arguments (template text, clipboard content or file paths).
    let logPath: string | undefined;
    try { logPath = await reportClientError("templates", `${command}: ${describeError(error)}`); }
    catch { /* Preserve the original error when diagnostic logging fails. */ }
    throw new Error(logPath === undefined ? describeError(error) : `${describeError(error)}. Подробности записаны в ${logPath}`);
  }
}

export async function listenTemplatesActivation(callback: (id: number) => void): Promise<() => void> {
  const events = window.__TAURI__?.event;
  if (events === undefined) throw new Error("События лаунчера недоступны");
  return events.listen<number>("templates-activated", ({ payload }) => callback(payload));
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
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

export async function reportHistoryExportError(
  error: unknown,
): Promise<string | undefined> {
  return reportClientError("history-export", error);
}

async function reportClientError(
  event: "history-export" | "templates",
  error: unknown,
): Promise<string | undefined> {
  const tauriGlobal = window.__TAURI__;
  const response = await fetch("/api/diagnostics/client-error", {
    method: "POST",
    signal: AbortSignal.timeout(5000),
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      event,
      message: describeError(error).slice(0, 4096),
      origin: window.location.origin.slice(0, 512),
      userAgent: navigator.userAgent.slice(0, 1024),
      launcherBridgeDetected: isLauncherWebView(),
      tauriNamespaces:
        tauriGlobal === undefined ? [] : Object.keys(tauriGlobal).sort().slice(0, 32).map((name) => name.slice(0, 64)),
    }),
  });
  if (!response.ok) return undefined;
  const result = (await response.json()) as { logPath?: unknown };
  return typeof result.logPath === "string" ? result.logPath : undefined;
}

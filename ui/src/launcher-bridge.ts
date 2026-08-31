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

import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { join } from "node:path";
import type { CorrectionOutput, TranscriptionCorrector } from "./correction-types.js";
import { isCorrectionOutput } from "./correction-types.js";

interface Pending {
  resolve(value: CorrectionOutput): void;
  reject(error: Error): void;
  timer: NodeJS.Timeout;
}

export interface CorrectionWorkerOptions {
  readonly executable: string;
  readonly config: string;
  readonly timeoutMs?: number;
  readonly maxPending?: number;
  /** Used by isolated protocol tests, never supplied by HTTP callers. */
  readonly arguments?: readonly string[];
}

export function correctionWorkerOptions(directory: string): CorrectionWorkerOptions {
  return {
    executable: process.env.HANDY_PROCESSOR_CORRECTION_BINARY ?? join(directory,
      "rechprav", "target", "release", "examples", process.platform === "win32" ? "correction_worker.exe" : "correction_worker"),
    config: process.env.HANDY_PROCESSOR_CORRECTION_CONFIG ?? join(directory, "rechprav", "handy-terms.toml"),
  };
}

/** One process; lifecycle callbacks only affect their own generation. */
class CorrectionProcess implements TranscriptionCorrector {
  readonly #options: CorrectionWorkerOptions;
  readonly #pending = new Map<number, Pending>();
  #child: ChildProcessWithoutNullStreams | undefined;
  #ready: Promise<void> | undefined;
  #readyResolve: (() => void) | undefined;
  #readyReject: ((error: Error) => void) | undefined;
  #startupTimer: NodeJS.Timeout | undefined;
  #buffer = "";
  #stderr = "";
  #sequence = 0;
  #waiting = 0;
  #closed = false;
  #profile: string | null = null;
  #error: string | null = null;
  readonly #idleWaiters: (() => void)[] = [];

  constructor(options: CorrectionWorkerOptions) { this.#options = options; }

  get busy(): boolean { return this.#waiting > 0; }

  whenIdle(): Promise<void> {
    if (!this.busy) return Promise.resolve();
    return new Promise(resolve => this.#idleWaiters.push(resolve));
  }

  load(): Promise<void> { return this.#start(); }

  status() {
    return { state: this.#profile ? "ready" : this.#child ? "loading" : "stopped",
      profile: this.#profile, config: this.#options.config, error: this.#error };
  }

  #fail(error: Error): void {
    const child = this.#child;
    this.#child = undefined;
    this.#profile = null;
    this.#ready = undefined;
    this.#buffer = "";
    this.#error = error.message;
    clearTimeout(this.#startupTimer);
    this.#readyReject?.(error);
    this.#readyResolve = undefined;
    this.#readyReject = undefined;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.#pending.clear();
    child?.kill("SIGKILL");
  }

  #start(): Promise<void> {
    if (this.#closed) return Promise.reject(new Error("Корректор остановлен"));
    if (this.#ready) return this.#ready;
    this.#stderr = "";
    this.#ready = new Promise<void>((resolve, reject) => {
      this.#readyResolve = resolve;
      this.#readyReject = reject;
    });
    const ready = this.#ready;
    let child: ChildProcessWithoutNullStreams;
    try {
      child = spawn(this.#options.executable,
        [...(this.#options.arguments ?? []), this.#options.config], { stdio: "pipe", windowsHide: true });
    } catch (error) {
      // spawn also throws synchronously (e.g. empty executable or NUL in args).
      // Reject the shared startup promise instead of leaving it pending forever.
      this.#fail(error instanceof Error ? error : new Error(String(error)));
      return ready;
    }
    this.#child = child;
    this.#startupTimer = setTimeout(() => this.#fail(new Error("Истекло время загрузки корректора")),
      this.#options.timeoutMs ?? 30_000);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (part: string) => {
      if (this.#child === child) this.#stderr = (this.#stderr + part).slice(-2000);
    });
    child.on("error", (error) => { if (this.#child === child) this.#fail(error); });
    child.stdin.on("error", (error) => { if (this.#child === child) this.#fail(error); });
    child.on("exit", () => {
      if (this.#child === child) this.#fail(new Error(`Корректор завершился: ${this.#stderr.trim()}`));
    });
    child.stdout.on("data", (part: string) => {
      if (this.#child !== child) return;
      this.#buffer += part;
      if (Buffer.byteLength(this.#buffer) > 2 * 1024 * 1024) {
        this.#fail(new Error("Ответ корректора превышает ограничение"));
        return;
      }
      let end: number;
      while ((end = this.#buffer.indexOf("\n")) >= 0) {
        const line = this.#buffer.slice(0, end);
        this.#buffer = this.#buffer.slice(end + 1);
        try {
          const reply = JSON.parse(line);
          if (reply.ready === true && reply.protocol === 1 && typeof reply.profile === "string" &&
              reply.profile.length > 0 && reply.profile.length <= 128 && this.#profile === null) {
            this.#profile = reply.profile;
            this.#error = null;
            clearTimeout(this.#startupTimer);
            this.#readyResolve?.();
            this.#readyReject = undefined;
            this.#readyResolve = undefined;
          } else {
            const id = reply.id;
            const pending = this.#pending.get(id);
            if (!pending || !this.#profile || !isCorrectionOutput(reply, this.#profile)) {
              throw new Error("Некорректный ответ корректора");
            }
            clearTimeout(pending.timer);
            this.#pending.delete(id);
            pending.resolve({ text: reply.text, trace: reply.trace });
          }
        } catch (error) {
          this.#fail(error instanceof Error ? error : new Error(String(error)));
          return;
        }
      }
    });
    return ready;
  }

  async analyze(text: string): Promise<CorrectionOutput> {
    if (Buffer.byteLength(text) > 16384) throw new Error("Коррекция ограничена 16 КиБ текста за запрос");
    if (this.#waiting >= (this.#options.maxPending ?? 8)) throw new Error("Очередь корректора заполнена");
    this.#waiting++;
    try {
      await this.#start();
      const child = this.#child;
      if (!child || !this.#profile) throw new Error("Корректор недоступен");
      const id = ++this.#sequence;
      const line = `${JSON.stringify({ id, text })}\n`;
      if (Buffer.byteLength(line) > 65536) throw new Error("Запрос коррекции превышает ограничение протокола");
      return await new Promise<CorrectionOutput>((resolve, reject) => {
        const timer = setTimeout(() => this.#fail(new Error("Истекло время коррекции")), this.#options.timeoutMs ?? 5000);
        this.#pending.set(id, { resolve, reject, timer });
        child.stdin.write(line);
      });
    } finally {
      this.#waiting--;
      if (!this.busy) for (const resolve of this.#idleWaiters.splice(0)) resolve();
    }
  }

  close(): void {
    this.#closed = true;
    this.#fail(new Error("Корректор остановлен"));
  }
}

/** Keep the loaded profile until its replacement is ready; at most two generations. */
export class CorrectionWorker implements TranscriptionCorrector {
  readonly #options: CorrectionWorkerOptions;
  #active: CorrectionProcess;
  #candidate: CorrectionProcess | undefined;
  #retired: CorrectionProcess | undefined;
  #closed = false;
  #reloadError: string | null = null;
  #waiting = 0;

  constructor(options: CorrectionWorkerOptions) {
    this.#options = options;
    this.#active = new CorrectionProcess(options);
  }

  status() {
    const active = this.#active.status();
    return { ...active, reloading: this.#candidate !== undefined,
      error: this.#reloadError ?? active.error };
  }

  async analyze(text: string): Promise<CorrectionOutput> {
    if (this.#closed) throw new Error("Корректор остановлен");
    if (this.#waiting >= (this.#options.maxPending ?? 8)) throw new Error("Очередь корректора заполнена");
    // With no loaded profile yet, share the candidate's bounded startup queue.
    const process = this.#candidate && this.#active.status().state !== "ready"
      ? this.#candidate : this.#active;
    this.#waiting++;
    try { return await process.analyze(text); }
    finally { this.#waiting--; }
  }

  async reload(): Promise<void> {
    if (this.#closed) throw new Error("Корректор остановлен");
    if (this.#candidate || this.#retired || this.#active.busy) {
      throw new Error("Дождитесь завершения текущей коррекции или перезагрузки");
    }
    const candidate = new CorrectionProcess(this.#options);
    this.#candidate = candidate;
    this.#reloadError = null;
    try {
      await candidate.load();
      if (this.#closed) throw new Error("Корректор остановлен");
      const previous = this.#active;
      this.#active = candidate;
      this.#candidate = undefined;
      // Requests accepted during loading finish on their original profile.
      this.#retired = previous;
      void previous.whenIdle().then(() => {
        previous.close();
        if (this.#retired === previous) this.#retired = undefined;
      });
    } catch (error) {
      candidate.close();
      this.#reloadError = error instanceof Error ? error.message : String(error);
      throw error;
    } finally {
      if (this.#candidate === candidate) this.#candidate = undefined;
    }
  }

  close(): void {
    this.#closed = true;
    this.#candidate?.close();
    this.#retired?.close();
    this.#active.close();
  }
}

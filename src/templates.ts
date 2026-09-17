import { open } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";
import { parseTemplates, type TemplateDocument, type TemplateResponse } from "./template-model.js";

const MAX_FILE_BYTES = 2 * 1024 * 1024;

export interface TemplateSettings {
  getTemplatesPath(): string | null;
  setTemplatesPath(path: string | null): void;
}

export class TemplateFile {
  readonly #settings: TemplateSettings;
  readonly #defaultPath: string;
  #lastGood: { path: string; document: TemplateDocument } | undefined;

  constructor(runtimeDirectory: string, settings: TemplateSettings) {
    this.#defaultPath = join(runtimeDirectory, "templates.txt");
    this.#settings = settings;
  }

  async #read(path: string): Promise<TemplateDocument> {
    const file = await open(path, "r");
    try {
      const stat = await file.stat();
      if (!stat.isFile()) throw new Error("Источник должен быть обычным текстовым файлом");
      if (stat.size > MAX_FILE_BYTES) throw new Error("Файл шаблонов превышает 2 МБ");
      // Read at most limit + 1, even if another process grows the file meanwhile.
      const buffer = Buffer.alloc(MAX_FILE_BYTES + 1);
      let length = 0;
      while (length < buffer.length) {
        const { bytesRead } = await file.read(buffer, length, buffer.length - length, null);
        if (bytesRead === 0) break;
        length += bytesRead;
      }
      if (length > MAX_FILE_BYTES) throw new Error("Файл шаблонов превышает 2 МБ");
      const document = parseTemplates(new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, length)));
      if (document.items.length === 0 && document.diagnostics.length > 0) {
        throw new Error(`В файле нет корректных шаблонов. Строка ${document.diagnostics[0]?.line}: нужны три уровня`);
      }
      return document;
    } finally {
      await file.close();
    }
  }

  async load(): Promise<TemplateResponse> {
    const path = this.#settings.getTemplatesPath() ?? this.#defaultPath;
    try {
      const document = await this.#read(path);
      this.#lastGood = { path, document };
      return { path, ...document, stale: false, error: null };
    } catch (error) {
      const document = this.#lastGood?.path === path ? this.#lastGood.document : undefined;
      return {
        path,
        items: document?.items ?? [],
        diagnostics: document?.diagnostics ?? [],
        stale: document !== undefined,
        error: error instanceof Error ? error.message : "Не удалось прочитать файл",
      };
    }
  }

  async select(path: string | null): Promise<TemplateResponse> {
    if (path !== null && (!isAbsolute(path) || path.includes("\0"))) {
      throw new Error("Укажите абсолютный путь к файлу");
    }
    const selected = path === null ? this.#defaultPath : resolve(path);
    const document = await this.#read(selected);
    this.#settings.setTemplatesPath(path === null ? null : selected);
    this.#lastGood = { path: selected, document };
    return { path: selected, ...document, stale: false, error: null };
  }
}

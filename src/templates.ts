import { open } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { parsePredefinedWords, parseTemplates, type TemplateDocument, type TemplateResponse } from "./template-model.js";

const MAX_FILE_BYTES = 2 * 1024 * 1024;

export interface TemplateSettings {
  getTemplatesPath(): string | null;
  setTemplatesPath(path: string | null): void;
}

export class TemplateFile {
  readonly #settings: TemplateSettings;
  readonly #defaultPath: string;
  #revision = 0;
  #lastGood: { path: string; document: TemplateDocument; revision: number } | undefined;

  constructor(runtimeDirectory: string, settings: TemplateSettings) {
    this.#defaultPath = join(runtimeDirectory, "templates.txt");
    this.#settings = settings;
  }

  async #readText(path: string): Promise<string> {
    const file = await open(path, "r");
    try {
      const stat = await file.stat();
      if (!stat.isFile()) throw new Error("Источник должен быть обычным текстовым файлом");
      if (stat.size > MAX_FILE_BYTES) throw new Error("Файл превышает 2 МБ");
      // Read at most limit + 1, even if another process grows the file meanwhile.
      const buffer = Buffer.alloc(MAX_FILE_BYTES + 1);
      let length = 0;
      while (length < buffer.length) {
        const { bytesRead } = await file.read(buffer, length, buffer.length - length, null);
        if (bytesRead === 0) break;
        length += bytesRead;
      }
      if (length > MAX_FILE_BYTES) throw new Error("Файл превышает 2 МБ");
      return new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, length));
    } finally {
      await file.close();
    }
  }

  async #read(path: string): Promise<TemplateDocument> {
    const document = parseTemplates(await this.#readText(path));
    if (document.items.length === 0 && document.diagnostics.length > 0) {
      throw new Error(`В файле нет корректных шаблонов. Строка ${document.diagnostics[0]?.line}: нужны три уровня`);
    }
    return document;
  }

  async #readWords(templatePath: string): Promise<Pick<TemplateResponse, "predefinedWords" | "predefinedWordsError">> {
    const path = join(dirname(templatePath), "predefined-words.txt");
    try {
      return { predefinedWords: parsePredefinedWords(await this.#readText(path)), predefinedWordsError: null };
    } catch (error) {
      const missing = (error as NodeJS.ErrnoException).code === "ENOENT";
      return {
        predefinedWords: [],
        predefinedWordsError: missing ? null : `predefined-words.txt: ${error instanceof Error ? error.message : "Не удалось прочитать файл"}`,
      };
    }
  }

  async load(): Promise<TemplateResponse> {
    const revision = ++this.#revision;
    const path = this.#settings.getTemplatesPath() ?? this.#defaultPath;
    const words = await this.#readWords(path);
    try {
      const document = await this.#read(path);
      // A delayed read must not replace a newer successful read or selection.
      if (revision > (this.#lastGood?.revision ?? 0)) {
        this.#lastGood = { path, document, revision };
      }
      return { path, ...document, ...words, stale: false, error: null };
    } catch (error) {
      const document = this.#lastGood?.path === path ? this.#lastGood.document : undefined;
      return {
        ...words,
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
    const words = await this.#readWords(selected);
    this.#settings.setTemplatesPath(path === null ? null : selected);
    this.#lastGood = { path: selected, document, revision: ++this.#revision };
    return { path: selected, ...document, ...words, stale: false, error: null };
  }
}

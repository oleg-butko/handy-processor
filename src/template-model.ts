/** Shared, environment-independent template parsing and search. */
export interface TemplateRow {
  readonly line: number;
  readonly levels: readonly [string, string, string];
}

export interface TemplateDiagnostic {
  readonly line: number;
  readonly message: string;
}

export interface TemplateDocument {
  readonly items: readonly TemplateRow[];
  readonly diagnostics: readonly TemplateDiagnostic[];
}

export interface TemplateResponse extends TemplateDocument {
  readonly path: string;
  readonly stale: boolean;
  readonly error: string | null;
}

export function parseTemplates(text: string): TemplateDocument {
  const items: TemplateRow[] = [];
  const diagnostics: TemplateDiagnostic[] = [];
  text.replace(/^\uFEFF/u, "").split(/\r?\n/u).forEach((source, index) => {
    if (source.trim() === "") return;
    const levels = source.split(">").map((part) => part.trim());
    if (levels.length !== 3 || levels.some((part) => part === "")) {
      diagnostics.push({
        line: index + 1,
        message: "Нужны три непустых уровня, разделённые двумя знаками >",
      });
      return;
    }
    items.push({ line: index + 1, levels: levels as [string, string, string] });
  });
  return { items, diagnostics };
}

function normalize(text: string): string {
  return text.toLocaleLowerCase("ru-RU").replaceAll("ё", "е");
}

export function templatePrefixes(query: string): readonly string[] {
  return [...new Set(query.split(".").map((part) => normalize(part.trim())).filter(Boolean))];
}

export interface MatchRange { readonly from: number; readonly to: number }

export function templateMatches(text: string, prefixes: readonly string[]): MatchRange[] {
  const ranges: MatchRange[] = [];
  for (const word of text.matchAll(/[\p{L}\p{N}]+/gu)) {
    const normalized = normalize(word[0]);
    const length = Math.max(0, ...prefixes.filter((prefix) => normalized.startsWith(prefix)).map((prefix) => prefix.length));
    if (length > 0) ranges.push({ from: word.index, to: word.index + length });
  }
  return ranges;
}

export function filterTemplates(
  rows: readonly TemplateRow[],
  query: string,
  levels: readonly number[],
): readonly TemplateRow[] {
  const prefixes = templatePrefixes(query);
  return rows.filter((row) => prefixes.every((prefix) => levels.some(
    (level) => templateMatches(row.levels[level] ?? "", [prefix]).length > 0,
  )));
}

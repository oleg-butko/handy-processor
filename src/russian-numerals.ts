import {
  RUSSIAN_NUMERAL_WORDS,
  type RussianNumeralEntry,
} from "./generated/russian-numerals.js";

interface WordToken {
  readonly start: number;
  readonly end: number;
  readonly entry: RussianNumeralEntry | undefined;
}

interface Replacement {
  readonly start: number;
  readonly end: number;
  readonly value: string;
}

const WORD_PATTERN = /\p{L}+/gu;
const WHITESPACE_PATTERN = /^\s+$/u;

function lookupNumeral(word: string): RussianNumeralEntry | undefined {
  return (RUSSIAN_NUMERAL_WORDS as Readonly<Record<string, RussianNumeralEntry>>)[
    word.replaceAll("ё", "е")
  ];
}

function tokenizeWords(text: string): WordToken[] {
  const tokens: WordToken[] = [];

  for (const match of text.matchAll(WORD_PATTERN)) {
    const start = match.index;
    const word = match[0];

    tokens.push({
      start,
      end: start + word.length,
      entry: lookupNumeral(word),
    });
  }

  return tokens;
}

function canAppend(
  entries: readonly RussianNumeralEntry[],
  candidate: RussianNumeralEntry,
): boolean {
  if (entries.length === 0) {
    return true;
  }

  const first = entries[0];
  const previous = entries.at(-1);

  if (first === undefined || previous === undefined || first.value === 0) {
    return false;
  }

  if (candidate.kind === "hundred" || candidate.value === 0) {
    return false;
  }

  if (previous.kind === "hundred") {
    return candidate.kind === "teen" || candidate.kind === "ten" || candidate.kind === "unit";
  }

  return previous.kind === "ten" && candidate.kind === "unit";
}

function parseEntries(entries: readonly RussianNumeralEntry[]): string {
  return entries.reduce((total, entry) => total + entry.value, 0).toString();
}

function findReplacements(text: string, tokens: readonly WordToken[]): Replacement[] {
  const replacements: Replacement[] = [];
  let index = 0;

  while (index < tokens.length) {
    const firstToken = tokens[index];
    if (firstToken?.entry === undefined) {
      index += 1;
      continue;
    }

    const entries: RussianNumeralEntry[] = [firstToken.entry];
    let lastToken = firstToken;
    let nextIndex = index + 1;

    while (nextIndex < tokens.length) {
      const nextToken = tokens[nextIndex];
      if (nextToken?.entry === undefined) {
        break;
      }

      const separator = text.slice(lastToken.end, nextToken.start);
      if (!WHITESPACE_PATTERN.test(separator) || !canAppend(entries, nextToken.entry)) {
        break;
      }

      entries.push(nextToken.entry);
      lastToken = nextToken;
      nextIndex += 1;
    }

    replacements.push({
      start: firstToken.start,
      end: lastToken.end,
      value: parseEntries(entries),
    });
    index = nextIndex;
  }

  return replacements;
}

/**
 * Replace unambiguous Russian cardinal numerals from zero through 999.
 *
 * Input is expected to be lowercase. Punctuation and all non-numeral text are
 * preserved exactly.
 */
export function normalizeRussianNumerals(text: string): string {
  const replacements = findReplacements(text, tokenizeWords(text));
  if (replacements.length === 0) {
    return text;
  }

  let result = "";
  let cursor = 0;

  for (const replacement of replacements) {
    result += text.slice(cursor, replacement.start);
    result += replacement.value;
    cursor = replacement.end;
  }

  return result + text.slice(cursor);
}

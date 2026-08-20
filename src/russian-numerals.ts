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

function parseSmallGroup(entries: readonly RussianNumeralEntry[]): bigint | undefined {
  if (entries.length === 0 || entries.some((entry) => entry.kind === "scale")) {
    return undefined;
  }

  if (entries.length === 1) {
    return BigInt(entries[0]?.value ?? 0);
  }

  const [first, second, third] = entries;
  if (first === undefined || second === undefined || first.value === 0 || second.value === 0) {
    return undefined;
  }

  if (entries.length === 2) {
    const followsHundred =
      first.kind === "hundred" &&
      (second.kind === "teen" || second.kind === "ten" || second.kind === "unit");
    const followsTen = first.kind === "ten" && second.kind === "unit";

    return followsHundred || followsTen
      ? BigInt(first.value + second.value)
      : undefined;
  }

  if (
    entries.length === 3 &&
    first.kind === "hundred" &&
    second.kind === "ten" &&
    third?.kind === "unit" &&
    third.value !== 0
  ) {
    return BigInt(first.value + second.value + third.value);
  }

  return undefined;
}

function parseEntries(entries: readonly RussianNumeralEntry[]): bigint | undefined {
  const hasScale = entries.some((entry) => entry.kind === "scale");
  if (!hasScale) {
    return parseSmallGroup(entries);
  }

  let total = 0n;
  let previousScale: bigint | undefined;
  let groupStart = 0;

  for (let index = 0; index < entries.length; index += 1) {
    const scale = entries[index];
    if (scale?.kind !== "scale") {
      continue;
    }

    const scaleValue = BigInt(scale.value);
    if (previousScale !== undefined && scaleValue >= previousScale) {
      return undefined;
    }

    const groupEntries = entries.slice(groupStart, index);
    if (groupEntries.length === 0 && index > 0) {
      return undefined;
    }
    const groupValue = groupEntries.length === 0 ? 1n : parseSmallGroup(groupEntries);
    if (groupValue === undefined || groupValue === 0n) {
      return undefined;
    }

    total += groupValue * scaleValue;
    previousScale = scaleValue;
    groupStart = index + 1;
  }

  const trailingEntries = entries.slice(groupStart);
  if (trailingEntries.length === 0) {
    return total;
  }

  const trailingValue = parseSmallGroup(trailingEntries);
  return trailingValue === undefined ? undefined : total + trailingValue;
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
    const numeralTokens: WordToken[] = [firstToken];
    let lastToken = firstToken;
    let nextIndex = index + 1;

    while (nextIndex < tokens.length) {
      const nextToken = tokens[nextIndex];
      if (nextToken?.entry === undefined) {
        break;
      }

      const separator = text.slice(lastToken.end, nextToken.start);
      if (!WHITESPACE_PATTERN.test(separator)) {
        break;
      }

      entries.push(nextToken.entry);
      numeralTokens.push(nextToken);
      lastToken = nextToken;
      nextIndex += 1;
    }

    let acceptedLength = entries.length;
    let value = parseEntries(entries);
    while (value === undefined && acceptedLength > 1) {
      acceptedLength -= 1;
      value = parseEntries(entries.slice(0, acceptedLength));
    }

    const acceptedLastToken = numeralTokens[acceptedLength - 1];
    if (value === undefined || acceptedLastToken === undefined) {
      index += 1;
      continue;
    }

    replacements.push({
      start: firstToken.start,
      end: acceptedLastToken.end,
      value: value.toString(),
    });
    index += acceptedLength;
  }

  return replacements;
}

/**
 * Replace supported Russian cardinal numerals using scales through trillion.
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

import {
  RUSSIAN_NUMERAL_WORDS,
  type RussianNumeralEntry,
} from "./generated/russian-numerals.js";

interface WordToken {
  readonly start: number;
  readonly end: number;
  readonly word: string;
  readonly entry: RussianNumeralEntry | undefined;
  readonly match: "exact" | "fuzzy" | undefined;
}

interface Replacement {
  readonly start: number;
  readonly end: number;
  readonly value: string;
}

const WORD_PATTERN = /\p{L}+/gu;
const WHITESPACE_PATTERN = /^\s+$/u;
const MIN_FUZZY_WORD_LENGTH = 5;
const NUMERAL_DICTIONARY = RUSSIAN_NUMERAL_WORDS as Readonly<
  Record<string, RussianNumeralEntry>
>;
const NUMERAL_DICTIONARY_ENTRIES = Object.entries(NUMERAL_DICTIONARY);
const SMALL_GROUP_MAX_TOKENS = 3;
const SCALE_COUNT = new Set(
  NUMERAL_DICTIONARY_ENTRIES
    .filter(([, entry]) => entry.kind === "scale")
    .map(([, entry]) => entry.canonical),
).size;
// A valid span contains at most one three-token group before each scale and
// one final three-token group. Capping the lookahead keeps invalid input O(n).
const NUMERAL_SPAN_MAX_TOKENS =
  SCALE_COUNT * (SMALL_GROUP_MAX_TOKENS + 1) + SMALL_GROUP_MAX_TOKENS;

function lookupNumeral(word: string): RussianNumeralEntry | undefined {
  const normalized = word.toLowerCase().replaceAll("ё", "е");
  return Object.hasOwn(NUMERAL_DICTIONARY, normalized)
    ? NUMERAL_DICTIONARY[normalized]
    : undefined;
}

function tokenizeWords(text: string): WordToken[] {
  const tokens: WordToken[] = [];

  for (const match of text.matchAll(WORD_PATTERN)) {
    const start = match.index;
    const word = match[0];
    const entry = lookupNumeral(word);

    tokens.push({
      start,
      end: start + word.length,
      word,
      entry,
      match: entry === undefined ? undefined : "exact",
    });
  }

  return tokens;
}

function isOneInsertionAway(shorter: string, longer: string): boolean {
  let shorterIndex = 0;
  let longerIndex = 0;
  let skipped = false;

  while (shorterIndex < shorter.length && longerIndex < longer.length) {
    if (shorter[shorterIndex] === longer[longerIndex]) {
      shorterIndex += 1;
      longerIndex += 1;
      continue;
    }

    if (skipped) {
      return false;
    }

    skipped = true;
    longerIndex += 1;
  }

  return true;
}

function isOneEditAway(source: string, target: string): boolean {
  const lengthDifference = source.length - target.length;
  if (Math.abs(lengthDifference) > 1) {
    return false;
  }

  if (lengthDifference === -1) {
    return isOneInsertionAway(source, target);
  }

  if (lengthDifference === 1) {
    return isOneInsertionAway(target, source);
  }

  const mismatches: number[] = [];
  for (let index = 0; index < source.length; index += 1) {
    if (source[index] !== target[index]) {
      mismatches.push(index);
      if (mismatches.length > 2) {
        return false;
      }
    }
  }

  if (mismatches.length === 1) {
    return true;
  }

  if (mismatches.length !== 2) {
    return false;
  }

  const [first, second] = mismatches;
  return (
    first !== undefined &&
    second === first + 1 &&
    source[first] === target[second] &&
    source[second] === target[first]
  );
}

function findUniqueFuzzyNumeral(word: string): RussianNumeralEntry | undefined {
  const normalizedWord = word.toLowerCase().replaceAll("ё", "е");
  if (normalizedWord.length < MIN_FUZZY_WORD_LENGTH) {
    return undefined;
  }

  let result: RussianNumeralEntry | undefined;
  let resultIdentity: string | undefined;

  for (const [candidate, entry] of NUMERAL_DICTIONARY_ENTRIES) {
    if (!isOneEditAway(normalizedWord, candidate)) {
      continue;
    }

    const identity = `${entry.kind}:${entry.value}:${entry.canonical}`;
    if (resultIdentity !== undefined && identity !== resultIdentity) {
      return undefined;
    }

    result = entry;
    resultIdentity = identity;
  }

  return result;
}

function hasExactWhitespaceNeighbor(
  text: string,
  tokens: readonly WordToken[],
  index: number,
): boolean {
  const token = tokens[index];
  if (token === undefined) {
    return false;
  }

  const previous = tokens[index - 1];
  if (
    previous?.match === "exact" &&
    WHITESPACE_PATTERN.test(text.slice(previous.end, token.start))
  ) {
    return true;
  }

  const next = tokens[index + 1];
  return (
    next?.match === "exact" &&
    WHITESPACE_PATTERN.test(text.slice(token.end, next.start))
  );
}

function addFuzzyMatches(text: string, tokens: readonly WordToken[]): WordToken[] {
  return tokens.map((token, index) => {
    if (token.entry !== undefined || !hasExactWhitespaceNeighbor(text, tokens, index)) {
      return token;
    }

    const entry = findUniqueFuzzyNumeral(token.word);
    return entry === undefined ? token : { ...token, entry, match: "fuzzy" };
  });
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

function parseCandidate(
  entries: readonly RussianNumeralEntry[],
  tokens: readonly WordToken[],
): bigint | undefined {
  const value = parseEntries(entries);
  if (value === undefined) {
    return undefined;
  }

  const containsFuzzyMatch = tokens.some((token) => token.match === "fuzzy");
  const containsExactMatch = tokens.some((token) => token.match === "exact");
  const containsAmbiguousForm = tokens.some(
    (token) => token.entry?.ambiguous === true,
  );

  if (containsFuzzyMatch && !containsExactMatch) {
    return undefined;
  }

  return containsAmbiguousForm && tokens.length === 1 ? undefined : value;
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

    while (
      nextIndex < tokens.length &&
      entries.length < NUMERAL_SPAN_MAX_TOKENS
    ) {
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
    let value = parseCandidate(entries, numeralTokens);
    while (value === undefined && acceptedLength > 1) {
      acceptedLength -= 1;
      value = parseCandidate(
        entries.slice(0, acceptedLength),
        numeralTokens.slice(0, acceptedLength),
      );
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
 * Matching is case-insensitive. Punctuation, letter casing outside replaced
 * numeral spans and all other text are preserved exactly.
 */
export function normalizeRussianNumerals(text: string): string {
  const tokens = addFuzzyMatches(text, tokenizeWords(text));
  const replacements = findReplacements(text, tokens);
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

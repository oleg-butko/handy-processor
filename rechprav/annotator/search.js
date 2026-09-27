const WORD = /[\p{L}\p{N}]+/gu;

export function words(text) {
  return (text.normalize("NFC").toLocaleLowerCase("ru-RU").match(WORD) ?? []);
}

export function editDistance(left, right, limit = 2) {
  const a = Array.from(left);
  const b = Array.from(right);
  if (Math.abs(a.length - b.length) > limit) return limit + 1;
  let previousPrevious = null;
  let previous = Array.from({ length: b.length + 1 }, (_, index) => index);
  for (let i = 1; i <= a.length; i += 1) {
    const row = [i];
    let rowMin = i;
    for (let j = 1; j <= b.length; j += 1) {
      let value = Math.min(
        row[j - 1] + 1,
        previous[j] + 1,
        previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        value = Math.min(value, previousPrevious[j - 2] + 1);
      }
      row.push(value);
      rowMin = Math.min(rowMin, value);
    }
    if (rowMin > limit) return limit + 1;
    previousPrevious = previous;
    previous = row;
  }
  return previous[b.length];
}

export function hasChangedCorrection(record) {
  return Array.isArray(record.appliedRules) && record.appliedRules.some((rule) =>
    rule?.id === "recognition.terms-and-phrases" && rule.outcome === "changed" && rule.changed === true
  );
}

export function buildIndex(records) {
  const entries = records.map((record) => ({
    record,
    original: words(record.originalText),
    result: words(record.transformedText),
  }));
  const idToIndex = new Map(entries.map((entry, index) => [entry.record.id, index]));
  const vocabulary = new Map();
  const postings = { original: new Map(), result: new Map() };
  for (let index = 0; index < entries.length; index += 1) {
    for (const scope of ["original", "result"]) {
      for (const word of new Set(entries[index][scope])) {
        const length = Array.from(word).length;
        if (!vocabulary.has(length)) vocabulary.set(length, new Set());
        vocabulary.get(length).add(word);
        if (!postings[scope].has(word)) postings[scope].set(word, []);
        postings[scope].get(word).push(index);
      }
    }
  }
  return { entries, idToIndex, vocabulary, postings };
}

function matchingWords(index, queryWord, distance) {
  if (distance === 0) return [queryWord];
  const matches = [];
  const size = Array.from(queryWord).length;
  for (let length = Math.max(1, size - distance); length <= size + distance; length += 1) {
    for (const candidate of index.vocabulary.get(length) ?? []) {
      if (editDistance(queryWord, candidate, distance) <= distance) matches.push(candidate);
    }
  }
  return matches;
}

function matchSequence(tokens, query, distance) {
  let best = null;
  for (let start = 0; start <= tokens.length - query.length; start += 1) {
    let score = 0;
    for (let offset = 0; offset < query.length; offset += 1) {
      const cost = editDistance(query[offset], tokens[start + offset], distance);
      if (cost > distance) {
        score = Infinity;
        break;
      }
      score += cost;
    }
    if (score !== Infinity && (best === null || score < best.distance)) {
      best = { distance: score, matched: tokens.slice(start, start + query.length).join(" ") };
    }
  }
  return best;
}

export function search(index, options = {}) {
  const query = words(options.query ?? "");
  const distance = Number(options.distance ?? 0);
  const mode = options.mode ?? "original";
  const scopes = mode === "both" ? ["original", "result"] : [mode];
  if (![0, 1, 2].includes(distance)) throw new Error("Недопустимое расстояние поиска");
  if (query.length > 8) throw new Error("Запрос должен содержать не более 8 слов");
  if (query.some((word) => Array.from(word).length < distance + 2)) {
    throw new Error("Для такого расстояния нужны более длинные слова: слишком много случайных совпадений");
  }

  let candidateIds;
  if (query.length === 0) {
    candidateIds = Array.from(index.entries.keys());
  } else {
    const perWord = query.map((word) => {
      const ids = new Set();
      for (const matched of matchingWords(index, word, distance)) {
        for (const scope of scopes) {
          for (const id of index.postings[scope].get(matched) ?? []) ids.add(id);
        }
      }
      return ids;
    });
    perWord.sort((a, b) => a.size - b.size);
    candidateIds = [...(perWord[0] ?? [])].filter((id) => perWord.every((ids) => ids.has(id)));
  }

  const groups = new Map();
  let total = 0;
  for (const id of candidateIds) {
    const entry = index.entries[id];
    if (options.source && options.source !== "all" && entry.record.source !== options.source) continue;
    if (options.changedOnly && !hasChangedCorrection(entry.record)) continue;
    let best = null;
    if (query.length === 0) {
      best = { matched: entry.original.join(" ") || "(пусто)", distance: 0, scope: "original" };
    } else {
      for (const scope of scopes) {
        const match = matchSequence(entry[scope], query, distance);
        if (match && (best === null || match.distance < best.distance)) best = { ...match, scope };
      }
    }
    if (!best) continue;
    total += 1;
    const key = `${best.scope}\u0000${best.matched}`;
    if (!groups.has(key)) groups.set(key, { key, phrase: best.matched, scope: best.scope, distance: best.distance, ids: [] });
    groups.get(key).ids.push(id);
  }
  return {
    total,
    groups: [...groups.values()].sort((a, b) =>
      options.sort === "closest"
        ? a.distance - b.distance || b.ids.length - a.ids.length || a.phrase.localeCompare(b.phrase, "ru")
        : b.ids.length - a.ids.length || a.distance - b.distance || a.phrase.localeCompare(b.phrase, "ru")
    ),
  };
}

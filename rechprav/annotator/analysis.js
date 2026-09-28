function comparePairs(a, b) {
  return b.count - a.count || a.source.localeCompare(b.source, "ru") || a.gold.localeCompare(b.gold, "ru");
}

function topPairs(pairs, limit) {
  const heap = [];
  const worse = (a, b) => comparePairs(a, b) > 0;
  for (const pair of pairs) {
    if (heap.length < limit) {
      heap.push(pair);
      for (let child = heap.length - 1; child > 0;) {
        const parent = Math.floor((child - 1) / 2);
        if (!worse(heap[child], heap[parent])) break;
        [heap[child], heap[parent]] = [heap[parent], heap[child]];
        child = parent;
      }
    } else if (comparePairs(pair, heap[0]) < 0) {
      heap[0] = pair;
      for (let parent = 0;;) {
        const left = parent * 2 + 1;
        if (left >= heap.length) break;
        const right = left + 1;
        const child = right < heap.length && worse(heap[right], heap[left]) ? right : left;
        if (!worse(heap[child], heap[parent])) break;
        [heap[child], heap[parent]] = [heap[parent], heap[child]];
        parent = child;
      }
    }
  }
  return heap.sort(comparePairs);
}

function* allPairs(pairs) {
  for (const targets of pairs.values()) yield* targets.values();
}

export function summarizeAnnotations(index, annotations) {
  const totals = {
    reviewed: 0,
    corrected: 0,
    alreadyCorrect: 0,
    exactFix: 0,
    missed: 0,
    otherResult: 0,
    damagedCorrect: 0,
    synthetic: 0,
    realAsr: 0,
    unconfirmedOrigin: 0,
  };
  const pairs = new Map();
  let pairCount = 0;
  for (const [id, annotation] of annotations) {
    if (annotation?.status !== "reviewed") continue;
    const recordIndex = index.idToIndex.get(id);
    if (recordIndex === undefined) continue;
    const record = index.entries[recordIndex].record;
    totals.reviewed += 1;
    if (annotation.originKind === "synthetic") totals.synthetic += 1;
    else if (annotation.originKind === "real-asr") totals.realAsr += 1;
    else totals.unconfirmedOrigin += 1;
    const gold = annotation.gold;
    if (gold === record.originalText) {
      totals.alreadyCorrect += 1;
      if (record.transformedText !== gold) totals.damagedCorrect += 1;
      continue;
    }
    totals.corrected += 1;
    const outcome = record.transformedText === gold ? "exactFix"
      : record.transformedText === record.originalText ? "missed" : "otherResult";
    totals[outcome] += 1;
    if (!pairs.has(record.originalText)) pairs.set(record.originalText, new Map());
    const targets = pairs.get(record.originalText);
    if (!targets.has(gold)) {
      targets.set(gold, {
        source: record.originalText, gold, count: 0, exactFix: 0, missed: 0, otherResult: 0,
        realAsr: 0, synthetic: 0, unconfirmedOrigin: 0, ids: [],
      });
      pairCount += 1;
    }
    const pair = targets.get(gold);
    pair.count += 1;
    pair[outcome] += 1;
    if (annotation.originKind === "real-asr") pair.realAsr += 1;
    else if (annotation.originKind === "synthetic") pair.synthetic += 1;
    else pair.unconfirmedOrigin += 1;
    pair.ids.push(record.id);
  }
  return {
    totals,
    pairCount,
    pairs: topPairs(allPairs(pairs), 30),
  };
}

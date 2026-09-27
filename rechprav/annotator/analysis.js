export function summarizeAnnotations(records, annotations) {
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
  for (const record of records) {
    const annotation = annotations.get(record.id);
    if (annotation?.status !== "reviewed") continue;
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
    const key = `${record.originalText}\u0000${gold}`;
    if (!pairs.has(key)) pairs.set(key, {
      source: record.originalText, gold, count: 0, exactFix: 0, missed: 0, otherResult: 0, ids: [],
    });
    const pair = pairs.get(key);
    pair.count += 1;
    pair[outcome] += 1;
    pair.ids.push(record.id);
  }
  return {
    totals,
    pairs: [...pairs.values()].sort((a, b) => b.count - a.count || a.source.localeCompare(b.source, "ru")),
  };
}

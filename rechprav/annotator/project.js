export const MAX_FILE_BYTES = 100 * 1024 * 1024;
export const MAX_RECORDS = 100_000;
export const ORIGINS = new Set(["unknown", "real-asr", "synthetic", "not-asr"]);
export const STATUSES = new Set(["pending", "reviewed", "ambiguous"]);

export function defaultAnnotation() {
  return { status: "pending", gold: null, note: "", originKind: "unknown", groupId: "" };
}

function validateRecord(record, line) {
  if (!record || typeof record !== "object" || Array.isArray(record) ||
      !Number.isSafeInteger(record.id) || record.id < 1 ||
      typeof record.originalText !== "string" || typeof record.transformedText !== "string" ||
      !["external", "voice-editor"].includes(record.source) ||
      record.originalText.length > 20_000 || record.transformedText.length > 20_000) {
    throw new Error(`Строка ${line}: неверная запись истории`);
  }
}

function validateAnnotation(annotation, line, version) {
  if (!annotation || typeof annotation !== "object" || Array.isArray(annotation) ||
      !STATUSES.has(annotation.status) ||
      (annotation.gold !== null && typeof annotation.gold !== "string") ||
      (typeof annotation.gold === "string" && annotation.gold.length > 20_000) ||
      (annotation.draftGold !== undefined && typeof annotation.draftGold !== "string") ||
      (typeof annotation.draftGold === "string" && annotation.draftGold.length > 20_000) ||
      typeof annotation.note !== "string" || annotation.note.length > 20_000 ||
      (annotation.status === "reviewed" && annotation.gold === null) ||
      (annotation.status !== "reviewed" && annotation.gold !== null) ||
      (version === 2 && (!ORIGINS.has(annotation.originKind) ||
        typeof annotation.groupId !== "string" || annotation.groupId.length > 120))) {
    throw new Error(`Строка ${line}: неверная разметка`);
  }
}

export function parseJsonl(text, project) {
  const records = [];
  const annotations = new Map();
  const ids = new Set();
  const lines = text.replace(/^\uFEFF/u, "").split(/\r?\n/u);
  for (let i = 0; i < lines.length; i += 1) {
    if (!lines[i].trim()) continue;
    if (records.length >= MAX_RECORDS) throw new Error(`Максимум ${MAX_RECORDS} записей в одном проекте`);
    let item;
    try { item = JSON.parse(lines[i]); } catch { throw new Error(`Строка ${i + 1}: некорректный JSON`); }
    const version = project ? item?.schema_version : null;
    if (project && (![1, 2].includes(version) || !item.history || !item.annotation)) {
      throw new Error(`Строка ${i + 1}: неизвестная версия проекта разметки`);
    }
    const record = project ? item.history : item;
    validateRecord(record, i + 1);
    if (ids.has(record.id)) throw new Error(`Строка ${i + 1}: повтор ID ${record.id}`);
    ids.add(record.id);
    records.push(record);
    if (project) {
      validateAnnotation(item.annotation, i + 1, version);
      annotations.set(record.id, {
        ...defaultAnnotation(),
        ...item.annotation,
        originKind: version === 1 ? "unknown" : item.annotation.originKind,
        groupId: version === 1 ? "" : item.annotation.groupId,
      });
    }
  }
  if (records.length === 0) throw new Error("JSONL-файл не содержит записей");
  return { records, annotations };
}

export function serializeProject(records, annotations) {
  return records.map((record) => JSON.stringify({
    schema_version: 2,
    history: record,
    annotation: annotations.get(record.id) ?? defaultAnnotation(),
  })).join("\n") + "\n";
}

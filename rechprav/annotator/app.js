import { buildIndex, hasChangedCorrection, search } from "./search.js";

const $ = (id) => document.getElementById(id);
const MAX_FILE_BYTES = 100 * 1024 * 1024;
const MAX_RECORDS = 100_000;
const state = {
  index: null,
  annotations: new Map(),
  result: { total: 0, groups: [] },
  selectedGroup: null,
  selectedRecord: null,
  recordPage: 0,
  shownGroups: 100,
  dirty: false,
};

function message(text) { $("message").textContent = text; }
function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
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

function parseJsonl(text, project) {
  const records = [];
  const annotations = new Map();
  const ids = new Set();
  const lines = text.replace(/^\uFEFF/u, "").split(/\r?\n/u);
  for (let i = 0; i < lines.length; i += 1) {
    if (!lines[i].trim()) continue;
    if (records.length >= MAX_RECORDS) throw new Error(`Максимум ${MAX_RECORDS} записей в одном проекте`);
    let item;
    try { item = JSON.parse(lines[i]); } catch { throw new Error(`Строка ${i + 1}: некорректный JSON`); }
    if (project && (item?.schema_version !== 1 || !item.history || !item.annotation)) {
      throw new Error(`Строка ${i + 1}: это не проект разметки версии 1`);
    }
    const record = project ? item.history : item;
    validateRecord(record, i + 1);
    if (ids.has(record.id)) throw new Error(`Строка ${i + 1}: повтор ID ${record.id}`);
    ids.add(record.id);
    records.push(record);
    if (project) {
      const annotation = item.annotation;
      if (!["pending", "reviewed", "ambiguous"].includes(annotation.status) ||
          (annotation.gold !== null && typeof annotation.gold !== "string") ||
          (annotation.draftGold !== undefined && typeof annotation.draftGold !== "string") ||
          typeof annotation.note !== "string" ||
          (annotation.status === "reviewed" && annotation.gold === null) ||
          (annotation.status !== "reviewed" && annotation.gold !== null)) {
        throw new Error(`Строка ${i + 1}: неверная разметка`);
      }
      annotations.set(record.id, annotation);
    }
  }
  return { records, annotations };
}

function annotationFor(record) {
  return state.annotations.get(record.id) ?? { status: "pending", gold: null, note: "" };
}

function selectedGroup() {
  return state.result.groups.find((group) => group.key === state.selectedGroup) ?? null;
}

function selectedEntry() {
  if (!state.index || state.selectedRecord === null) return null;
  const index = state.index.idToIndex.get(state.selectedRecord);
  return index === undefined ? null : state.index.entries[index];
}

function renderSummary() {
  if (!state.index) return;
  let reviewed = 0;
  let corrected = 0;
  let ambiguous = 0;
  for (const [id, annotation] of state.annotations) {
    const index = state.index.idToIndex.get(id);
    if (index === undefined) continue;
    const record = state.index.entries[index].record;
    if (annotation.status === "reviewed") {
      reviewed += 1;
      if (annotation.gold !== record.originalText) corrected += 1;
    }
    if (annotation.status === "ambiguous") ambiguous += 1;
  }
  $("summary").textContent = `${state.index.entries.length} записей · ${reviewed} проверено · ${corrected} ошибок подтверждено · ${ambiguous} неясно · ` +
    `${state.result.total} найдено в ${state.result.groups.length} вариантах`;
}

function renderGroups() {
  const container = $("groups");
  const scrollTop = container.scrollTop;
  container.replaceChildren();
  $("group-count").textContent = String(state.result.groups.length);
  for (const group of state.result.groups.slice(0, state.shownGroups)) {
    const confirmed = group.ids.reduce((count, index) => {
      const record = state.index.entries[index].record;
      const annotation = annotationFor(record);
      return count + Number(annotation.status === "reviewed" && annotation.gold !== record.originalText);
    }, 0);
    const button = element("button", group.key === state.selectedGroup ? "active" : "");
    button.type = "button";
    button.append(element("strong", "", group.phrase));
    button.append(element("small", "", `${group.ids.length} записей · ${confirmed} ошибок подтверждено · расстояние ${group.distance}` +
      (group.scope === "result" ? " · результат" : " · исходник")));
    button.addEventListener("click", () => {
      state.selectedGroup = group.key;
      state.recordPage = 0;
      state.selectedRecord = state.index.entries[group.ids[0]].record.id;
      renderGroups();
      renderRecords();
      renderDetail();
    });
    container.append(button);
  }
  if (state.result.groups.length > state.shownGroups) {
    const more = element("button", "", `Показать ещё (${state.result.groups.length - state.shownGroups})`);
    more.type = "button";
    more.addEventListener("click", () => { state.shownGroups += 100; renderGroups(); });
    container.append(more);
  }
  container.scrollTop = scrollTop;
}

function renderRecords() {
  const container = $("record-list");
  const scrollTop = container.scrollTop;
  container.replaceChildren();
  const group = selectedGroup();
  $("record-count").textContent = group ? String(group.ids.length) : "";
  $("page-label").textContent = group ? `${state.recordPage + 1}/${Math.ceil(group.ids.length / 40)}` : "";
  $("previous-page").disabled = !group || state.recordPage === 0;
  $("next-page").disabled = !group || (state.recordPage + 1) * 40 >= group.ids.length;
  if (!group) return;
  for (const index of group.ids.slice(state.recordPage * 40, (state.recordPage + 1) * 40)) {
    const record = state.index.entries[index].record;
    const annotation = annotationFor(record);
    const label = annotation.status === "reviewed" ? "Проверено" : annotation.status === "ambiguous" ? "Неясно" : "Не проверено";
    const button = element("button", record.id === state.selectedRecord ? "active" : "");
    button.type = "button";
    button.append(element("strong", "", `#${record.id} · ${label}`));
    button.append(element("small", "", record.originalText.slice(0, 160) || "(пустой текст)"));
    button.addEventListener("click", () => { state.selectedRecord = record.id; renderRecords(); renderDetail(); });
    container.append(button);
  }
  container.scrollTop = scrollTop;
}

function renderDetail() {
  const entry = selectedEntry();
  const detail = $("record-detail");
  detail.replaceChildren();
  for (const id of ["gold", "note", "mark-correct", "mark-edited", "mark-ambiguous"]) $(id).disabled = !entry;
  if (!entry) {
    detail.textContent = "Выберите вариант и запись.";
    $("gold").value = "";
    $("note").value = "";
    $("annotation-state").textContent = "";
    return;
  }
  const record = entry.record;
  const annotation = annotationFor(record);
  detail.append(element("small", "", `#${record.id} · ${record.createdAt ?? "дата неизвестна"} · ` +
    (record.source === "external" ? "внешний ввод" : "голосовой редактор") +
    (hasChangedCorrection(record) ? " · корректор изменил текст" : "")));
  detail.append(element("div", "line", `Исходник: ${record.originalText}`));
  detail.append(element("div", "line", `Результат: ${record.transformedText}`));
  if (record.context) {
    const position = record.context.editor?.sentencePosition;
    const labels = { start: "начало предложения", middle: "середина предложения", unknown: "позиция неизвестна" };
    detail.append(element("div", "line", `Сохранённый контекст: ${labels[position] ?? "другой контекст"}`));
  }
  if (Array.isArray(record.appliedRules) && record.appliedRules.length) {
    const disclosure = element("details");
    disclosure.append(element("summary", "", "Применённые правила и диагностика"));
    disclosure.append(element("pre", "diagnostics", JSON.stringify(record.appliedRules, null, 2)));
    detail.append(disclosure);
  }
  $("gold").value = annotation.status === "reviewed" ? annotation.gold ?? "" : annotation.draftGold ?? "";
  $("note").value = annotation.note;
  $("annotation-state").textContent = annotation.status === "reviewed"
    ? "Текст подтверждён человеком." : annotation.status === "ambiguous"
      ? "Смысл неясен; запись не должна входить в оценку." : "Эталон пока не подтверждён.";
}

function render() { renderSummary(); renderGroups(); renderRecords(); renderDetail(); }

async function runSearch() {
  if (!state.index) return;
  message("Поиск…");
  await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
  try {
    state.result = search(state.index, {
      query: $("query").value,
      mode: $("mode").value,
      distance: Number($("distance").value),
      source: $("source").value,
      sort: $("sort").value,
      changedOnly: $("changed-only").checked,
    });
    state.shownGroups = 100;
    state.selectedGroup = state.result.groups[0]?.key ?? null;
    state.selectedRecord = state.result.groups[0]
      ? state.index.entries[state.result.groups[0].ids[0]].record.id : null;
    state.recordPage = 0;
    render();
    message("");
  } catch (error) { message(error.message); }
}

async function importFile(file, project) {
  if (!file) return;
  if (state.dirty && !window.confirm("Несохранённая разметка будет потеряна. Продолжить?")) return;
  if (file.size > MAX_FILE_BYTES) { message("Файл слишком большой для браузерной версии (максимум 100 МиБ)."); return; }
  message("Чтение и индексация файла…");
  await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
  try {
    const parsed = parseJsonl(await file.text(), project);
    const index = buildIndex(parsed.records);
    state.index = index;
    state.annotations = parsed.annotations;
    state.dirty = false;
    $("file-name").textContent = `${file.name} · ${parsed.records.length} записей`;
    $("save-project").disabled = false;
    await runSearch();
  } catch (error) { message(error.message); }
}

function updateAnnotation(update) {
  const entry = selectedEntry();
  if (!entry) return;
  const current = annotationFor(entry.record);
  state.annotations.set(entry.record.id, { ...current, ...update });
  state.dirty = true;
  renderSummary();
  renderGroups();
  renderRecords();
  renderDetail();
}

function downloadProject() {
  if (!state.index) return;
  const lines = state.index.entries.map(({ record }) => JSON.stringify({
    schema_version: 1,
    history: record,
    annotation: annotationFor(record),
  }));
  const blob = new Blob([lines.join("\n") + "\n"], { type: "application/x-ndjson;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `rechprav-annotation-${new Date().toISOString().slice(0, 10)}.jsonl`;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  state.dirty = false;
  message("Проект подготовлен к скачиванию. Проверьте, что файл сохранён.");
}

$("history-file").addEventListener("change", (event) => { void importFile(event.target.files?.[0], false); event.target.value = ""; });
$("project-file").addEventListener("change", (event) => { void importFile(event.target.files?.[0], true); event.target.value = ""; });
$("save-project").addEventListener("click", downloadProject);
$("search-form").addEventListener("submit", (event) => { event.preventDefault(); void runSearch(); });
$("previous-page").addEventListener("click", () => { state.recordPage -= 1; renderRecords(); });
$("next-page").addEventListener("click", () => { state.recordPage += 1; renderRecords(); });
$("gold").addEventListener("input", () => {
  const entry = selectedEntry();
  if (!entry) return;
  const current = annotationFor(entry.record);
  state.annotations.set(entry.record.id, { ...current, status: "pending", gold: null, draftGold: $("gold").value });
  state.dirty = true;
  $("annotation-state").textContent = "Черновик сохранён в памяти; подтвердите правку и скачайте проект.";
});
$("note").addEventListener("input", () => {
  const entry = selectedEntry();
  if (!entry) return;
  state.annotations.set(entry.record.id, { ...annotationFor(entry.record), note: $("note").value });
  state.dirty = true;
});
$("mark-correct").addEventListener("click", () => {
  const entry = selectedEntry();
  if (entry) updateAnnotation({ status: "reviewed", gold: entry.record.originalText, draftGold: "" });
});
$("mark-edited").addEventListener("click", () => {
  const entry = selectedEntry();
  if (!entry) return;
  const gold = $("gold").value;
  if (!gold || gold === entry.record.originalText) {
    message("Для правки укажите непустой текст, отличный от исходника. Если исходник верен, нажмите соответствующую кнопку.");
    return;
  }
  message("");
  updateAnnotation({ status: "reviewed", gold, draftGold: "" });
});
$("mark-ambiguous").addEventListener("click", () => updateAnnotation({ status: "ambiguous", gold: null, draftGold: "" }));
window.addEventListener("beforeunload", (event) => {
  if (!state.dirty) return;
  event.preventDefault();
  event.returnValue = "";
});

import { buildIndex, hasChangedCorrection, search } from "./search.js";
import { defaultAnnotation, MAX_FILE_BYTES, parseJsonl, serializeProject } from "./project.js";

const $ = (id) => document.getElementById(id);
const state = {
  index: null,
  annotations: new Map(),
  result: { total: 0, groups: [] },
  selectedGroup: null,
  selectedRecord: null,
  recordPage: 0,
  shownGroups: 100,
  dirty: false,
  operation: 0,
  searchOperation: 0,
};

function message(text) { $("message").textContent = text; }
function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function annotationFor(record) {
  return state.annotations.get(record.id) ?? defaultAnnotation();
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
  for (const id of ["gold", "note", "origin-kind", "group-id", "apply-group", "mark-correct", "mark-edited", "mark-ambiguous"]) $(id).disabled = !entry;
  if (!entry) {
    detail.textContent = "Выберите вариант и запись.";
    $("gold").value = "";
    $("note").value = "";
    $("origin-kind").value = "unknown";
    $("group-id").value = "";
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
  $("origin-kind").value = annotation.originKind;
  $("group-id").value = annotation.groupId;
  $("annotation-state").textContent = annotation.status === "reviewed"
    ? "Текст подтверждён человеком." : annotation.status === "ambiguous"
      ? "Смысл неясен; запись не должна входить в оценку." : "Эталон пока не подтверждён.";
}

function render() { renderSummary(); renderGroups(); renderRecords(); renderDetail(); }

async function runSearch() {
  if (!state.index) return;
  const index = state.index;
  const operation = state.operation;
  const searchOperation = ++state.searchOperation;
  message("Поиск…");
  await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
  if (operation !== state.operation || searchOperation !== state.searchOperation || index !== state.index) return;
  try {
    state.result = search(index, {
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
  const operation = ++state.operation;
  ++state.searchOperation;
  message("Чтение и индексация файла…");
  await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
  if (operation !== state.operation) return;
  try {
    const parsed = parseJsonl(await file.text(), project);
    if (operation !== state.operation) return;
    const index = buildIndex(parsed.records);
    if (operation !== state.operation) return;
    state.index = index;
    state.annotations = parsed.annotations;
    state.dirty = false;
    $("file-name").textContent = `${file.name} · ${parsed.records.length} записей`;
    $("save-project").disabled = false;
    await runSearch();
  } catch (error) { if (operation === state.operation) message(error.message); }
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
  const blob = new Blob([serializeProject(state.index.entries.map((entry) => entry.record), state.annotations)], { type: "application/x-ndjson;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `rechprav-annotation-${new Date().toISOString().slice(0, 10)}.jsonl`;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  message("Проект подготовлен к скачиванию. Проверьте файл перед закрытием вкладки; предупреждение о несохранённых изменениях останется.");
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
$("origin-kind").addEventListener("change", () => {
  const entry = selectedEntry();
  if (entry) updateAnnotation({ originKind: $("origin-kind").value });
});
$("group-id").addEventListener("input", () => {
  const entry = selectedEntry();
  if (!entry) return;
  state.annotations.set(entry.record.id, { ...annotationFor(entry.record), groupId: $("group-id").value });
  state.dirty = true;
});
$("group-id").addEventListener("blur", () => {
  const entry = selectedEntry();
  if (!entry) return;
  const groupId = $("group-id").value.trim();
  if (groupId === $("group-id").value) return;
  $("group-id").value = groupId;
  state.annotations.set(entry.record.id, { ...annotationFor(entry.record), groupId });
});
$("apply-group").addEventListener("click", () => {
  const group = selectedGroup();
  const groupId = $("group-id").value.trim();
  if (!group || !groupId) { message("Укажите непустое имя группы."); return; }
  if (!window.confirm(`Назначить группу «${groupId}» ${group.ids.length} записям этого варианта?`)) return;
  for (const index of group.ids) {
    const record = state.index.entries[index].record;
    state.annotations.set(record.id, { ...annotationFor(record), groupId });
  }
  state.dirty = true;
  renderDetail();
  message(`Группа «${groupId}» назначена ${group.ids.length} записям; правильный текст не менялся.`);
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

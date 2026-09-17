import { ActionIcon, Alert, Button, Group, NumberInput, Paper, Stack, Switch, Text, TextInput, Tooltip } from "@mantine/core";
import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { filterTemplates, templateMatches, templatePrefixes, type TemplateResponse } from "../../src/template-model.js";
import { invokeLauncher, isLauncherWebView, type TemplateDesktopState, type TemplateHotkeySettings } from "./launcher-bridge.js";

const ALPHABET = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя";

function keyLabel(key: string): string {
  return ({ Win: "Win × 2", MetaLeft: "Левый Win × 2", MetaRight: "Правый Win × 2", ControlLeft: "Левый Ctrl × 2", ControlRight: "Правый Ctrl × 2", AltLeft: "Левый Alt × 2", AltRight: "Правый Alt × 2", ShiftLeft: "Левый Shift × 2", ShiftRight: "Правый Shift × 2", Space: "Пробел × 2" } as Record<string, string>)[key] ?? `${key.replace(/^Key/u, "")} × 2`;
}

function Highlight({ text, prefixes }: { text: string; prefixes: readonly string[] }) {
  let cursor = 0;
  const parts = templateMatches(text, prefixes).map(({ from, to }) => {
    const before = text.slice(cursor, from);
    cursor = to;
    return <Fragment key={from}>{before}<mark>{text.slice(from, to)}</mark></Fragment>;
  });
  return <>{parts}{text.slice(cursor)}</>;
}

function ActionGlyph({ paste }: { paste?: boolean }) {
  return <svg aria-hidden="true" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    {paste ? <><path d="M9 5H5v15h14v-7M9 3h6v4H9zM14 11h8m-3-3 3 3-3 3" /></> : <><rect x="8" y="8" width="12" height="13" rx="2" /><path d="M16 8V3H3v13h5" /></>}
  </svg>;
}

export function TemplatesPage({ active, activation }: { active: boolean; activation: number }) {
  const [document, setDocument] = useState<TemplateResponse>();
  const [query, setQuery] = useState("");
  const [levels, setLevels] = useState([0, 1, 2]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [desktop, setDesktop] = useState<TemplateDesktopState>();
  const [settings, setSettings] = useState<TemplateHotkeySettings>();
  const [saving, setSaving] = useState(false);
  const [recording, setRecording] = useState(false);
  const [acting, setActing] = useState(false);
  const actionRef = useRef(false);
  const requestRef = useRef(0);
  const changingSourceRef = useRef(false);
  const input = useRef<HTMLInputElement>(null);
  const launcher = isLauncherWebView();

  const load = useCallback(async () => {
    if (changingSourceRef.current) return;
    const id = ++requestRef.current;
    setLoading(true);
    setError(undefined);
    try {
      const response = await fetch("/api/templates");
      if (!response.ok) throw new Error(`Сервер вернул HTTP ${response.status}`);
      const result = await response.json() as TemplateResponse;
      if (id === requestRef.current) setDocument(result);
    } catch (failure) {
      if (id === requestRef.current) setError(String(failure));
    } finally {
      if (id === requestRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => { if (active) void load(); }, [active, load]);
  useEffect(() => {
    if (!active) return;
    input.current?.focus();
    input.current?.select();
    if (launcher) {
      let cancelled = false;
      void invokeLauncher<TemplateDesktopState>("templates_desktop_state").then((state) => {
        if (!cancelled) { setDesktop(state); setSettings(state.settings); }
      }).catch((failure) => { if (!cancelled) setError(String(failure)); });
      return () => { cancelled = true; };
    }
  }, [active, activation, launcher]);

  useEffect(() => {
    if (!recording) return;
    const cancel = () => { void invokeLauncher("cancel_template_recording").catch(() => {}); };
    if (!active) { cancel(); return; }
    window.addEventListener("blur", cancel);
    window.addEventListener("pagehide", cancel);
    return () => {
      window.removeEventListener("blur", cancel);
      window.removeEventListener("pagehide", cancel);
      cancel();
    };
  }, [active, recording]);

  const chooseFile = async (reset = false) => {
    if (changingSourceRef.current) return;
    changingSourceRef.current = true;
    setError(undefined);
    setLoading(true);
    ++requestRef.current;
    try {
      const path = reset ? null : await invokeLauncher<string | null>("choose_templates_file");
      if (!reset && path === null) return;
      const response = await fetch("/api/templates", { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ path }) });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? `HTTP ${response.status}`);
      setDocument(result as TemplateResponse);
    } catch (failure) { setError(String(failure)); }
    finally { changingSourceRef.current = false; setLoading(false); }
  };

  const saveSettings = async () => {
    if (!settings) return;
    setSaving(true);
    setError(undefined);
    try {
      const state = await invokeLauncher<TemplateDesktopState>("set_template_hotkey", { settings });
      setDesktop(state); setSettings(state.settings); setNotice("Настройки хоткея сохранены");
    } catch (failure) { setError(String(failure)); }
    finally { setSaving(false); }
  };

  const record = async () => {
    setRecording(true); setError(undefined);
    try {
      const key = await invokeLauncher<string>("record_template_key");
      setSettings((current) => current ? { ...current, key } : current);
    } catch (failure) { setError(String(failure)); }
    finally { setRecording(false); }
  };

  const editQuery = (text: string, erase = false) => {
    const start = input.current?.selectionStart ?? query.length;
    const end = input.current?.selectionEnd ?? start;
    const from = erase && start === end ? Math.max(0, start - 1) : start;
    setQuery(query.slice(0, from) + text + query.slice(end));
    requestAnimationFrame(() => {
      input.current?.focus(); input.current?.setSelectionRange(from + text.length, from + text.length);
    });
  };

  const act = async (text: string, paste: boolean) => {
    if (actionRef.current) return;
    actionRef.current = true; setActing(true); setError(undefined); setNotice(undefined);
    try {
      if (paste) {
        const result = await invokeLauncher<{ pasted: boolean; message: string }>("paste_template", { text });
        setNotice(result.message);
      } else {
        if (launcher) await invokeLauncher("copy_template", { text });
        else await navigator.clipboard.writeText(text);
        setNotice("Третий уровень скопирован");
      }
    } catch (failure) { setError(String(failure)); }
    finally { actionRef.current = false; setActing(false); }
  };

  const addWord = (word: string) => {
    const prefix = query.trimEnd();
    const next = `${prefix}${prefix && !prefix.endsWith(".") ? "." : ""}${word}.`;
    setQuery(next);
    setNotice(undefined);
    requestAnimationFrame(() => {
      input.current?.focus(); input.current?.setSelectionRange(next.length, next.length);
    });
  };

  const prefixes = useMemo(() => templatePrefixes(query), [query]);
  const rows = useMemo(() => filterTemplates(document?.items ?? [], query, levels), [document, query, levels]);

  return <Stack pt="md" gap="sm" className="templates-page">
    <Group justify="space-between" align="flex-start">
      <Group gap="xs">
        <Text size="sm">Уровни:</Text>
        {[0, 1, 2].map((level) => <Button key={level} size="xs" aria-label={`Уровень ${level + 1}`} aria-pressed={levels.includes(level)} variant={levels.includes(level) ? "filled" : "default"} onClick={() => setLevels((current) => current.includes(level) ? current.length > 1 ? current.filter((item) => item !== level) : current : [...current, level].sort())}>{level + 1}</Button>)}
      </Group>
      <details className="templates-hotkey" onToggle={(event) => {
        if (!event.currentTarget.open && recording) void invokeLauncher("cancel_template_recording").catch((failure) => setError(String(failure)));
      }}>
        <summary>Хоткей{desktop?.supported ? `: ${desktop.settings.enabled ? `${keyLabel(desktop.settings.key)}, ${desktop.settings.intervalMs} мс` : "выключен"}` : ""}</summary>
        <Paper withBorder p="sm" mt="xs" radius="md" className="hp-surface">
        {desktop?.supported && settings ? <Stack gap="xs">
          <Switch label="Двойное нажатие" checked={settings.enabled} disabled={saving || recording} onChange={(event) => setSettings({ ...settings, enabled: event.currentTarget.checked })} />
          <Group gap="xs" align="end">
            <Button size="xs" variant="light" disabled={saving} loading={recording} onClick={() => void record()}>{recording ? "Нажмите клавишу…" : keyLabel(settings.key)}</Button>
            <NumberInput label="Интервал, мс" aria-label="Интервал двойного нажатия" size="xs" min={50} max={2000} step={50} allowDecimal={false} value={settings.intervalMs} w={115} disabled={saving || recording} onChange={(value) => setSettings({ ...settings, intervalMs: Number(value) })} />
            <Button size="xs" loading={saving} disabled={recording || settings.intervalMs < 50 || settings.intervalMs > 2000} onClick={() => void saveSettings()}>Сохранить</Button>
          </Group>
          <Text size="xs" c="dimmed">{settings.key === "Win" || settings.key.startsWith("Meta") ? "Одиночный Win не открывает «Пуск» при включённом хоткее." : "Двойное нажатие одной клавиши, без сочетаний. Обычная клавиша при включённом хоткее перехватывается."}</Text>
          {recording ? <Button size="xs" variant="subtle" onClick={() => void invokeLauncher("cancel_template_recording").catch((failure) => setError(String(failure)))}>Отмена записи</Button> : null}
        </Stack> : <Text size="xs" c="dimmed">Глобальный хоткей и вставка доступны в Windows-лаунчере.</Text>}
        </Paper>
      </details>
    </Group>
    {desktop?.error ? <Alert color="red">{desktop.error}</Alert> : null}
    <TextInput ref={input} label="Фильтр" placeholder="сл.др — слова начинаются на «сл» и «др»" value={query} onChange={(event) => setQuery(event.currentTarget.value)} />
    {document?.predefinedWords?.length ? <Group gap={4} aria-label="Готовые слова" role="group">
      {document.predefinedWords.map((word) => <Button key={word} size="compact-sm" variant="light" onMouseDown={(event) => event.preventDefault()} onClick={() => addWord(word)}>{word}</Button>)}
    </Group> : null}
    {document?.predefinedWordsError ? <Alert color="yellow">{document.predefinedWordsError}</Alert> : null}
    <Group gap={4} className="templates-alphabet">
      {[...ALPHABET, "."].map((letter) => <Button key={letter} size="compact-sm" variant="default" aria-label={letter === "." ? "Следующее слово" : `Буква ${letter}`} onMouseDown={(event) => event.preventDefault()} onClick={() => editQuery(letter)}>{letter}</Button>)}
      <Button size="compact-sm" variant="light" aria-label="Удалить букву" onMouseDown={(event) => event.preventDefault()} onClick={() => editQuery("", true)}>⌫</Button>
      <Button size="compact-sm" variant="subtle" onClick={() => { setQuery(""); input.current?.focus(); }}>Очистить</Button>
    </Group>
    <Group justify="space-between" gap="xs">
      <Text size="xs" c="dimmed" className="templates-path">{document?.path}</Text>
      <Group gap="xs">
        {launcher ? <Button size="compact-xs" variant="subtle" disabled={loading} onClick={() => void chooseFile()}>Выбрать файл</Button> : null}
        <Button size="compact-xs" variant="subtle" disabled={loading} onClick={() => void chooseFile(true)}>Файл по умолчанию</Button>
        <Button size="compact-xs" variant="light" loading={loading} onClick={() => void load()}>Обновить</Button>
      </Group>
    </Group>
    {error ? <Alert color="red">{error}</Alert> : null}
    {document?.error ? <Alert color="yellow" title={document.stale ? "Показана последняя загруженная версия" : "Не удалось открыть файл"}>{document.error}<Text size="sm">Создайте файл с тремя уровнями в каждой строке: категория &gt; тема &gt; текст.</Text></Alert> : null}
    {document?.diagnostics.length ? <Alert color="yellow">{document.diagnostics.map((item) => <div key={item.line}>Строка {item.line}: {item.message}</div>)}</Alert> : null}
    <Text size="xs" c="dimmed" role="status">{notice ?? `Найдено: ${rows.length} из ${document?.items.length ?? 0}`}</Text>
    <Stack gap={6}>
      {rows.map((row) => <Paper key={row.line} withBorder p="xs" radius="sm" className="hp-surface template-row">
        <div className="template-levels">{levels.map((level, index) => <Fragment key={level}>{index > 0 ? <span className="template-separator"> &gt; </span> : null}<span data-level={level + 1}><Highlight text={row.levels[level] ?? ""} prefixes={prefixes} /></span></Fragment>)}</div>
        <Group gap={4} wrap="nowrap">
          <Tooltip label="Скопировать третий уровень"><ActionIcon variant="subtle" aria-label={`Скопировать строку ${row.line}`} disabled={acting} onClick={() => void act(row.levels[2], false)}><ActionGlyph /></ActionIcon></Tooltip>
          <Tooltip label={desktop?.canPaste ? "Вставить в исходное окно" : "Для вставки вызовите Шаблоны хоткеем из другого приложения"}><ActionIcon variant="light" aria-label={`Вставить строку ${row.line}`} disabled={acting || !desktop?.supported || !desktop.canPaste} onClick={() => void act(row.levels[2], true)}><ActionGlyph paste /></ActionIcon></Tooltip>
        </Group>
      </Paper>)}
      {document && rows.length === 0 && !loading && !document.error ? <Text c="dimmed" ta="center" py="xl">Подходящих шаблонов нет</Text> : null}
    </Stack>
  </Stack>;
}

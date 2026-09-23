import { Alert, Button, Group, Stack, Text } from "@mantine/core";
import { useCallback, useEffect, useRef, useState } from "react";

interface Status { state: string; config: string | null; error: string | null; profile: string | null; reloading?: boolean }

export function CorrectionStatus({ enabled }: { readonly enabled: boolean }) {
  const [status, setStatus] = useState<Status>();
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState(false);
  const sequence = useRef(0);
  const reloading = useRef(false);
  const refresh = useCallback(() => {
    if (reloading.current) return;
    const id = ++sequence.current;
    const controller = new AbortController();
    fetch("/api/correction", { signal: controller.signal }).then(async (response) => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const value = await response.json() as Status;
      if (id === sequence.current && !controller.signal.aborted) {
        setStatus(value); setError(undefined);
      }
    }).catch((e: Error) => {
      if (id === sequence.current && !controller.signal.aborted) setError(e.message);
    });
    return () => controller.abort();
  }, []);
  useEffect(refresh, [enabled, refresh]);
  useEffect(() => () => { sequence.current++; }, []);
  async function reload() {
    if (reloading.current) return;
    reloading.current = true;
    const id = ++sequence.current;
    setPending(true); setError(undefined);
    try {
      const response = await fetch("/api/correction/reload", {
        method: "POST", headers: { "content-type": "application/json" }, body: "{}",
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? `HTTP ${response.status}`);
      if (id === sequence.current) setStatus(body as Status);
    } catch (e) {
      if (id === sequence.current) setError(e instanceof Error ? e.message : String(e));
      // Failed replacement may still leave a working profile. Display its real state.
      try {
        const response = await fetch("/api/correction");
        if (response.ok) {
          const value = await response.json() as Status;
          if (id === sequence.current) setStatus(value);
        }
      } catch { /* Keep the original reload error. */ }
    } finally {
      reloading.current = false;
      if (id === sequence.current) setPending(false);
    }
  }
  return <details className="correction-settings" onToggle={event => {
    if (event.currentTarget.open) refresh();
  }}>
    <summary>Словарь терминов и настройки</summary>
    <Stack gap="xs" mt="xs">
      <Text size="sm">Варианты слов, формы, контекст, веса и наборы задаются в файле профиля.
        После редактирования загрузите его заново. Прошлая история сохранит прежние оценки.</Text>
      {status?.config ? <Text size="xs" className="correction-path">{status.config}</Text> : null}
      <Group><Text size="sm">{status?.reloading ? "Загрузка нового профиля" : status?.state === "ready" ? "Профиль загружен" : status?.state === "loading" ? "Загрузка профиля" : "Профиль загрузится при первом запросе"}</Text>
        <Button size="xs" variant="light" loading={pending} onClick={() => void reload()}>Загрузить профиль заново</Button></Group>
      {error || status?.error ? <Alert color="orange" title={status?.state === "ready" ? "Новый профиль не загружен; прежний продолжает работать" : "Коррекция недоступна"}>{error ?? status?.error}</Alert> : null}
    </Stack>
  </details>;
}

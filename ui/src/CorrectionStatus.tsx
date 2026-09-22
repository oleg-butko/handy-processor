import { Alert, Button, Group, Stack, Text } from "@mantine/core";
import { useEffect, useState } from "react";

interface Status { state: string; config: string | null; error: string | null; profile: string | null }

export function CorrectionStatus({ enabled }: { readonly enabled: boolean }) {
  const [status, setStatus] = useState<Status>();
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    fetch("/api/correction", { signal: controller.signal }).then(async (response) => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      setStatus(await response.json() as Status);
    }).catch((e: Error) => { if (!controller.signal.aborted) setError(e.message); });
    return () => controller.abort();
  }, [enabled]);
  async function reload() {
    setPending(true); setError(undefined);
    try {
      const response = await fetch("/api/correction/reload", {
        method: "POST", headers: { "content-type": "application/json" }, body: "{}",
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? `HTTP ${response.status}`);
      setStatus(body as Status);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setPending(false); }
  }
  return <details className="correction-settings">
    <summary>Словарь терминов и настройки</summary>
    <Stack gap="xs" mt="xs">
      <Text size="sm">Варианты слов, формы, контекст, веса и наборы задаются в файле профиля.
        После редактирования загрузите его заново. Прошлая история сохранит прежние оценки.</Text>
      {status?.config ? <Text size="xs" className="correction-path">{status.config}</Text> : null}
      <Group><Text size="sm">{status?.state === "ready" ? "Профиль загружен" : status?.state === "loading" ? "Загрузка профиля" : "Профиль загрузится при первом запросе"}</Text>
        <Button size="xs" variant="light" loading={pending} onClick={() => void reload()}>Загрузить профиль заново</Button></Group>
      {error || status?.error ? <Alert color="orange" title="Коррекция недоступна">{error ?? status?.error}</Alert> : null}
    </Stack>
  </details>;
}

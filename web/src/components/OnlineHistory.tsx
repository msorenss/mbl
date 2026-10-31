import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { Button } from "./ui/Button";
export function OnlineHistory(props: {
  onPlay: (source: components["schemas"]["OnlineSource"]) => void;
}) {
  const { t } = useI18n();
  const [rows, setRows] = createSignal<components["schemas"]["OnlineHistoryEntry"][]>([]);
  const abort = new AbortController();
  const deadline = setTimeout(() => abort.abort(), 10000);
  onCleanup(() => {
    clearTimeout(deadline);
    abort.abort();
  });
  onMount(async () => {
    try {
      const res = await client.GET("/api/v1/online/history", { signal: abort.signal });
      if (!abort.signal.aborted && Array.isArray(res.data)) setRows(res.data);
    } catch {
    } finally {
      clearTimeout(deadline);
    }
  });
  return (
    <Show when={rows().length}>
      <section aria-label={t("onlineRecent")}>
        <h3>{t("onlineRecent")}</h3>
        <For each={rows()}>
          {(row) => (
            <p>
              <Button type="button" variant="secondary" onClick={() => props.onPlay(row.source)}>
                {row.source.title} · {Math.floor(row.position / 60)}:
                {String(Math.floor(row.position % 60)).padStart(2, "0")}
              </Button>
            </p>
          )}
        </For>
      </section>
    </Show>
  );
}

import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { Button } from "./ui/Button";

type Feed = components["schemas"]["PluginChannelFeed"];
type Item = components["schemas"]["PluginItem"];

/** Catalogue browsing only. Built-in demo providers do not have a playback transport. */
export function PluginFeedBrowser(props: { pluginId: string }) {
  const { t } = useI18n();
  const [feeds, setFeeds] = createSignal<Feed[]>([]);
  const [activeFeed, setActiveFeed] = createSignal<string>();
  const [items, setItems] = createSignal<Item[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");
  let version = 0;
  let request: AbortController | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;
  let retryFeed: string | undefined;

  const load = async (feedId?: string) => {
    const current = ++version;
    request?.abort();
    clearTimeout(deadline);
    const abort = new AbortController();
    request = abort;
    retryFeed = feedId;
    let timedOut = false;
    deadline = setTimeout(() => {
      timedOut = true;
      abort.abort();
    }, 10000);
    setError("");
    setLoading(true);
    setItems([]);
    if (feedId) setActiveFeed(feedId);
    try {
      const result = feedId
        ? await client.GET("/api/v1/plugins/{id}/channels/{feedId}", {
            params: { path: { id: props.pluginId, feedId } },
            signal: abort.signal,
          })
        : await client.GET("/api/v1/plugins/{id}/channels", {
            params: { path: { id: props.pluginId } },
            signal: abort.signal,
          });
      if (current !== version) return;
      if (timedOut || result.error || !result.data)
        throw new Error(apiErrorMessage(result.error, t("catalogueFailed")));
      if (feedId) setItems(result.data as Item[]);
      else {
        const loaded = result.data as Feed[];
        setFeeds(loaded);
        setActiveFeed(loaded[0]?.id);
        setItems(loaded[0]?.items || []);
      }
    } catch (cause) {
      if (current === version)
        setError(timedOut ? t("catalogueFailed") : apiErrorMessage(cause, t("catalogueFailed")));
    } finally {
      if (current === version) {
        clearTimeout(deadline);
        setLoading(false);
      }
    }
  };
  onMount(() => void load());
  onCleanup(() => {
    version++;
    request?.abort();
    clearTimeout(deadline);
  });

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
      <p role="note">{t("pluginDemoHelp")}</p>
      <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
        <For each={feeds()}>
          {(feed) => (
            <Button
              type="button"
              aria-pressed={activeFeed() === feed.id}
              variant={activeFeed() === feed.id ? "primary" : "secondary"}
              onClick={() => void load(feed.id)}
            >
              {feed.title}
            </Button>
          )}
        </For>
      </div>
      <Show when={loading()}>
        <p role="status">{t("catalogueLoading")}</p>
      </Show>
      <Show when={error()}>
        <div role="alert">
          <p>{error()}</p>
          <Button type="button" variant="secondary" onClick={() => void load(retryFeed)}>
            {t("retry")}
          </Button>
        </div>
      </Show>
      <Show when={!loading() && !error() && items().length === 0}>
        <p>{t("catalogueEmpty")}</p>
      </Show>
      <div
        style={{
          "max-height": "360px",
          "overflow-y": "auto",
          display: "flex",
          "flex-direction": "column",
          gap: "8px",
        }}
      >
        <For each={items()}>
          {(item) => (
            <div
              style={{
                display: "flex",
                "flex-wrap": "wrap",
                "align-items": "center",
                "justify-content": "space-between",
                gap: "12px",
                padding: "8px 12px",
                background: "var(--bg-base)",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
              }}
            >
              <div style={{ "min-width": "0", "overflow-wrap": "anywhere" }}>
                <strong>{item.title}</strong>
                <Show when={item.durationMs}>
                  <div>{Math.floor((item.durationMs || 0) / 60000)} min</div>
                </Show>
              </div>
              <Button type="button" variant="secondary" disabled>
                {t("pluginDemoUnavailable")}
              </Button>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

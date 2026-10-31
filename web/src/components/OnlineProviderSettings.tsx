import { createSignal, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import { useI18n } from "../i18n";
import { Button } from "./ui/Button";

export function OnlineProviderSettings() {
  const { t } = useI18n();
  const [provider, setProvider] = createSignal("mwmbl");
  const [url, setUrl] = createSignal("");
  const [config, setConfig] = createSignal<{
    enabled: boolean;
    settings: Record<string, unknown>;
  }>();
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal("");
  let disposed = false;
  let abort = new AbortController();
  const deadline = setTimeout(() => abort.abort(), 10000);
  onCleanup(() => {
    disposed = true;
    clearTimeout(deadline);
    abort.abort();
  });
  onMount(async () => {
    try {
      const res = await client.GET("/api/v1/plugins/{id}", {
        params: { path: { id: "webdrama" } },
        signal: abort.signal,
      });
      if (disposed) return;
      if (!res.data) throw new Error(apiErrorMessage(res.error, t("onlineSearchError")));
      const settings = (res.data.settings || {}) as Record<string, unknown>;
      setConfig({ enabled: res.data.enabled, settings });
      setProvider(settings.searchProvider === "searxng" ? "searxng" : "mwmbl");
      setUrl(typeof settings.searxngUrl === "string" ? settings.searxngUrl : "");
    } catch (error) {
      if (!disposed) setMessage(apiErrorMessage(error, t("onlineSearchError")));
    } finally {
      clearTimeout(deadline);
    }
  });
  const save = async () => {
    if (busy() || !config()) return;
    abort = new AbortController();
    setBusy(true);
    setMessage("");
    const timer = setTimeout(() => abort.abort(), 10000);
    try {
      const current = await client.GET("/api/v1/plugins/{id}", {
        params: { path: { id: "webdrama" } },
        signal: abort.signal,
      });
      if (disposed) return;
      if (!current.data) throw new Error(apiErrorMessage(current.error, t("onlineSearchError")));
      const res = await client.PATCH("/api/v1/plugins/{id}", {
        params: { path: { id: "webdrama" } },
        signal: abort.signal,
        body: {
          enabled: current.data.enabled,
          settings: {
            ...(current.data.settings as Record<string, unknown>),
            searchProvider: provider(),
            searxngUrl: url().trim(),
          },
        },
      });
      if (disposed) return;
      if (!res.data) throw new Error(apiErrorMessage(res.error, t("onlineSearchError")));
      setMessage(t("onlineProviderSaved"));
    } catch (error) {
      if (!disposed) setMessage(apiErrorMessage(error, t("onlineSearchError")));
    } finally {
      clearTimeout(timer);
      if (!disposed) setBusy(false);
    }
  };
  return (
    <details>
      <summary>{t("onlineProviderSettings")}</summary>
      <p>{t("onlineProviderHelp")}</p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
        style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
      >
        <label for="online-provider">{t("onlineProvider")}</label>
        <select
          id="online-provider"
          value={provider()}
          disabled={!config() || busy()}
          onChange={(e) => setProvider(e.currentTarget.value)}
        >
          <option value="mwmbl">Mwmbl</option>
          <option value="searxng">SearXNG</option>
        </select>
        <Show when={provider() === "searxng"}>
          <label for="online-searx-url">SearXNG HTTPS URL</label>
          <input
            id="online-searx-url"
            type="url"
            required
            value={url()}
            disabled={busy()}
            onInput={(e) => setUrl(e.currentTarget.value)}
            placeholder="https://search.example.org"
          />
        </Show>
        <Button type="submit" disabled={!config() || busy()}>
          {t("onlineProviderSave")}
        </Button>
      </form>
      <Show when={message()}>
        <p role="status">{message()}</p>
      </Show>
    </details>
  );
}

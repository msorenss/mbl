import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useTheme } from "../context/ThemeContext";
import { Badge, Button, Dialog } from "./ui";

type Hit = components["schemas"]["SubtitleSearchHit"];
export type DownloadedTrack = components["schemas"]["DownloadedSubtitleTrack"];

/** Languages offered first; any two-letter code works on the server. */
const LANGUAGES: { code: string; label: string }[] = [
  { code: "sv", label: "Svenska" },
  { code: "en", label: "English" },
  { code: "no", label: "Norsk" },
  { code: "da", label: "Dansk" },
  { code: "fi", label: "Suomi" },
  { code: "de", label: "Deutsch" },
  { code: "fr", label: "Français" },
  { code: "es", label: "Español" },
  { code: "it", label: "Italiano" },
  { code: "nl", label: "Nederlands" },
  { code: "pl", label: "Polski" },
  { code: "pt", label: "Português" },
  { code: "tr", label: "Türkçe" },
  { code: "ru", label: "Русский" },
  { code: "ar", label: "العربية" },
  { code: "zh", label: "中文" },
  { code: "ja", label: "日本語" },
  { code: "ko", label: "한국어" },
  { code: "tl", label: "Tagalog" },
];

/** The server's error text without its prefix ("validation: …"). */
function reason(error: unknown, fallback: string): string {
  const text =
    (error as { message?: string; error?: string } | undefined)?.message ??
    (error as { error?: string } | undefined)?.error;
  return typeof text === "string" && text ? text.replace(/^[a-z_]+:\s*/, "") : fallback;
}

/**
 * Find a subtitle on OpenSubtitles and save it next to the video. It is a track at once.
 */
export default function SubtitleSearch(props: {
  itemId: number;
  title: string;
  onClose: () => void;
  onDownloaded?: (track: DownloadedTrack) => void;
}) {
  const { locale } = useTheme();
  const start = LANGUAGES.some((l) => l.code === locale()) ? locale() : "en";
  const [language, setLanguage] = createSignal<string>(start);
  const [hits, setHits] = createSignal<Hit[] | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");

  const search = async () => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.GET("/api/v1/items/{id}/subtitles/search", {
        params: { path: { id: props.itemId }, query: { language: language() } },
      });
      if (!result.data) throw new Error(reason(result.error, "Could not search for subtitles."));
      setHits(result.data);
    } catch (e) {
      setHits(null);
      setError(e instanceof Error ? e.message : "Could not search for subtitles.");
    } finally {
      setBusy(false);
    }
  };

  const download = async (hit: Hit) => {
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/items/{id}/subtitles/download", {
        params: { path: { id: props.itemId } },
        body: { fileId: hit.fileId, language: language(), hearingImpaired: hit.hearingImpaired },
      });
      if (!result.data) throw new Error(reason(result.error, "Could not download the subtitle."));
      setNotice(`Saved as ${result.data.fileName}.`);
      props.onDownloaded?.(result.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not download the subtitle.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
      title="Find subtitles"
      description={`Search OpenSubtitles for ${props.title}. The subtitle is saved next to the video.`}
    >
      <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
        <div style={{ display: "flex", gap: "8px", "align-items": "center", "flex-wrap": "wrap" }}>
          <label>
            Language{" "}
            <select value={language()} onChange={(e) => setLanguage(e.currentTarget.value)}>
              <For each={LANGUAGES}>
                {(l) => (
                  <option value={l.code} selected={language() === l.code}>
                    {l.label}
                  </option>
                )}
              </For>
            </select>
          </label>
          <Button onClick={() => void search()} disabled={busy()}>
            Search
          </Button>
        </div>
        <Show when={error()}>
          <p role="alert" style={{ margin: 0, color: "var(--danger)" }}>
            {error()}
          </p>
        </Show>
        <Show when={notice()}>
          <p role="status" style={{ margin: 0, color: "var(--success)" }}>
            {notice()}
          </p>
        </Show>
        <Show when={hits()}>
          {(list) => (
            <Show
              when={list().length > 0}
              fallback={<p style={{ margin: 0 }}>No subtitles found in this language.</p>}
            >
              <ul
                style={{
                  margin: 0,
                  padding: 0,
                  "list-style": "none",
                  display: "flex",
                  "flex-direction": "column",
                  gap: "6px",
                  "max-height": "50vh",
                  overflow: "auto",
                }}
              >
                <For each={list()}>
                  {(hit) => (
                    <li
                      style={{
                        display: "flex",
                        gap: "8px",
                        "align-items": "center",
                        padding: "8px",
                        border: "1px solid var(--border)",
                        "border-radius": "var(--radius-md)",
                      }}
                    >
                      <div style={{ flex: 1, "min-width": 0 }}>
                        <div style={{ "overflow-wrap": "anywhere" }}>
                          {hit.release || hit.fileName}
                        </div>
                        <div style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                          {`${hit.downloads} downloads`}
                          <Show when={hit.hearingImpaired}>
                            {" "}
                            <Badge>Hearing impaired</Badge>
                          </Show>
                        </div>
                      </div>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy()}
                        onClick={() => void download(hit)}
                      >
                        Download
                      </Button>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          )}
        </Show>
      </div>
    </Dialog>
  );
}

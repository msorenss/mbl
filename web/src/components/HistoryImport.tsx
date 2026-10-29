import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

export default function HistoryImport() {
  const [provider, setProvider] = createSignal<"plex" | "jellyfin">("plex");
  const [payload, setPayload] = createSignal<unknown>(null);
  const [fileName, setFileName] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [summary, setSummary] = createSignal<components["schemas"]["ImportSummary"] | null>(null);
  let generation = 0;
  const readFile = async (file?: File) => {
    const current = ++generation;
    setPayload(null);
    setSummary(null);
    setError("");
    setFileName("");
    if (!file) return;
    try {
      if (file.size > 1_000_000) throw new Error("Choose a JSON file smaller than 1 MB.");
      const data: unknown = JSON.parse(await file.text());
      if (current !== generation) return;
      if (
        !data ||
        typeof data !== "object" ||
        !("items" in data) ||
        !Array.isArray(data.items) ||
        data.items.length === 0
      )
        throw new Error("The JSON file must contain a nonempty items array.");
      const field = provider() === "plex" ? "title" : "name";
      if (
        !data.items.every(
          (item: unknown) =>
            item &&
            typeof item === "object" &&
            field in item &&
            typeof (item as Record<string, unknown>)[field] === "string",
        )
      )
        throw new Error(`Each item must contain ${field}. Check the export format.`);
      setPayload(data);
      setFileName(file.name);
    } catch (reason) {
      if (current === generation)
        setError(reason instanceof Error ? reason.message : "Could not read file.");
    }
  };
  const submit = async () => {
    if (!payload() || busy()) return;
    setBusy(true);
    setError("");
    setSummary(null);
    try {
      const result =
        provider() === "plex"
          ? await client.POST("/api/v1/import/plex", {
              body: payload() as components["schemas"]["PlexImportRequest"],
            })
          : await client.POST("/api/v1/import/jellyfin", {
              body: payload() as components["schemas"]["JellyfinImportRequest"],
            });
      if (result.error || !result.data)
        throw new Error("Import failed. Your file remains selected so you can retry.");
      setSummary(result.data);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Import failed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <section aria-labelledby="history-import-heading">
      <h2 id="history-import-heading">Import watch history</h2>
      <p>
        Import into your current account. Existing library items are matched by external ID or
        title/year. Matching watch state may be updated.
      </p>
      <label for="history-provider">Export format</label>{" "}
      <select
        id="history-provider"
        disabled={busy()}
        value={provider()}
        onChange={(event) => {
          generation++;
          setProvider(event.currentTarget.value as "plex" | "jellyfin");
          setPayload(null);
          setFileName("");
          setSummary(null);
          setError("");
        }}
      >
        <option value="plex">Plex</option>
        <option value="jellyfin">Jellyfin</option>
      </select>
      <p>
        Expected JSON:{" "}
        {provider() === "plex"
          ? '{"items":[{"title":"Movie","year":2024,"viewCount":1,"viewOffsetMs":0}]}'
          : '{"items":[{"name":"Movie","productionYear":2024,"played":true,"playbackPositionTicks":0}]}'}
        .
      </p>
      <label for="history-file">History JSON file</label>{" "}
      <input
        id="history-file"
        type="file"
        accept=".json,application/json"
        disabled={busy()}
        onChange={(event) => void readFile(event.currentTarget.files?.[0])}
      />
      <p>{fileName()}</p>
      <Button disabled={!payload() || busy()} onClick={submit}>
        {busy() ? "Importing…" : "Import history"}
      </Button>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={summary()}>
        {(result) => (
          <div role="status">
            <p>
              {result().matchedItems} of {result().totalItems} matched; {result().updatedWatchState}{" "}
              watch states updated.
            </p>
            <Show when={result().unmatched.length}>
              <h3>Unmatched titles</h3>
              <ul>
                <For each={result().unmatched}>{(title) => <li>{title}</li>}</For>
              </ul>
            </Show>
          </div>
        )}
      </Show>
    </section>
  );
}

import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { Button } from "./ui/Button";

type DirListing = components["schemas"]["DirListing"];
type Props = {
  paths: string[];
  disabled?: boolean;
  onAdd: (path: string) => void;
  onRemove: (path: string) => void;
};

export function LibraryFolderPicker(props: Props) {
  const { t } = useI18n();
  const [listing, setListing] = createSignal<DirListing>();
  const [path, setPath] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");
  let version = 0;
  let inputRevision = 0;
  let request: AbortController | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;

  const browse = async (target?: string) => {
    const current = ++version;
    const revision = inputRevision;
    request?.abort();
    clearTimeout(deadline);
    const abort = new AbortController();
    request = abort;
    let timedOut = false;
    deadline = setTimeout(() => {
      timedOut = true;
      abort.abort();
    }, 10000);
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/system/browse", {
        params: { query: { path: target || undefined } },
        signal: abort.signal,
      });
      if (current !== version) return;
      if (timedOut) throw new Error(t("folderTimeout"));
      if (result.error || !result.data)
        throw new Error(apiErrorMessage(result.error, t("folderRequestFailed")));
      setListing(result.data);
      if (revision === inputRevision) setPath(result.data.path || "");
    } catch (cause) {
      if (current === version)
        setError(timedOut ? t("folderTimeout") : apiErrorMessage(cause, t("folderRequestFailed")));
    } finally {
      if (current === version) {
        clearTimeout(deadline);
        setLoading(false);
      }
    }
  };
  onMount(() => void browse());
  onCleanup(() => {
    version++;
    request?.abort();
    clearTimeout(deadline);
  });

  return (
    <fieldset disabled={props.disabled} style={{ border: "none", padding: "0", margin: "0" }}>
      <legend style={{ "font-weight": "600", "margin-bottom": "8px" }}>{t("folderRoots")}</legend>
      <Show when={props.paths.length > 0}>
        <ul style={{ padding: "0", "list-style": "none" }}>
          <For each={props.paths}>
            {(selected) => (
              <li
                style={{
                  display: "flex",
                  gap: "8px",
                  "align-items": "center",
                  "margin-bottom": "6px",
                }}
              >
                <span style={{ "overflow-wrap": "anywhere", flex: "1" }}>{selected}</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  aria-label={`${t("folderRemove")}: ${selected}`}
                  onClick={() => props.onRemove(selected)}
                >
                  ✕
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <Show when={props.paths.some((p) => p === "/" || /^[a-z]:[\\/]?$/i.test(p))}>
        <p role="note" style={{ color: "var(--text-secondary)" }}>
          {t("folderRootWarning")}
        </p>
      </Show>
      <label for="library-folder-path">{t("folderPath")}</label>
      <div style={{ display: "flex", gap: "8px", "margin-top": "6px", "margin-bottom": "8px" }}>
        <input
          id="library-folder-path"
          value={path()}
          placeholder="/media/series"
          autocomplete="off"
          spellcheck={false}
          onInput={(event) => {
            inputRevision++;
            setPath(event.currentTarget.value);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.stopPropagation();
              if (path().trim()) void browse(path().trim());
            }
          }}
          style={{
            width: "100%",
            "min-width": "0",
            padding: "8px",
            background: "var(--bg-base)",
            color: "var(--text-primary)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-sm)",
          }}
        />
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={!path().trim()}
          onClick={() => void browse(path().trim())}
        >
          {t("folderOpen")}
        </Button>
      </div>
      <Show when={error()}>
        <div role="alert" style={{ "overflow-wrap": "anywhere", "margin-bottom": "10px" }}>
          <strong>{t("folderRequestFailed")}</strong>
          <p>{error()}</p>
          <p>{t("folderAccessHelp")}</p>
        </div>
      </Show>
      <div
        aria-busy={loading()}
        style={{
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-sm)",
          padding: "10px",
        }}
      >
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "8px",
            "justify-content": "space-between",
          }}
        >
          <span style={{ "overflow-wrap": "anywhere", "min-width": "0" }}>
            {t("folderCurrent")}: {listing()?.path || t("folderServerRoots")}
          </span>
          <Button
            type="button"
            size="sm"
            disabled={
              loading() ||
              !!error() ||
              !listing()?.path ||
              path().trim() !== listing()?.path ||
              props.paths.includes(listing()?.path || "")
            }
            onClick={() => {
              const current = listing()?.path;
              if (current && path().trim() === current && !loading() && !error())
                props.onAdd(current);
            }}
          >
            {t("folderAddCurrent")}
          </Button>
        </div>
        <Show when={loading()}>
          <p role="status">{t("folderLoading")}</p>
        </Show>
        <div style={{ "max-height": "180px", "overflow-y": "auto", "margin-top": "8px" }}>
          <Show when={listing()?.parent}>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={loading()}
              onClick={() => void browse(listing()?.parent || undefined)}
            >
              ↑ {t("folderUp")}
            </Button>
          </Show>
          <For each={listing()?.entries || []}>
            {(entry) => (
              <div style={{ display: "flex", "justify-content": "space-between", gap: "8px" }}>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={loading()}
                  style={{ "min-width": "0", "overflow-wrap": "anywhere", "text-align": "left" }}
                  onClick={() => void browse(entry.path)}
                >
                  📁 {entry.name}
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  disabled={loading() || !!error() || props.paths.includes(entry.path)}
                  aria-label={`${t("folderAdd")}: ${entry.path}`}
                  onClick={() => props.onAdd(entry.path)}
                >
                  + {t("folderAdd")}
                </Button>
              </div>
            )}
          </For>
          <Show when={listing()?.path && !loading() && !error() && listing()?.entries.length === 0}>
            <p>{t("folderEmpty")}</p>
          </Show>
        </div>
      </div>
    </fieldset>
  );
}

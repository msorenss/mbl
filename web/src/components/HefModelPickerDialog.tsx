import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { Button } from "./ui/Button";
import { Dialog } from "./ui/Dialog";

type DirListing = components["schemas"]["DirListing"];

export type HefModelSlot = "vision" | "vlm" | "llm" | "whisper";

const QUICK_DIRS: { label: string; path: string }[] = [
  { label: "/usr/share/hailo-models", path: "/usr/share/hailo-models" },
  { label: "/var/lib/mbl/models", path: "/var/lib/mbl/models" },
  { label: "/usr/share/hailo-ollama/models/blobs", path: "/usr/share/hailo-ollama/models/blobs" },
  { label: "/", path: "/" },
];

function formatFileSize(bytes?: number | null): string {
  if (bytes == null || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export interface HefModelPickerDialogProps {
  open: boolean;
  slotLabel: string;
  slotDescription: string;
  currentPath?: string | null;
  onClose: () => void;
  onSelect: (hefPath: string) => Promise<void> | void;
}

export function HefModelPickerDialog(props: HefModelPickerDialogProps) {
  const [listing, setListing] = createSignal<DirListing>();
  const [pathInput, setPathInput] = createSignal("/usr/share/hailo-models");
  const [selectedFile, setSelectedFile] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");

  let reqVersion = 0;
  let activeAbort: AbortController | undefined;

  const browseDir = async (targetDir?: string) => {
    const myVer = ++reqVersion;
    activeAbort?.abort();
    const abort = new AbortController();
    activeAbort = abort;
    setLoading(true);
    setError("");
    try {
      const res = await client.GET("/api/v1/system/browse", {
        params: { query: { path: targetDir || undefined, ext: "hef" } },
        signal: abort.signal,
      });
      if (myVer !== reqVersion) return;
      if (res.error || !res.data) {
        throw new Error(apiErrorMessage(res.error, "Kunde inte läsa katalogen."));
      }
      setListing(res.data);
      if (res.data.path) {
        setPathInput(res.data.path);
      }
    } catch (err) {
      if (myVer === reqVersion) {
        setError(apiErrorMessage(err, "Kunde inte öppna katalogen."));
      }
    } finally {
      if (myVer === reqVersion) {
        setLoading(false);
      }
    }
  };

  createEffect(() => {
    if (props.open) {
      const cur = props.currentPath?.trim() || "";
      setSelectedFile(cur);
      const lastSlash = cur.lastIndexOf("/");
      const initialDir = lastSlash > 0 ? cur.slice(0, lastSlash) : "/usr/share/hailo-models";
      setPathInput(initialDir);
      void browseDir(initialDir);
    }
  });

  onCleanup(() => {
    reqVersion++;
    activeAbort?.abort();
  });

  const handleConfirm = async (filePath: string) => {
    const trimmed = filePath.trim();
    if (!trimmed.toLowerCase().endsWith(".hef")) {
      setError("Vald fil måste ha filändelsen .hef");
      return;
    }
    setSaving(true);
    setError("");
    try {
      await props.onSelect(trimmed);
      props.onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Kunde inte spara .hef-sökvägen.");
    } finally {
      setSaving(false);
    }
  };

  const handleResetAuto = async () => {
    setSaving(true);
    setError("");
    try {
      await props.onSelect("");
      props.onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Kunde inte återställa till automatisk.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={(isOpen) => {
        if (!isOpen) props.onClose();
      }}
      title={`Välj .hef-modell — ${props.slotLabel}`}
      description={props.slotDescription}
    >
      <div
        style={{ display: "flex", "flex-direction": "column", gap: "12px", "font-size": "13px" }}
      >
        {/* Quick jump buttons */}
        <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
          <For each={QUICK_DIRS}>
            {(q) => (
              <button
                type="button"
                onClick={() => void browseDir(q.path)}
                style={{
                  padding: "4px 8px",
                  "font-size": "11px",
                  "font-family": "var(--font-mono)",
                  background: "var(--bg-overlay)",
                  color: "var(--text-secondary)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  cursor: "pointer",
                }}
              >
                📁 {q.label}
              </button>
            )}
          </For>
        </div>

        {/* Manual directory or file path input */}
        <div style={{ display: "flex", gap: "8px" }}>
          <input
            type="text"
            aria-label="Katalog eller .hef-sökväg"
            value={pathInput()}
            placeholder="/usr/share/hailo-models"
            onInput={(e) => setPathInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                const val = pathInput().trim();
                if (val.toLowerCase().endsWith(".hef")) {
                  void handleConfirm(val);
                } else {
                  void browseDir(val);
                }
              }
            }}
            style={{
              flex: "1",
              "min-width": "0",
              padding: "7px 10px",
              background: "var(--bg-base)",
              color: "var(--text-primary)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-sm)",
              "font-family": "var(--font-mono)",
              "font-size": "12px",
            }}
          />
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={loading()}
            onClick={() => {
              const val = pathInput().trim();
              if (val.toLowerCase().endsWith(".hef")) {
                void handleConfirm(val);
              } else {
                void browseDir(val);
              }
            }}
          >
            {pathInput().trim().toLowerCase().endsWith(".hef") ? "Använd fil" : "Öppna mapp"}
          </Button>
        </div>

        <Show when={error()}>
          <div
            role="alert"
            style={{
              padding: "8px 10px",
              background: "rgba(239, 68, 68, 0.12)",
              border: "1px solid var(--danger)",
              "border-radius": "var(--radius-sm)",
              color: "var(--danger)",
              "font-size": "12px",
            }}
          >
            {error()}
          </div>
        </Show>

        {/* Directory & .hef file listing */}
        <div
          style={{
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-sm)",
            background: "var(--bg-base)",
            padding: "8px",
            "max-height": "260px",
            "overflow-y": "auto",
            display: "flex",
            "flex-direction": "column",
            gap: "4px",
          }}
        >
          <div
            style={{
              display: "flex",
              "justify-content": "space-between",
              "align-items": "center",
              "padding-bottom": "6px",
              "margin-bottom": "4px",
              "border-bottom": "1px solid var(--border)",
              "font-size": "11px",
              color: "var(--text-secondary)",
            }}
          >
            <span style={{ "font-family": "var(--font-mono)", "overflow-wrap": "anywhere" }}>
              Mapp: {listing()?.path || "/"}
            </span>
            <Show when={listing()?.parent}>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={loading()}
                onClick={() => void browseDir(listing()?.parent || undefined)}
              >
                ↑ Upp en nivå
              </Button>
            </Show>
          </div>

          <Show when={loading()}>
            <div style={{ padding: "12px", color: "var(--text-muted)", "text-align": "center" }}>
              Läser katalog...
            </div>
          </Show>

          <Show when={!loading()}>
            <For each={listing()?.entries || []}>
              {(entry) => (
                <div
                  style={{
                    display: "flex",
                    "justify-content": "space-between",
                    "align-items": "center",
                    gap: "8px",
                    padding: "5px 8px",
                    "border-radius": "var(--radius-sm)",
                    background:
                      selectedFile() === entry.path
                        ? "var(--accent-light, rgba(59, 130, 246, 0.16))"
                        : "transparent",
                  }}
                >
                  <Show
                    when={entry.isFile}
                    fallback={
                      <button
                        type="button"
                        onClick={() => void browseDir(entry.path)}
                        style={{
                          flex: "1",
                          background: "none",
                          border: "none",
                          color: "var(--text-primary)",
                          "text-align": "left",
                          cursor: "pointer",
                          "font-size": "13px",
                          padding: "2px 0",
                        }}
                      >
                        📁 {entry.name}
                      </button>
                    }
                  >
                    <button
                      type="button"
                      onClick={() => {
                        setSelectedFile(entry.path);
                        setPathInput(entry.path);
                      }}
                      style={{
                        flex: "1",
                        background: "none",
                        border: "none",
                        color: "var(--accent)",
                        "font-family": "var(--font-mono)",
                        "font-weight": "600",
                        "text-align": "left",
                        cursor: "pointer",
                        "font-size": "12px",
                        padding: "2px 0",
                        "overflow-wrap": "anywhere",
                      }}
                    >
                      🧠 {entry.name}{" "}
                      <span style={{ color: "var(--text-muted)", "font-weight": "400" }}>
                        {entry.sizeBytes != null ? `(${formatFileSize(entry.sizeBytes)})` : ""}
                      </span>
                    </button>
                    <Button
                      type="button"
                      size="sm"
                      variant="primary"
                      disabled={saving()}
                      onClick={() => void handleConfirm(entry.path)}
                    >
                      Välj .hef
                    </Button>
                  </Show>
                </div>
              )}
            </For>

            <Show when={(listing()?.entries.length ?? 0) === 0}>
              <div style={{ padding: "12px", color: "var(--text-muted)", "text-align": "center" }}>
                Inga undermappar eller .hef-filer i denna katalog.
              </div>
            </Show>
          </Show>
        </div>

        {/* Footer actions */}
        <div
          style={{
            display: "flex",
            "justify-content": "space-between",
            "align-items": "center",
            "padding-top": "8px",
            "border-top": "1px solid var(--border)",
          }}
        >
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={saving()}
            onClick={() => void handleResetAuto()}
          >
            ↺ Återställ till automatisk identifiering
          </Button>
          <div style={{ display: "flex", gap: "8px" }}>
            <Button type="button" variant="secondary" size="sm" onClick={() => props.onClose()}>
              Avbryt
            </Button>
            <Button
              type="button"
              variant="primary"
              size="sm"
              disabled={saving() || !selectedFile().trim().toLowerCase().endsWith(".hef")}
              onClick={() => void handleConfirm(selectedFile())}
            >
              Spara val
            </Button>
          </div>
        </div>
      </div>
    </Dialog>
  );
}

import { A } from "@solidjs/router";
import { createResource, createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { LibraryFolderPicker } from "../components/LibraryFolderPicker";
import { LibraryScanControl } from "../components/LibraryScanControl";
import { PosterCard } from "../components/PosterCard";
import { Dialog } from "../components/ui/Dialog";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";
import { useI18n } from "../i18n";
import { formatRelativeTime } from "../utils/time";

type LibraryDto = components["schemas"]["LibraryDto"];
type LibraryKind = components["schemas"]["LibraryKind"];

function LibraryMediaPreview(props: { libraryId: number }) {
  const { t } = useI18n();
  const [items] = createResource(
    () => props.libraryId,
    async (id) => {
      try {
        const { data } = await client.GET("/api/v1/libraries/{id}/items", {
          params: { path: { id }, query: { sort: "added", order: "desc", limit: 8 } },
        });
        return data?.items ?? null;
      } catch {
        return null;
      }
    },
  );

  return (
    <section style={{ "margin-top": "16px", "min-width": 0 }} aria-label={t("recentlyAdded")}>
      <h3 style={{ margin: "0 0 10px", "font-size": "var(--font-size-sm)" }}>
        {t("recentlyAdded")}
      </h3>
      <Show when={!items.loading} fallback={<p>{t("Loading media…")}</p>}>
        <Show
          when={items()?.length}
          fallback={<p>{t(items() === null ? "Could not load media." : "No media added yet.")}</p>}
        >
          <div
            style={{
              display: "flex",
              gap: "12px",
              "overflow-x": "auto",
              padding: "4px 4px 10px",
              "scroll-snap-type": "x proximity",
            }}
          >
            <For each={items()}>{(item) => <PosterCard item={item} />}</For>
          </div>
        </Show>
      </Show>
    </section>
  );
}

export function Libraries() {
  const { user } = useAuth();
  const { showToast } = useToast();
  const { t } = useI18n();

  const [settings] = createResource(async () => {
    try {
      const res = await client.GET("/api/v1/system/settings");
      return res.data;
    } catch {
      return null;
    }
  });

  const [libraries, setLibraries] = createSignal<LibraryDto[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [editingId, setEditingId] = createSignal<number | null>(null);
  const [showAddModal, setShowAddModal] = createSignal(false);

  // Add Library form state
  const [newLibName, setNewLibName] = createSignal("");
  const [newLibKind, setNewLibKind] = createSignal<LibraryKind>("movie");
  const [selectedPaths, setSelectedPaths] = createSignal<string[]>([]);
  const [isCreating, setIsCreating] = createSignal(false);
  const [formError, setFormError] = createSignal("");
  let dialogTrigger: HTMLElement | undefined;

  const loadLibraries = async () => {
    setLoading(true);
    try {
      const { data, error } = await client.GET("/api/v1/libraries");
      if (error) throw new Error("Could not load libraries");
      if (data) setLibraries(data);
    } catch {
      showToast({
        title: "Error",
        message: "The server request failed. Try again.",
        type: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenAddModal = () => {
    dialogTrigger =
      document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    setEditingId(null);
    setNewLibName("");
    setNewLibKind("movie");
    setSelectedPaths([]);
    setShowAddModal(true);
    setFormError("");
  };

  const addPath = (path: string) => {
    if (!selectedPaths().includes(path)) {
      setSelectedPaths([...selectedPaths(), path]);
    }
  };

  const removePath = (path: string) => {
    setSelectedPaths(selectedPaths().filter((p) => p !== path));
  };

  const handleCreateLibrary = async (e: Event) => {
    e.preventDefault();
    if (!newLibName().trim()) {
      showToast({ title: "Validation Error", message: "Library name is required", type: "danger" });
      return;
    }
    if (selectedPaths().length === 0) {
      showToast({
        title: "Validation Error",
        message: "Select at least one folder path",
        type: "danger",
      });
      return;
    }

    setFormError("");
    setIsCreating(true);
    try {
      const editId = editingId();
      const { data, error } =
        editId === null
          ? await client.POST("/api/v1/libraries", {
              body: { name: newLibName().trim(), kind: newLibKind(), paths: selectedPaths() },
            })
          : await client.PATCH("/api/v1/libraries/{id}", {
              params: { path: { id: editId } },
              body: { name: newLibName().trim(), paths: selectedPaths() },
            });

      if (data && !error) {
        showToast({
          title: editingId() === null ? "Library Created" : "Library Updated",
          message: `Library '${data.name}' saved`,
          type: "success",
        });
        setShowAddModal(false);
        // Automatically start scan
        const scan = await client.POST("/api/v1/libraries/{id}/scan", {
          params: { path: { id: data.id } },
        });
        if (scan.error)
          showToast({
            title: "Scan failed",
            message: "Library was saved. Retry its scan.",
            type: "error",
          });
        await loadLibraries();
      } else {
        setFormError(apiErrorMessage(error, "Failed to save library"));
        showToast({
          title: "Save failed",
          message: apiErrorMessage(error, "Failed to save library"),
          type: "danger",
        });
      }
    } catch (e) {
      setFormError(apiErrorMessage(e, "Failed to save library"));
      showToast({
        title: "Error",
        message: apiErrorMessage(e, "Failed to save library"),
        type: "danger",
      });
    } finally {
      setIsCreating(false);
    }
  };

  const editLibrary = (library: LibraryDto) => {
    dialogTrigger =
      document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    setEditingId(library.id);
    setNewLibName(library.name);
    setNewLibKind(library.kind);
    setSelectedPaths([...library.paths]);
    setShowAddModal(true);
    setFormError("");
  };
  const deleteLibrary = async (library: LibraryDto) => {
    if (
      !window.confirm(`Remove library '${library.name}' and its index? Media files remain on disk.`)
    )
      return;
    try {
      const result = await client.DELETE("/api/v1/libraries/{id}", {
        params: { path: { id: library.id } },
      });
      if (result.error) throw new Error("Could not remove library");
      await loadLibraries();
    } catch {
      showToast({ title: "Error", message: "Library could not be removed", type: "error" });
    }
  };

  const handleRefreshMetadata = async (lib: LibraryDto) => {
    try {
      const { data, error } = await client.POST("/api/v1/libraries/{id}/refresh", {
        params: { path: { id: lib.id } },
      });
      if (data && !error) {
        showToast({
          title: "Metadata Refresh Queued",
          message: `Refreshing TMDB metadata for ${lib.name} (Job #${data.jobId})`,
          type: "success",
        });
      } else {
        showToast({
          title: "Refresh Failed",
          message: apiErrorMessage(error, "Failed to trigger metadata refresh"),
          type: "danger",
        });
      }
    } catch (e) {
      showToast({
        title: "Error",
        message: e instanceof Error ? e.message : "Network error",
        type: "danger",
      });
    }
  };

  onMount(loadLibraries);

  return (
    <div class="mbl-page">
      {/* Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "24px",
        }}
      >
        <div>
          <h1 style={{ margin: "0 0 6px", "font-size": "24px", "font-weight": "800" }}>
            {t("libraries")}
          </h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            {t("librariesSubtitle")}
          </p>
        </div>

        <Show when={user()?.isAdmin}>
          <button
            type="button"
            onClick={handleOpenAddModal}
            style={{
              padding: "10px 18px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-sm)",
              "font-weight": "600",
              "font-size": "13px",
              cursor: "pointer",
            }}
          >
            {t("addLibrary")}
          </button>
        </Show>
      </div>

      <Show when={user()?.isAdmin && settings() && !settings()?.tmdbConfigured}>
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "12px 16px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            "margin-bottom": "20px",
            "font-size": "13px",
            color: "var(--text-secondary)",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
            <span>💡</span>
            <span>
              <strong>Metadata Provider:</strong> TMDB API key is not configured. Media will match
              local NFO files and embedded tags only.
            </span>
          </div>
          <A
            href="/admin/settings"
            style={{
              color: "var(--accent)",
              "font-weight": "600",
              "text-decoration": "underline",
              "white-space": "nowrap",
              "margin-left": "12px",
            }}
          >
            Configure TMDB &rarr;
          </A>
        </div>
      </Show>

      {/* Libraries Grid */}
      <Show
        when={!loading()}
        fallback={<div style={{ color: "var(--text-muted)" }}>Loading libraries...</div>}
      >
        <Show
          when={libraries().length > 0}
          fallback={
            <div
              style={{
                padding: "48px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
                "text-align": "center",
                color: "var(--text-muted)",
              }}
            >
              No libraries configured yet.
            </div>
          }
        >
          <div
            style={{
              display: "grid",
              "grid-template-columns": "minmax(0, 1fr)",
              gap: "16px",
            }}
          >
            <For each={libraries()}>
              {(lib) => (
                <div
                  style={{
                    padding: "24px",
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-md)",
                    display: "flex",
                    "flex-direction": "column",
                    "justify-content": "space-between",
                  }}
                >
                  <div>
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        "justify-content": "space-between",
                      }}
                    >
                      <A
                        href={`/library/${lib.id}`}
                        style={{
                          "font-size": "18px",
                          "font-weight": "700",
                          color: "var(--text-primary)",
                        }}
                      >
                        {lib.name}
                      </A>
                      <span
                        style={{
                          "font-size": "11px",
                          padding: "2px 8px",
                          background: "var(--bg-card)",
                          border: "1px solid var(--border)",
                          "border-radius": "10px",
                          color: "var(--text-muted)",
                          "text-transform": "capitalize",
                        }}
                      >
                        {lib.kind}
                      </span>
                    </div>

                    <LibraryMediaPreview libraryId={lib.id} />
                    <Show when={user()?.isAdmin && lib.paths.length}>
                      <details style={{ "margin-top": "12px", "overflow-wrap": "anywhere" }}>
                        <summary>{t("Library folders")}</summary>
                        <For each={lib.paths}>{(path) => <div>{path}</div>}</For>
                      </details>
                    </Show>

                    <div
                      style={{
                        "margin-top": "10px",
                        "font-size": "11px",
                        color: "var(--text-muted)",
                        display: "flex",
                        "align-items": "center",
                        gap: "6px",
                      }}
                    >
                      <Show when={lib.lastScannedAt} fallback={<span>⏳ {t("neverScanned")}</span>}>
                        <span>
                          🕒 {t("lastScanned")}: {formatRelativeTime(lib.lastScannedAt!)}
                        </span>
                      </Show>
                    </div>
                  </div>

                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
                      "flex-wrap": "wrap",
                      gap: "12px",
                      "margin-top": "20px",
                      "padding-top": "16px",
                      "border-top": "1px solid var(--border)",
                    }}
                  >
                    <A
                      href={`/library/${lib.id}`}
                      style={{ color: "var(--accent)", "font-size": "13px", "font-weight": "600" }}
                    >
                      Browse Media &rarr;
                    </A>

                    <Show when={user()?.isAdmin}>
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          gap: "6px",
                          "flex-wrap": "wrap",
                        }}
                      >
                        <LibraryScanControl id={lib.id} onComplete={loadLibraries} />
                        <button
                          type="button"
                          onClick={() => handleRefreshMetadata(lib)}
                          title="Refresh TMDB metadata and artwork"
                          style={{
                            background: "var(--bg-card)",
                            border: "1px solid var(--border)",
                            color: "var(--text-secondary)",
                            padding: "5px 10px",
                            "border-radius": "var(--radius-sm)",
                            "font-size": "12px",
                            cursor: "pointer",
                          }}
                        >
                          ✨ Refresh TMDB
                        </button>
                        <button
                          type="button"
                          onClick={() => editLibrary(lib)}
                          style={{
                            background: "var(--bg-card)",
                            border: "1px solid var(--border)",
                            color: "var(--text-secondary)",
                            padding: "5px 10px",
                            "border-radius": "var(--radius-sm)",
                            "font-size": "12px",
                            cursor: "pointer",
                          }}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          onClick={() => deleteLibrary(lib)}
                          style={{
                            background: "var(--danger-light)",
                            border: "1px solid color-mix(in srgb, var(--danger) 45%, transparent)",
                            color: "var(--danger)",
                            padding: "5px 10px",
                            "border-radius": "var(--radius-sm)",
                            "font-size": "12px",
                            cursor: "pointer",
                          }}
                        >
                          Remove
                        </button>
                      </div>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Show>

      <Show when={showAddModal()}>
        <Dialog
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            document.getElementById("lib-name")?.focus();
          }}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            dialogTrigger?.focus();
          }}
          open={showAddModal()}
          onOpenChange={(open) => {
            if (!isCreating()) setShowAddModal(open);
          }}
          title={editingId() === null ? "Add New Library" : "Edit Library"}
          description="Select readable media folders on the server. Existing library roots cannot overlap."
        >
          <form
            onSubmit={handleCreateLibrary}
            style={{ display: "flex", "flex-direction": "column", gap: "16px" }}
          >
            <div>
              <label
                for="lib-name"
                style={{
                  display: "block",
                  "margin-bottom": "6px",
                  "font-size": "13px",
                  "font-weight": "600",
                }}
              >
                Library Name
              </label>
              <input
                id="lib-name"
                type="text"
                value={newLibName()}
                onInput={(e) => setNewLibName(e.currentTarget.value)}
                placeholder="e.g. Movies, TV Shows, Anime"
                style={{
                  width: "100%",
                  padding: "9px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "14px",
                }}
              />
            </div>

            <div>
              <label
                for="lib-kind"
                style={{
                  display: "block",
                  "margin-bottom": "6px",
                  "font-size": "13px",
                  "font-weight": "600",
                }}
              >
                Content Type
              </label>
              <select
                id="lib-kind"
                disabled={editingId() !== null}
                value={newLibKind()}
                onChange={(e) => setNewLibKind(e.currentTarget.value as LibraryKind)}
                style={{
                  width: "100%",
                  padding: "9px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "14px",
                }}
              >
                <option value="movie">Movies</option>
                <option value="show">TV Shows</option>
                <option value="music">Music</option>
                <option value="other">Other / Mixed</option>
              </select>
            </div>

            <LibraryFolderPicker
              paths={selectedPaths()}
              disabled={isCreating()}
              onAdd={addPath}
              onRemove={removePath}
            />
            <Show when={formError()}>
              <p role="alert">{formError()}</p>
            </Show>

            {/* Submit Buttons */}
            <div
              style={{
                display: "flex",
                "justify-content": "flex-end",
                gap: "10px",
                "margin-top": "12px",
              }}
            >
              <button
                type="button"
                onClick={() => setShowAddModal(false)}
                disabled={isCreating()}
                style={{
                  padding: "9px 16px",
                  background: "var(--bg-card)",
                  border: "1px solid var(--border)",
                  color: "var(--text-primary)",
                  "border-radius": "var(--radius-sm)",
                  cursor: "pointer",
                }}
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={isCreating()}
                style={{
                  padding: "9px 18px",
                  background: "var(--accent)",
                  color: "#fff",
                  border: "none",
                  "border-radius": "var(--radius-sm)",
                  "font-weight": "600",
                  cursor: isCreating() ? "not-allowed" : "pointer",
                }}
              >
                {isCreating() ? "Saving..." : "Save Library"}
              </button>
            </div>
          </form>
        </Dialog>
      </Show>
    </div>
  );
}
export default Libraries;

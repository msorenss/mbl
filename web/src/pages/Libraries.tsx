import { A } from "@solidjs/router";
import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";

type LibraryDto = components["schemas"]["LibraryDto"];
type LibraryKind = components["schemas"]["LibraryKind"];
type DirEntry = components["schemas"]["DirEntry"];

export function Libraries() {
  const { user } = useAuth();
  const { showToast } = useToast();

  const [libraries, setLibraries] = createSignal<LibraryDto[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [showAddModal, setShowAddModal] = createSignal(false);

  // Add Library form state
  const [newLibName, setNewLibName] = createSignal("");
  const [newLibKind, setNewLibKind] = createSignal<LibraryKind>("movie");
  const [selectedPaths, setSelectedPaths] = createSignal<string[]>([]);
  const [currentBrowsePath, setCurrentBrowsePath] = createSignal<string>("");
  const [browseEntries, setBrowseEntries] = createSignal<DirEntry[]>([]);
  const [browseParent, setBrowseParent] = createSignal<string | null>(null);
  const [browseLoading, setBrowseLoading] = createSignal(false);
  const [isCreating, setIsCreating] = createSignal(false);

  const loadLibraries = async () => {
    setLoading(true);
    try {
      const { data } = await client.GET("/api/v1/libraries");
      if (data) setLibraries(data);
    } catch {
      // Ignore
    } finally {
      setLoading(false);
    }
  };

  const browseFolder = async (path?: string) => {
    setBrowseLoading(true);
    try {
      const { data } = await client.GET("/api/v1/system/browse", {
        params: { query: { path: path || undefined } },
      });
      if (data) {
        setCurrentBrowsePath(data.path || "");
        setBrowseParent(data.parent || null);
        setBrowseEntries(data.entries || []);
      }
    } catch {
      // Ignore
    } finally {
      setBrowseLoading(false);
    }
  };

  const handleOpenAddModal = () => {
    setNewLibName("");
    setNewLibKind("movie");
    setSelectedPaths([]);
    setShowAddModal(true);
    browseFolder();
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

    setIsCreating(true);
    try {
      const { data, error } = await client.POST("/api/v1/libraries", {
        body: {
          name: newLibName().trim(),
          kind: newLibKind(),
          paths: selectedPaths(),
        },
      });

      if (data && !error) {
        showToast({
          title: "Library Created",
          message: `Library '${data.name}' created`,
          type: "success",
        });
        setShowAddModal(false);
        // Automatically start scan
        client
          .POST("/api/v1/libraries/{id}/scan", { params: { path: { id: data.id } } })
          .catch(() => {});
        await loadLibraries();
      } else {
        showToast({
          title: "Creation Failed",
          message: error ? String(error) : "Failed to create library",
          type: "danger",
        });
      }
    } catch (e) {
      showToast({
        title: "Error",
        message: e instanceof Error ? e.message : "Error creating library",
        type: "danger",
      });
    } finally {
      setIsCreating(false);
    }
  };

  const handleScan = async (lib: LibraryDto) => {
    try {
      const { data, error } = await client.POST("/api/v1/libraries/{id}/scan", {
        params: { path: { id: lib.id } },
      });
      if (data && !error) {
        showToast({
          title: "Scan Queued",
          message: `Scanning ${lib.name} (Job #${data.jobId})`,
          type: "success",
        });
      } else {
        showToast({
          title: "Scan Failed",
          message: error ? String(error) : "Failed to trigger scan",
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
    <div>
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
            Libraries
          </h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            Media collections configured and monitored on this server
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
            + Add Library
          </button>
        </Show>
      </div>

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
              "grid-template-columns": "repeat(auto-fill, minmax(300px, 1fr))",
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

                    <div
                      style={{
                        "margin-top": "12px",
                        "font-size": "12px",
                        color: "var(--text-muted)",
                      }}
                    >
                      <For each={lib.paths}>
                        {(p) => (
                          <div
                            style={{
                              "white-space": "nowrap",
                              overflow: "hidden",
                              "text-overflow": "ellipsis",
                            }}
                          >
                            📁 {p}
                          </div>
                        )}
                      </For>
                    </div>
                  </div>

                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
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
                      <button
                        type="button"
                        onClick={() => handleScan(lib)}
                        title="Scan library folder"
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
                        🔄 Scan
                      </button>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Show>

      {/* Add Library Modal (Admin) */}
      <Show when={showAddModal()}>
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            width: "100vw",
            height: "100vh",
            background: "rgba(0, 0, 0, 0.7)",
            display: "flex",
            "justify-content": "center",
            "align-items": "center",
            "z-index": 1000,
          }}
        >
          <div
            style={{
              width: "560px",
              "max-height": "90vh",
              overflow: "auto",
              padding: "28px",
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-lg)",
            }}
          >
            <div
              style={{
                display: "flex",
                "justify-content": "space-between",
                "margin-bottom": "20px",
              }}
            >
              <h2 style={{ margin: 0, "font-size": "20px", "font-weight": "700" }}>
                Add New Library
              </h2>
              <button
                type="button"
                onClick={() => setShowAddModal(false)}
                style={{
                  background: "transparent",
                  border: "none",
                  color: "var(--text-muted)",
                  "font-size": "18px",
                  cursor: "pointer",
                }}
              >
                ✕
              </button>
            </div>

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

              {/* Folder Browser */}
              <div>
                <div
                  style={{
                    display: "flex",
                    "justify-content": "space-between",
                    "margin-bottom": "6px",
                  }}
                >
                  <span style={{ "font-size": "13px", "font-weight": "600" }}>Folder Roots</span>
                  <span style={{ "font-size": "12px", color: "var(--text-muted)" }}>
                    Select directories on server
                  </span>
                </div>

                {/* Selected Paths */}
                <Show when={selectedPaths().length > 0}>
                  <div
                    style={{
                      display: "flex",
                      "flex-direction": "column",
                      gap: "6px",
                      "margin-bottom": "12px",
                    }}
                  >
                    <For each={selectedPaths()}>
                      {(p) => (
                        <div
                          style={{
                            display: "flex",
                            "align-items": "center",
                            "justify-content": "space-between",
                            padding: "6px 10px",
                            background: "var(--bg-card)",
                            border: "1px solid var(--border)",
                            "border-radius": "var(--radius-sm)",
                            "font-size": "12px",
                          }}
                        >
                          <span style={{ overflow: "hidden", "text-overflow": "ellipsis" }}>
                            📁 {p}
                          </span>
                          <button
                            type="button"
                            onClick={() => removePath(p)}
                            style={{
                              background: "transparent",
                              border: "none",
                              color: "var(--danger)",
                              cursor: "pointer",
                            }}
                          >
                            ✕
                          </button>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>

                {/* Directory Explorer */}
                <div
                  style={{
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    background: "var(--bg-base)",
                    padding: "10px",
                    "max-height": "180px",
                    overflow: "auto",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
                      "margin-bottom": "8px",
                      "padding-bottom": "6px",
                      "border-bottom": "1px solid var(--border)",
                      "font-size": "12px",
                      color: "var(--text-muted)",
                    }}
                  >
                    <span style={{ overflow: "hidden", "text-overflow": "ellipsis" }}>
                      Current: {currentBrowsePath() || "/"}
                    </span>
                    <button
                      type="button"
                      onClick={() => addPath(currentBrowsePath())}
                      disabled={
                        !currentBrowsePath() || selectedPaths().includes(currentBrowsePath())
                      }
                      style={{
                        padding: "3px 8px",
                        background: "var(--accent)",
                        color: "#fff",
                        border: "none",
                        "border-radius": "var(--radius-sm)",
                        "font-size": "11px",
                        "font-weight": "600",
                        cursor: "pointer",
                      }}
                    >
                      + Add Current
                    </button>
                  </div>

                  <Show when={browseParent()}>
                    {(parent) => (
                      <button
                        type="button"
                        onClick={() => browseFolder(parent())}
                        style={{
                          display: "block",
                          width: "100%",
                          "text-align": "left",
                          background: "transparent",
                          border: "none",
                          padding: "5px 8px",
                          cursor: "pointer",
                          color: "var(--accent)",
                          "font-size": "13px",
                        }}
                      >
                        📁 .. (up)
                      </button>
                    )}
                  </Show>

                  <Show
                    when={!browseLoading()}
                    fallback={
                      <div style={{ "font-size": "12px", color: "var(--text-muted)" }}>
                        Loading...
                      </div>
                    }
                  >
                    <For each={browseEntries()}>
                      {(entry) => (
                        <div
                          style={{
                            padding: "4px 8px",
                            display: "flex",
                            "align-items": "center",
                            "justify-content": "space-between",
                            "font-size": "13px",
                            "border-radius": "var(--radius-sm)",
                          }}
                        >
                          <button
                            type="button"
                            onClick={() => browseFolder(entry.path)}
                            style={{
                              background: "transparent",
                              border: "none",
                              color: "inherit",
                              cursor: "pointer",
                              padding: 0,
                              "font-size": "13px",
                              display: "flex",
                              "align-items": "center",
                              gap: "6px",
                            }}
                          >
                            <span>📁 {entry.name}</span>
                          </button>
                          <button
                            type="button"
                            onClick={() => addPath(entry.path)}
                            style={{
                              padding: "2px 6px",
                              background: "var(--bg-card)",
                              border: "1px solid var(--border)",
                              color: "var(--text-secondary)",
                              "border-radius": "var(--radius-sm)",
                              "font-size": "11px",
                              cursor: "pointer",
                            }}
                          >
                            + Add
                          </button>
                        </div>
                      )}
                    </For>
                  </Show>
                </div>
              </div>

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
                  {isCreating() ? "Creating..." : "Save Library"}
                </button>
              </div>
            </form>
          </div>
        </div>
      </Show>
    </div>
  );
}
export default Libraries;

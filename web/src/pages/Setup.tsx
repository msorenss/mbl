import { useNavigate } from "@solidjs/router";
import { createSignal, For, onMount, Show } from "solid-js";
import client, { setStoredToken } from "../api/client";
import type { components } from "../api/schema";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";

type LibraryKind = components["schemas"]["LibraryKind"];
type DirEntry = components["schemas"]["DirEntry"];

export function Setup() {
  const [step, setStep] = createSignal<1 | 2 | 3 | 4>(1);

  // Step 1: Admin account
  const [username, setUsername] = createSignal("admin");
  const [password, setPassword] = createSignal("");
  const [confirmPassword, setConfirmPassword] = createSignal("");

  // Step 2: First Library
  const [libraryName, setLibraryName] = createSignal("Movies");
  const [libraryKind, setLibraryKind] = createSignal<LibraryKind>("movie");
  const [selectedPaths, setSelectedPaths] = createSignal<string[]>([]);
  const [currentBrowsePath, setCurrentBrowsePath] = createSignal<string>("");
  const [browseEntries, setBrowseEntries] = createSignal<DirEntry[]>([]);
  const [browseParent, setBrowseParent] = createSignal<string | null>(null);
  const [browseLoading, setBrowseLoading] = createSignal(false);
  const [createdLibraryId, setCreatedLibraryId] = createSignal<number | null>(null);

  // Step 3: TMDB key hint
  const [tmdbKey, setTmdbKey] = createSignal("");

  // General state
  const [error, setError] = createSignal<string | null>(null);
  const [isSubmitting, setIsSubmitting] = createSignal(false);

  const { refreshUser } = useAuth();
  const { showToast } = useToast();
  const navigate = useNavigate();

  onMount(async () => {
    try {
      const { data } = await client.GET("/api/v1/system/setup");
      if (data && !data.needsSetup) {
        navigate("/login", { replace: true });
      }
    } catch {
      // Ignore
    }
  });

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

  const handleStep1Submit = async (e: Event) => {
    e.preventDefault();
    if (!username() || !password()) {
      setError("Please fill in all fields");
      return;
    }
    if (password().length < 8) {
      setError("Password must be at least 8 characters long");
      return;
    }
    if (password() !== confirmPassword()) {
      setError("Passwords do not match");
      return;
    }

    setError(null);
    setIsSubmitting(true);
    try {
      const { data, error: err } = await client.POST("/api/v1/system/setup", {
        body: { username: username(), password: password() },
      });

      if (data && !err) {
        setStoredToken(data.token);
        await refreshUser();
        setStep(2);
        browseFolder();
      } else {
        setError(err ? String(err) : "Admin setup failed");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create administrator");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleStep2Submit = async (e: Event) => {
    e.preventDefault();
    if (!libraryName().trim()) {
      setError("Please enter a library name");
      return;
    }
    if (selectedPaths().length === 0) {
      setError("Please select at least one media folder");
      return;
    }

    setError(null);
    setIsSubmitting(true);
    try {
      const { data, error: err } = await client.POST("/api/v1/libraries", {
        body: {
          name: libraryName().trim(),
          kind: libraryKind(),
          paths: selectedPaths(),
        },
      });

      if (data && !err) {
        setCreatedLibraryId(data.id);
        setStep(3);
      } else {
        setError(err ? String(err) : "Failed to create library");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create library");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleFinish = async () => {
    setIsSubmitting(true);
    try {
      const libId = createdLibraryId();
      if (libId) {
        // Trigger initial scan
        await client.POST("/api/v1/libraries/{id}/scan", {
          params: { path: { id: libId } },
        });
      }
      showToast({
        title: "Setup Complete",
        message: "Server is ready! Initial scan in progress.",
        type: "success",
      });
      navigate("/");
    } catch {
      navigate("/");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div
      style={{
        display: "flex",
        "justify-content": "center",
        "align-items": "center",
        "min-height": "100vh",
        padding: "40px 20px",
        background: "var(--bg-base)",
      }}
    >
      <div
        style={{
          width: "520px",
          padding: "36px",
          background: "var(--bg-surface)",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-lg)",
          "box-shadow": "0 25px 50px -12px rgba(0, 0, 0, 0.7)",
        }}
      >
        {/* Wizard Progress indicator */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            gap: "10px",
            "margin-bottom": "28px",
          }}
        >
          <For each={[1, 2, 3, 4]}>
            {(s) => (
              <div
                style={{
                  width: "28px",
                  height: "28px",
                  "border-radius": "50%",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  "font-size": "12px",
                  "font-weight": "700",
                  background:
                    step() === s
                      ? "var(--accent)"
                      : step() > s
                        ? "var(--success)"
                        : "var(--bg-card)",
                  color: step() >= s ? "#fff" : "var(--text-muted)",
                  border: "1px solid var(--border)",
                }}
              >
                {step() > s ? "✓" : s}
              </div>
            )}
          </For>
        </div>

        <Show when={error()}>
          <div
            style={{
              padding: "10px 14px",
              background: "var(--danger-light)",
              border: "1px solid var(--danger)",
              "border-radius": "var(--radius-sm)",
              color: "var(--danger)",
              "font-size": "13px",
              "margin-bottom": "20px",
            }}
          >
            {error()}
          </div>
        </Show>

        {/* STEP 1: Admin Account */}
        <Show when={step() === 1}>
          <div style={{ "text-align": "center", "margin-bottom": "24px" }}>
            <h2 style={{ margin: "0 0 6px", "font-size": "22px", "font-weight": "700" }}>
              1. Create Administrator
            </h2>
            <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "13px" }}>
              Initial setup: define your server's primary admin account
            </p>
          </div>

          <form
            onSubmit={handleStep1Submit}
            style={{ display: "flex", "flex-direction": "column", gap: "16px" }}
          >
            <div>
              <label
                for="step1-username"
                style={{
                  display: "block",
                  "margin-bottom": "6px",
                  "font-size": "13px",
                  "font-weight": "600",
                }}
              >
                Admin Username
              </label>
              <input
                id="step1-username"
                type="text"
                value={username()}
                onInput={(e) => setUsername(e.currentTarget.value)}
                placeholder="admin"
                style={{
                  width: "100%",
                  padding: "10px 12px",
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
                for="step1-pw"
                style={{
                  display: "block",
                  "margin-bottom": "6px",
                  "font-size": "13px",
                  "font-weight": "600",
                }}
              >
                Admin Password (min. 8 chars)
              </label>
              <input
                id="step1-pw"
                type="password"
                value={password()}
                onInput={(e) => setPassword(e.currentTarget.value)}
                placeholder="••••••••"
                style={{
                  width: "100%",
                  padding: "10px 12px",
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
                for="step1-conf"
                style={{
                  display: "block",
                  "margin-bottom": "6px",
                  "font-size": "13px",
                  "font-weight": "600",
                }}
              >
                Confirm Password
              </label>
              <input
                id="step1-conf"
                type="password"
                value={confirmPassword()}
                onInput={(e) => setConfirmPassword(e.currentTarget.value)}
                placeholder="••••••••"
                style={{
                  width: "100%",
                  padding: "10px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "14px",
                }}
              />
            </div>

            <button
              type="submit"
              disabled={isSubmitting()}
              style={{
                "margin-top": "10px",
                padding: "12px",
                background: "var(--accent)",
                color: "#fff",
                border: "none",
                "border-radius": "var(--radius-sm)",
                "font-weight": "600",
                cursor: "pointer",
              }}
            >
              {isSubmitting() ? "Creating Admin..." : "Next: Add Library &rarr;"}
            </button>
          </form>
        </Show>

        {/* STEP 2: Add Initial Library */}
        <Show when={step() === 2}>
          <div style={{ "text-align": "center", "margin-bottom": "24px" }}>
            <h2 style={{ margin: "0 0 6px", "font-size": "22px", "font-weight": "700" }}>
              2. Add Your First Library
            </h2>
            <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "13px" }}>
              Select where your media files are stored on this server
            </p>
          </div>

          <form
            onSubmit={handleStep2Submit}
            style={{ display: "flex", "flex-direction": "column", gap: "16px" }}
          >
            <div>
              <label
                for="step2-libname"
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
                id="step2-libname"
                type="text"
                value={libraryName()}
                onInput={(e) => setLibraryName(e.currentTarget.value)}
                placeholder="Movies"
                style={{
                  width: "100%",
                  padding: "10px 12px",
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
                for="step2-kind"
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
                id="step2-kind"
                value={libraryKind()}
                onChange={(e) => setLibraryKind(e.currentTarget.value as LibraryKind)}
                style={{
                  width: "100%",
                  padding: "10px 12px",
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
                  Server Directory
                </span>
              </div>

              <Show when={selectedPaths().length > 0}>
                <div
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "6px",
                    "margin-bottom": "10px",
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
                          onClick={() => setSelectedPaths(selectedPaths().filter((x) => x !== p))}
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

              <div
                style={{
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  background: "var(--bg-base)",
                  padding: "10px",
                  "max-height": "160px",
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
                    onClick={() => {
                      if (!selectedPaths().includes(currentBrowsePath())) {
                        setSelectedPaths([...selectedPaths(), currentBrowsePath()]);
                      }
                    }}
                    disabled={!currentBrowsePath() || selectedPaths().includes(currentBrowsePath())}
                    style={{
                      padding: "3px 8px",
                      background: "var(--accent)",
                      color: "#fff",
                      border: "none",
                      "border-radius": "var(--radius-sm)",
                      "font-size": "11px",
                      cursor: "pointer",
                    }}
                  >
                    + Select Current
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
                        padding: "4px 6px",
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
                          padding: "4px 6px",
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          "font-size": "13px",
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
                          onClick={() => {
                            if (!selectedPaths().includes(entry.path)) {
                              setSelectedPaths([...selectedPaths(), entry.path]);
                            }
                          }}
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
                          + Select
                        </button>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </div>

            <div style={{ display: "flex", gap: "10px", "margin-top": "10px" }}>
              <button
                type="button"
                onClick={() => setStep(3)}
                style={{
                  padding: "12px",
                  background: "var(--bg-card)",
                  border: "1px solid var(--border)",
                  color: "var(--text-secondary)",
                  "border-radius": "var(--radius-sm)",
                  cursor: "pointer",
                }}
              >
                Skip For Now
              </button>
              <button
                type="submit"
                disabled={isSubmitting()}
                style={{
                  flex: 1,
                  padding: "12px",
                  background: "var(--accent)",
                  color: "#fff",
                  border: "none",
                  "border-radius": "var(--radius-sm)",
                  "font-weight": "600",
                  cursor: "pointer",
                }}
              >
                {isSubmitting() ? "Creating..." : "Next: Metadata Agent &rarr;"}
              </button>
            </div>
          </form>
        </Show>

        {/* STEP 3: TMDB Metadata Hint */}
        <Show when={step() === 3}>
          <div style={{ "text-align": "center", "margin-bottom": "24px" }}>
            <h2 style={{ margin: "0 0 6px", "font-size": "22px", "font-weight": "700" }}>
              3. Metadata & Artwork
            </h2>
            <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "13px" }}>
              MBL automatically downloads posters, backdrops, and cast info from The Movie Database
              (TMDB).
            </p>
          </div>

          <div
            style={{
              padding: "16px",
              background: "var(--bg-card)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-md)",
              "font-size": "13px",
              "line-height": "1.6",
              color: "var(--text-secondary)",
              "margin-bottom": "20px",
            }}
          >
            <p style={{ margin: "0 0 10px" }}>
              You can supply your own free TMDB API key in your server's configuration file (
              <code>mbl.toml</code>) under <code>[metadata.tmdb]</code>.
            </p>
            <p style={{ margin: 0 }}>
              Without an API key, MBL operates in local-metadata mode, reading embedded file tags
              and sidecar NFO/artwork files.
            </p>
          </div>

          <div style={{ "margin-bottom": "24px" }}>
            <label
              for="step3-key"
              style={{
                display: "block",
                "margin-bottom": "6px",
                "font-size": "13px",
                "font-weight": "600",
              }}
            >
              TMDB API Key (Optional)
            </label>
            <input
              id="step3-key"
              type="text"
              value={tmdbKey()}
              onInput={(e) => setTmdbKey(e.currentTarget.value)}
              placeholder="e.g. 3a7b..."
              style={{
                width: "100%",
                padding: "10px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
              }}
            />
          </div>

          <button
            type="button"
            onClick={() => setStep(4)}
            style={{
              width: "100%",
              padding: "12px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-sm)",
              "font-weight": "600",
              cursor: "pointer",
            }}
          >
            Next: Review & Start Scan &rarr;
          </button>
        </Show>

        {/* STEP 4: Ready to Go */}
        <Show when={step() === 4}>
          <div style={{ "text-align": "center", "margin-bottom": "28px" }}>
            <div style={{ "font-size": "48px", "margin-bottom": "14px" }}>🎉</div>
            <h2 style={{ margin: "0 0 8px", "font-size": "24px", "font-weight": "800" }}>
              All Set!
            </h2>
            <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
              Your MBL server is configured. Clicking below will start the initial media scan and
              take you to your home dashboard.
            </p>
          </div>

          <button
            type="button"
            onClick={handleFinish}
            disabled={isSubmitting()}
            style={{
              width: "100%",
              padding: "13px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-sm)",
              "font-weight": "700",
              "font-size": "15px",
              cursor: "pointer",
            }}
          >
            {isSubmitting() ? "Starting Scan..." : "Finish & Start Scan"}
          </button>
        </Show>
      </div>
    </div>
  );
}
export default Setup;

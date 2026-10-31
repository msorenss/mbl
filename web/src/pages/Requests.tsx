import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";

type MediaRequest = components["schemas"]["MediaRequestRow"];
type TmdbSearchHit = components["schemas"]["TmdbSearchHit"];

export default function Requests() {
  const { user } = useAuth();
  const [requests, setRequests] = createSignal<MediaRequest[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [title, setTitle] = createSignal("");
  const [kind, setKind] = createSignal("movie");
  const [tmdb, setTmdb] = createSignal("");
  const [year, setYear] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<TmdbSearchHit[]>([]);
  const [searching, setSearching] = createSignal(false);
  const [selectedPoster, setSelectedPoster] = createSignal<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/requests");
      if (result.error || !result.data) throw new Error("Could not load requests. Retry below.");
      setRequests(result.data);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Could not load requests.");
    } finally {
      setLoading(false);
    }
  };

  onMount(() => void load());

  const searchTmdb = async () => {
    const q = title().trim();
    if (!q) {
      setError("Please enter a title to search on TMDB.");
      return;
    }
    setError("");
    setSearching(true);
    try {
      const res = await client.GET("/api/v1/system/tmdb/search", {
        params: { query: { query: q, kind: kind() } },
      });
      if (res.data?.results) {
        setSearchResults(res.data.results);
        if (res.data.results.length === 0) {
          setError(
            res.data.configured
              ? "No matches found on TMDB for this query."
              : "TMDB API key is not configured in Server Settings.",
          );
        }
      }
    } catch {
      setError("Failed to query TMDB API.");
    } finally {
      setSearching(false);
    }
  };

  const selectHit = (hit: TmdbSearchHit) => {
    setTitle(hit.title);
    setTmdb(String(hit.id));
    setKind(hit.kind === "show" ? "show" : "movie");
    setYear(hit.year ? String(hit.year) : "");
    setSelectedPoster(hit.posterUrl || null);
    setSearchResults([]);
    setError("");
  };

  const submit = async (event: Event) => {
    event.preventDefault();
    if (busy()) return;
    setError("");
    setNotice("");

    let resolvedTmdb = tmdb().trim();
    let resolvedYear = year().trim();

    // If TMDB ID was omitted, try searching for the title automatically
    if (!resolvedTmdb && title().trim()) {
      setBusy(true);
      try {
        const res = await client.GET("/api/v1/system/tmdb/search", {
          params: { query: { query: title().trim(), kind: kind() } },
        });
        const hit = res.data?.results?.[0];
        if (hit) {
          resolvedTmdb = String(hit.id);
          if (!resolvedYear && hit.year) resolvedYear = String(hit.year);
          setTmdb(resolvedTmdb);
          if (resolvedYear) setYear(resolvedYear);
          if (hit.posterUrl) setSelectedPoster(hit.posterUrl);
        }
      } catch {
        // Fall through to validation
      } finally {
        setBusy(false);
      }
    }

    if (
      !title().trim() ||
      !/^\d+$/.test(resolvedTmdb) ||
      !Number.isSafeInteger(Number(resolvedTmdb)) ||
      Number(resolvedTmdb) < 1 ||
      (resolvedYear && (!/^\d{4}$/.test(resolvedYear) || Number(resolvedYear) < 1800))
    ) {
      setError(
        "Enter a title, positive TMDB ID (or click Search TMDB) and optional four-digit year.",
      );
      return;
    }

    setBusy(true);
    try {
      const result = await client.POST("/api/v1/requests", {
        body: {
          title: title().trim(),
          kind: kind(),
          tmdbId: Number(resolvedTmdb),
          year: resolvedYear ? Number(resolvedYear) : undefined,
        },
      });
      if (result.error || !result.data)
        throw new Error("Request could not be submitted. Your entries have been kept.");
      setRequests((previous) => [result.data, ...previous]);
      setTitle("");
      setTmdb("");
      setYear("");
      setSelectedPoster(null);
      setSearchResults([]);
      setNotice("Request submitted for review.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Submission failed.");
    } finally {
      setBusy(false);
    }
  };

  const update = async (request: MediaRequest, status: string) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PATCH("/api/v1/requests/{id}/status", {
        params: { path: { id: request.id } },
        body: { status },
      });
      if (result.error) throw new Error("Status could not be saved.");
      setRequests((rows) => rows.map((row) => (row.id === request.id ? { ...row, status } : row)));
      setNotice("Status saved.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Status update failed.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ "max-width": "900px", margin: "0 auto", padding: "16px 0" }}>
      <h1 style={{ "margin-bottom": "6px" }}>Media Requests</h1>
      <p style={{ color: "var(--text-secondary)", "margin-bottom": "20px" }}>
        Request a movie or TV show. Search TMDB directly by title or enter a known TMDB ID. Approval
        records the decision for automated library acquisition.
      </p>

      <form
        onSubmit={submit}
        style={{
          display: "grid",
          gap: "14px",
          background: "var(--bg-surface)",
          padding: "20px",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-md)",
          "margin-bottom": "24px",
        }}
      >
        <div>
          <label
            for="request-title"
            style={{
              display: "block",
              "font-weight": "600",
              "font-size": "13px",
              "margin-bottom": "6px",
            }}
          >
            Title
          </label>
          <div style={{ display: "flex", gap: "8px" }}>
            <input
              id="request-title"
              required
              maxlength={300}
              disabled={busy() || searching()}
              value={title()}
              onInput={(event) => setTitle(event.currentTarget.value)}
              placeholder="e.g. Inception or Severance"
              style={{
                flex: 1,
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            />
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={busy() || searching() || !title().trim()}
              onClick={searchTmdb}
            >
              {searching() ? "Searching…" : "🔍 Search TMDB"}
            </Button>
          </div>
        </div>

        {/* TMDB Search Results Dropdown/Grid */}
        <Show when={searchResults().length > 0}>
          <div
            style={{
              background: "var(--bg-base)",
              border: "1px solid var(--accent)",
              "border-radius": "var(--radius-md)",
              padding: "12px",
              "max-height": "280px",
              overflow: "auto",
            }}
          >
            <div
              style={{
                "font-size": "12px",
                "font-weight": "700",
                "margin-bottom": "8px",
                color: "var(--accent)",
              }}
            >
              Select a matching title from TMDB:
            </div>
            <div style={{ display: "grid", gap: "8px" }}>
              <For each={searchResults()}>
                {(hit) => (
                  <button
                    type="button"
                    onClick={() => selectHit(hit)}
                    style={{
                      display: "flex",
                      gap: "10px",
                      "align-items": "center",
                      padding: "8px",
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-sm)",
                      cursor: "pointer",
                      "text-align": "left",
                    }}
                  >
                    <Show when={hit.posterUrl}>
                      <img
                        src={hit.posterUrl!}
                        alt={hit.title}
                        style={{
                          width: "32px",
                          height: "48px",
                          "object-fit": "cover",
                          "border-radius": "2px",
                        }}
                      />
                    </Show>
                    <div style={{ flex: 1 }}>
                      <div style={{ "font-weight": "600", "font-size": "13px" }}>
                        {hit.title} {hit.year ? `(${hit.year})` : ""}
                      </div>
                      <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                        {hit.kind === "show" ? "TV Show" : "Movie"} · TMDB ID: {hit.id}
                      </div>
                    </div>
                  </button>
                )}
              </For>
            </div>
          </div>
        </Show>

        <div
          style={{
            display: "grid",
            "grid-template-columns": "repeat(auto-fit, minmax(160px, 1fr))",
            gap: "12px",
          }}
        >
          <div>
            <label
              for="request-kind"
              style={{
                display: "block",
                "font-weight": "600",
                "font-size": "13px",
                "margin-bottom": "6px",
              }}
            >
              Media type
            </label>
            <select
              id="request-kind"
              disabled={busy()}
              value={kind()}
              onChange={(event) => setKind(event.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            >
              <option value="movie">Movie</option>
              <option value="show">TV show</option>
            </select>
          </div>

          <div>
            <label
              for="request-tmdb"
              style={{
                display: "block",
                "font-weight": "600",
                "font-size": "13px",
                "margin-bottom": "6px",
              }}
            >
              TMDB ID (optional if searching)
            </label>
            <input
              id="request-tmdb"
              inputmode="numeric"
              disabled={busy()}
              value={tmdb()}
              onInput={(event) => setTmdb(event.currentTarget.value)}
              placeholder="e.g. 157336"
              style={{
                width: "100%",
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            />
          </div>

          <div>
            <label
              for="request-year"
              style={{
                display: "block",
                "font-weight": "600",
                "font-size": "13px",
                "margin-bottom": "6px",
              }}
            >
              Year (optional)
            </label>
            <input
              id="request-year"
              inputmode="numeric"
              disabled={busy()}
              value={year()}
              onInput={(event) => setYear(event.currentTarget.value)}
              placeholder="e.g. 2024"
              style={{
                width: "100%",
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            />
          </div>
        </div>

        <Show when={selectedPoster()}>
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <img
              src={selectedPoster()!}
              alt="Selected Poster"
              style={{
                width: "40px",
                height: "60px",
                "object-fit": "cover",
                "border-radius": "4px",
              }}
            />
            <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
              Selected from TMDB metadata
            </span>
          </div>
        </Show>

        <div style={{ "margin-top": "6px" }}>
          <Button type="submit" variant="primary" disabled={busy()}>
            {busy() ? "Submitting…" : "Submit Request"}
          </Button>
        </div>
      </form>

      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)", "margin-bottom": "14px" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ color: "var(--success)", "margin-bottom": "14px" }}>
          {notice()}
        </p>
      </Show>

      <div
        style={{
          display: "flex",
          "justify-content": "space-between",
          "align-items": "center",
          "margin-bottom": "14px",
        }}
      >
        <h2 style={{ margin: 0, "font-size": "18px" }}>
          {user()?.isAdmin ? "All Requests" : "Your Requests"}
        </h2>
        <Button variant="secondary" size="sm" disabled={loading() || busy()} onClick={load}>
          Refresh
        </Button>
      </div>

      <Show when={!loading()} fallback={<p role="status">Loading requests…</p>}>
        <Show when={requests().length} fallback={<p>No requests found.</p>}>
          <ul style={{ "list-style": "none", padding: 0, display: "grid", gap: "10px" }}>
            <For each={requests()}>
              {(request) => (
                <li
                  style={{
                    padding: "14px 18px",
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-md)",
                    display: "flex",
                    "justify-content": "space-between",
                    "align-items": "center",
                  }}
                >
                  <div>
                    <div style={{ "font-weight": "700", "font-size": "15px" }}>
                      {request.title} {request.year ? `(${request.year})` : ""}
                    </div>
                    <div
                      style={{
                        "font-size": "12px",
                        color: "var(--text-muted)",
                        "margin-top": "2px",
                      }}
                    >
                      {request.kind === "show" ? "TV Show" : "Movie"} · TMDB ID: {request.tmdbId} ·
                      Status:{" "}
                      <span
                        style={{
                          "font-weight": "600",
                          color:
                            request.status === "available"
                              ? "var(--success)"
                              : request.status === "approved"
                                ? "var(--accent)"
                                : "var(--text-secondary)",
                        }}
                      >
                        {request.status}
                      </span>
                    </div>
                  </div>
                  <Show when={user()?.isAdmin}>
                    <div style={{ display: "flex", gap: "8px" }}>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy() || request.status === "approved"}
                        onClick={() => update(request, "approved")}
                      >
                        Approve
                      </Button>
                      <Button
                        size="sm"
                        variant="primary"
                        disabled={busy() || request.status === "available"}
                        onClick={() => update(request, "available")}
                      >
                        Mark available
                      </Button>
                    </div>
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </div>
  );
}

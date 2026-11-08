import { A } from "@solidjs/router";
import { createEffect, createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";

type Entry = { id: number; name: string; itemCount: number; userId?: number; isPublic?: boolean };
type Item = components["schemas"]["ItemDetail"];
type Hit = components["schemas"]["ItemSummary"];
export default function Collections() {
  const { user } = useAuth();
  const [kind, setKind] = createSignal<"collections" | "playlists">("collections");
  const [entries, setEntries] = createSignal<Entry[]>([]);
  const [selected, setSelected] = createSignal<Entry | null>(null);
  const [ids, setIds] = createSignal<number[]>([]);
  const [items, setItems] = createSignal<Item[]>([]);
  const [name, setName] = createSignal("");
  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<Hit[]>([]);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  let version = 0;
  const editable = () =>
    !!selected() &&
    (user()?.isAdmin || (kind() === "playlists" && selected()?.userId === user()?.id));
  const run = async (action: () => Promise<void>) => {
    setError("");
    setBusy(true);
    try {
      await action();
    } catch {
      setError("The request failed. Please retry.");
    } finally {
      setBusy(false);
    }
  };
  const loadList = async () => {
    const current = kind();
    const result =
      current === "collections"
        ? await client.GET("/api/v1/collections")
        : await client.GET("/api/v1/playlists");
    if (result.error || !result.data) throw new Error();
    if (kind() === current) setEntries(result.data);
  };
  createEffect(() => {
    kind();
    version++;
    setSelected(null);
    setItems([]);
    setIds([]);
    setHits([]);
    setName("");
    void run(loadList);
  });
  const showEntry = (entry: Entry) =>
    run(async () => {
      const current = kind();
      const request = ++version;
      setSelected(entry);
      setName(entry.name);
      setItems([]);
      setHits([]);
      setIds([]);
      const result =
        current === "collections"
          ? await client.GET("/api/v1/collections/{id}/items", {
              params: { path: { id: entry.id } },
            })
          : await client.GET("/api/v1/playlists/{id}/items", {
              params: { path: { id: entry.id } },
            });
      if (result.error || !result.data) throw new Error();
      if (request !== version) return;
      setIds(result.data);
      await loadItemPage(result.data.slice(0, 50), request);
    });
  const loadItemPage = async (page: number[], request: number) => {
    const loaded: Item[] = [];
    for (const id of page) {
      const result = await client.GET("/api/v1/items/{id}", { params: { path: { id } } });
      if (request !== version) return;
      if (result.error || !result.data) throw new Error();
      loaded.push(result.data);
    }
    if (request === version) setItems((previous) => [...previous, ...loaded]);
  };
  const create = () =>
    run(async () => {
      if (!name().trim()) {
        setError("Enter a name.");
        return;
      }
      const result =
        kind() === "collections"
          ? await client.POST("/api/v1/collections", { body: { name: name().trim() } })
          : await client.POST("/api/v1/playlists", {
              body: { name: name().trim(), isPublic: false },
            });
      if (result.error || !result.data) throw new Error();
      setName("");
      await loadList();
    });
  const rename = () =>
    run(async () => {
      const entry = selected();
      if (!entry || !name().trim()) return;
      const result =
        kind() === "collections"
          ? await client.PATCH("/api/v1/collections/{id}", {
              params: { path: { id: entry.id } },
              body: { name: name().trim() },
            })
          : await client.PATCH("/api/v1/playlists/{id}", {
              params: { path: { id: entry.id } },
              body: { name: name().trim(), isPublic: entry.isPublic ?? false },
            });
      if (result.error) throw new Error();
      setSelected({ ...entry, name: name().trim() });
      await loadList();
    });
  const remove = () =>
    run(async () => {
      const entry = selected();
      if (!entry || !confirm(`Delete '${entry.name}'? Media files will remain in the library.`))
        return;
      const result =
        kind() === "collections"
          ? await client.DELETE("/api/v1/collections/{id}", { params: { path: { id: entry.id } } })
          : await client.DELETE("/api/v1/playlists/{id}", { params: { path: { id: entry.id } } });
      if (result.error) throw new Error();
      version++;
      setSelected(null);
      setItems([]);
      setIds([]);
      setName("");
      await loadList();
    });
  const search = () =>
    run(async () => {
      const term = query().trim();
      if (!term) {
        setHits([]);
        return;
      }
      const result = await client.GET("/api/v1/search", { params: { query: { q: term } } });
      if (result.error || !result.data) throw new Error();
      setHits(result.data.flatMap((group) => group.items));
    });
  const membership = (itemId: number, add: boolean) =>
    run(async () => {
      const entry = selected();
      if (!entry) return;
      const params = { params: { path: { id: entry.id, itemId } } };
      const result =
        kind() === "collections"
          ? add
            ? await client.POST("/api/v1/collections/{id}/items/{itemId}", params)
            : await client.DELETE("/api/v1/collections/{id}/items/{itemId}", params)
          : add
            ? await client.POST("/api/v1/playlists/{id}/items/{itemId}", params)
            : await client.DELETE("/api/v1/playlists/{id}/items/{itemId}", params);
      if (result.error) throw new Error();
      await showEntry(entry);
      await loadList();
    });
  return (
    <div class="mbl-page" style={{ display: "grid", gap: "20px", "max-width": "1000px" }}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "flex-wrap": "wrap",
          gap: "12px",
        }}
      >
        <h1 style={{ margin: 0, "font-size": "24px", "font-weight": "700" }}>
          Collections & playlists
        </h1>
        <label
          style={{
            display: "inline-flex",
            "align-items": "center",
            gap: "10px",
            "font-size": "var(--font-size-sm)",
            "font-weight": "var(--font-weight-semibold)",
            color: "var(--text-secondary)",
          }}
        >
          <span>View</span>
          <select
            disabled={busy()}
            value={kind()}
            onChange={(e) => setKind(e.currentTarget.value as "collections" | "playlists")}
          >
            <option value="collections">Collections</option>
            <option value="playlists">Playlists</option>
          </select>
        </label>
      </div>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={busy()}>
        <p role="status">Loading…</p>
      </Show>
      <div class="mbl-card-section">
        <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
          <For
            each={entries()}
            fallback={<p style={{ margin: 0, color: "var(--text-muted)" }}>No entries yet.</p>}
          >
            {(entry) => (
              <Button
                variant={selected()?.id === entry.id ? "primary" : "secondary"}
                size="sm"
                disabled={busy()}
                onClick={() => showEntry(entry)}
              >
                {entry.name} ({entry.itemCount})
              </Button>
            )}
          </For>
        </div>
      </div>
      <Show when={selected()}>
        {(entry) => (
          <section class="mbl-card-section">
            <h2 style={{ margin: "0 0 12px", "font-size": "18px" }}>{entry().name}</h2>
            <For
              each={items()}
              fallback={<p style={{ color: "var(--text-muted)" }}>No items loaded.</p>}
            >
              {(item) => (
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    "justify-content": "space-between",
                    gap: "12px",
                    padding: "10px 0",
                    "border-bottom": "1px solid var(--border)",
                  }}
                >
                  <A
                    href={`/item/${item.id}`}
                    style={{ "font-weight": "var(--font-weight-medium)", color: "var(--accent)" }}
                  >
                    {item.title}
                  </A>
                  <Show when={editable()}>
                    <Button
                      variant="danger"
                      size="sm"
                      disabled={busy()}
                      onClick={() => membership(item.id, false)}
                    >
                      Remove
                    </Button>
                  </Show>
                </div>
              )}
            </For>
            <Show when={items().length < ids().length}>
              <div style={{ "margin-top": "12px" }}>
                <Button
                  variant="secondary"
                  disabled={busy()}
                  onClick={() =>
                    run(() =>
                      loadItemPage(ids().slice(items().length, items().length + 50), version),
                    )
                  }
                >
                  Load more items
                </Button>
              </div>
            </Show>
          </section>
        )}
      </Show>
      <Show when={editable() || (!selected() && (kind() === "playlists" || user()?.isAdmin))}>
        <form
          class="mbl-card-section"
          style={{
            display: "flex",
            "align-items": "flex-end",
            "flex-wrap": "wrap",
            gap: "10px",
          }}
          onSubmit={(e) => {
            e.preventDefault();
            void (selected() ? rename() : create());
          }}
        >
          <label
            style={{
              display: "flex",
              "flex-direction": "column",
              gap: "6px",
              flex: "1 1 220px",
              "font-size": "var(--font-size-xs)",
              "font-weight": "var(--font-weight-semibold)",
              color: "var(--text-secondary)",
            }}
          >
            <span>Name</span>
            <input
              required
              maxlength={200}
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
            />
          </label>
          <Button type="submit" disabled={busy()}>
            {selected() ? "Rename" : "Create"}
          </Button>
          <Show when={selected()}>
            <Button variant="danger" disabled={busy()} onClick={remove}>
              Delete
            </Button>
            <Button
              variant="secondary"
              onClick={() => {
                version++;
                setSelected(null);
                setName("");
              }}
            >
              New entry
            </Button>
          </Show>
        </form>
      </Show>
      <Show when={editable()}>
        <div class="mbl-card-section" style={{ display: "grid", gap: "12px" }}>
          <form
            style={{
              display: "flex",
              "align-items": "flex-end",
              "flex-wrap": "wrap",
              gap: "10px",
            }}
            onSubmit={(e) => {
              e.preventDefault();
              void search();
            }}
          >
            <label
              style={{
                display: "flex",
                "flex-direction": "column",
                gap: "6px",
                flex: "1 1 220px",
                "font-size": "var(--font-size-xs)",
                "font-weight": "var(--font-weight-semibold)",
                color: "var(--text-secondary)",
              }}
            >
              <span>Find media</span>
              <input
                value={query()}
                maxlength={200}
                onInput={(e) => setQuery(e.currentTarget.value)}
              />
            </label>
            <Button type="submit" variant="secondary" disabled={busy()}>
              Search
            </Button>
          </form>
          <For each={hits()}>
            {(hit) => (
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "space-between",
                  gap: "10px",
                  padding: "8px 0",
                  "border-top": "1px solid var(--border)",
                }}
              >
                <span>{hit.title}</span>
                <Button
                  size="sm"
                  disabled={busy() || ids().includes(hit.id)}
                  onClick={() => membership(hit.id, true)}
                >
                  Add
                </Button>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

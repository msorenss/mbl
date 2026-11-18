import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import { Badge, Button } from "./ui";

const RATINGS = ["G", "PG", "PG-13", "R", "NC-17"];

const label = {
  display: "block",
  "font-size": "13px",
  "font-weight": "600",
  "margin-bottom": "6px",
} as const;

/** Admin settings for the DLNA media server: on/off, name, shared libraries and age limit. */
export default function DlnaSettings() {
  const [enabled, setEnabled] = createSignal(false);
  const [name, setName] = createSignal("");
  const [shared, setShared] = createSignal<number[]>([]);
  const [rating, setRating] = createSignal("");
  const [libraries, setLibraries] = createSignal<{ id: number; name: string }[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");

  onMount(async () => {
    const [settings, libs] = await Promise.all([
      client.GET("/api/v1/system/settings"),
      client.GET("/api/v1/libraries"),
    ]);
    if (!settings.data || !libs.data) {
      setError("Could not load DLNA settings.");
      return;
    }
    setEnabled(settings.data.dlnaEnabled === true);
    setName(settings.data.dlnaFriendlyName ?? "");
    setShared(settings.data.dlnaLibraryIds ?? []);
    setRating(settings.data.dlnaMaxRating ?? "");
    setLibraries(libs.data.map((l) => ({ id: l.id, name: l.name })));
    setLoaded(true);
  });

  const toggle = (id: number, on: boolean) =>
    setShared((ids) => (on ? [...ids, id] : ids.filter((x) => x !== id)));

  const active = () => enabled() && shared().length > 0;

  const save = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PATCH("/api/v1/system/settings", {
        body: {
          dlnaEnabled: enabled(),
          dlnaFriendlyName: name().trim() || undefined,
          dlnaLibraryIds: shared(),
          dlnaMaxRating: rating() || undefined,
        },
      });
      if (!result.data) {
        const message = (result.error as { error?: { message?: string } } | undefined)?.error
          ?.message;
        throw new Error(message?.replace(/^validation: /, "") || "Could not save DLNA settings.");
      }
      setShared(result.data.dlnaLibraryIds ?? []);
      setNotice(
        !result.data.dlnaEnabled
          ? "Saved. DLNA is turned off."
          : (result.data.dlnaLibraryIds ?? []).length === 0
            ? "Saved. Nothing is shared until you choose a library."
            : "Saved. TVs on your network can now find the server.",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save DLNA settings.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      style={{
        background: "var(--bg-surface)",
        border: "1px solid var(--border)",
        "border-radius": "var(--radius-lg)",
        padding: "24px",
        "margin-bottom": "24px",
      }}
    >
      <div
        style={{ display: "flex", "align-items": "center", gap: "10px", "margin-bottom": "12px" }}
      >
        <span style={{ "font-size": "24px" }}>📺</span>
        <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>DLNA for TVs</h2>
        <Badge variant={active() ? "success" : "default"}>
          {active() ? "Shared on the network" : "Off"}
        </Badge>
      </div>
      <p
        style={{
          "font-size": "13px",
          color: "var(--text-secondary)",
          "line-height": "1.6",
          "margin-bottom": "16px",
        }}
      >
        Smart TVs and other DLNA players on your home network can browse and play the libraries you
        choose here. DLNA has no sign-in, so everyone on the network sees them. Files are played as
        they are; if a TV cannot play a format, use the MBL app or Cast instead.
      </p>
      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)", margin: "0 0 12px" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ margin: "0 0 12px" }}>
          {notice()}
        </p>
      </Show>
      <Show when={loaded()}>
        <form onSubmit={save} style={{ display: "grid", gap: "16px" }}>
          <label style={{ display: "flex", gap: "8px", "align-items": "center" }}>
            <input
              type="checkbox"
              checked={enabled()}
              onChange={(e) => setEnabled(e.currentTarget.checked)}
            />
            <span style={{ "font-size": "14px", "font-weight": "600" }}>Share over DLNA</span>
          </label>
          <div>
            <label for="dlna-name" style={label}>
              Name shown on TVs
            </label>
            <input
              id="dlna-name"
              type="text"
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              placeholder="Media Box Lounge"
            />
          </div>
          <fieldset style={{ border: "none", padding: 0, margin: 0 }}>
            <legend style={label}>Shared libraries</legend>
            <Show
              when={libraries().length > 0}
              fallback={<p style={{ margin: 0 }}>There are no libraries yet.</p>}
            >
              <div style={{ display: "grid", gap: "6px" }}>
                <For each={libraries()}>
                  {(lib) => (
                    <label style={{ display: "flex", gap: "8px", "align-items": "center" }}>
                      <input
                        type="checkbox"
                        checked={shared().includes(lib.id)}
                        onChange={(e) => toggle(lib.id, e.currentTarget.checked)}
                      />
                      <span>{lib.name}</span>
                    </label>
                  )}
                </For>
              </div>
            </Show>
            <Show when={enabled() && shared().length === 0}>
              <p style={{ margin: "8px 0 0", color: "var(--warning, var(--text-secondary))" }}>
                Choose at least one library. Nothing is shared until you do.
              </p>
            </Show>
          </fieldset>
          <div>
            <label for="dlna-rating" style={label}>
              Maximum content rating
            </label>
            <select
              id="dlna-rating"
              value={rating()}
              onChange={(e) => setRating(e.currentTarget.value)}
            >
              <option value="">No limit</option>
              <For each={[...new Set([...RATINGS, ...(rating() ? [rating()] : [])])]}>
                {(r) => <option value={r}>{r}</option>}
              </For>
            </select>
          </div>
          <p style={{ margin: 0, "font-size": "12px", color: "var(--text-secondary)" }}>
            TVs find the server on UDP port 1900 and play over the server's HTTP port. Only devices
            on the local network are answered.
          </p>
          <div>
            <Button type="submit" variant="primary" size="md" disabled={busy()}>
              {busy() ? "Saving…" : "Save DLNA settings"}
            </Button>
          </div>
        </form>
      </Show>
    </div>
  );
}

import { A } from "@solidjs/router";
import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { HubRow } from "../components/HubRow";
import { useAuth } from "../context/AuthContext";

type Hub = components["schemas"]["Hub"];

export function Home() {
  const { user } = useAuth();
  const [hubs, setHubs] = createSignal<Hub[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [serverVersion, setServerVersion] = createSignal<string>("");
  const [libraryCount, setLibraryCount] = createSignal<number>(0);

  onMount(async () => {
    try {
      const [hubsRes, infoRes, libsRes] = await Promise.all([
        client.GET("/api/v1/hubs/home"),
        client.GET("/api/v1/system/info"),
        client.GET("/api/v1/libraries"),
      ]);

      if (hubsRes.data) setHubs(hubsRes.data);
      if (infoRes.data) setServerVersion(infoRes.data.version);
      if (libsRes.data) setLibraryCount(libsRes.data.length);
    } catch {
      // Ignore
    } finally {
      setLoading(false);
    }
  });

  const hasAnyItems = () => hubs().some((h) => h.items && h.items.length > 0);

  return (
    <div>
      {/* Top Welcome / Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "28px",
        }}
      >
        <div>
          <h1 style={{ margin: "0 0 6px", "font-size": "26px", "font-weight": "800" }}>
            Welcome back, {user()?.displayName || user()?.username}!
          </h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            MBL Media Server v{serverVersion() || "0.1.0"}
          </p>
        </div>

        <Show when={user()?.isAdmin}>
          <div style={{ display: "flex", gap: "10px" }}>
            <A
              href="/admin/ai-agents"
              style={{
                display: "inline-flex",
                "align-items": "center",
                gap: "6px",
                padding: "8px 14px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                "font-size": "13px",
                "font-weight": "600",
                color: "var(--text-primary)",
              }}
            >
              🤖 AI Agents
            </A>
          </div>
        </Show>
      </div>

      {/* Hubs or Empty State */}
      <Show
        when={!loading()}
        fallback={
          <div style={{ padding: "40px 0", color: "var(--text-muted)", "text-align": "center" }}>
            Loading home media hubs...
          </div>
        }
      >
        <Show
          when={hasAnyItems()}
          fallback={
            <div
              style={{
                padding: "48px 32px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-lg)",
                "text-align": "center",
                "max-width": "540px",
                margin: "40px auto 0",
              }}
            >
              <div style={{ "font-size": "40px", "margin-bottom": "14px" }}>🎬</div>
              <h2 style={{ margin: "0 0 8px", "font-size": "20px", "font-weight": "700" }}>
                Your media library is empty
              </h2>
              <p
                style={{ margin: "0 0 24px", color: "var(--text-secondary)", "font-size": "14px" }}
              >
                {libraryCount() === 0
                  ? "Get started by creating your first media library and pointing it to your movies or TV shows."
                  : "Libraries are configured, but no scanned media is present yet. Run a library scan to populate your media!"}
              </p>
              <A
                href="/libraries"
                style={{
                  display: "inline-block",
                  padding: "10px 22px",
                  background: "var(--accent)",
                  color: "#fff",
                  "border-radius": "var(--radius-sm)",
                  "font-weight": "600",
                  "font-size": "14px",
                }}
              >
                Go to Libraries &rarr;
              </A>
            </div>
          }
        >
          {/* Active Hub Rows */}
          <For each={hubs()}>{(hub) => <HubRow hub={hub} />}</For>
        </Show>
      </Show>
    </div>
  );
}
export default Home;

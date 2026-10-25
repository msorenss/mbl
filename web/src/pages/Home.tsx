import { A } from "@solidjs/router";
import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { PosterCard } from "../components/PosterCard";
import { Hero, HubRow, LandscapeCard, Skeleton } from "../components/ui";
import { usePlayer } from "../context/PlayerContext";
import { useI18n } from "../i18n";

type Hub = components["schemas"]["Hub"];
type ItemSummary = components["schemas"]["ItemSummary"];

export function Home() {
  const { playItem } = usePlayer();
  const { t } = useI18n();

  const [hubs, setHubs] = createSignal<Hub[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [featuredItem, setFeaturedItem] = createSignal<ItemSummary | null>(null);

  const loadData = async () => {
    try {
      const hubsRes = await client.GET("/api/v1/hubs/home");
      if (hubsRes.data) {
        const loadedHubs = hubsRes.data as Hub[];
        setHubs(loadedHubs);

        // Pick featured item from continue watching, up next, or first recently added
        const cw = loadedHubs.find((h) => h.key === "continueWatching");
        const upNext = loadedHubs.find((h) => h.key === "upNext");
        const recent = loadedHubs.find((h) => h.key.startsWith("recentlyAdded"));

        const cand = cw?.items?.[0] || upNext?.items?.[0] || recent?.items?.[0] || null;
        setFeaturedItem(cand);
      }
    } catch {
      // Ignore
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    loadData();
  });

  const handleMoveHub = async (index: number, direction: "up" | "down") => {
    const list = [...hubs()];
    const targetIdx = direction === "up" ? index - 1 : index + 1;
    if (targetIdx < 0 || targetIdx >= list.length) return;

    // Swap
    const temp = list[index];
    list[index] = list[targetIdx];
    list[targetIdx] = temp;
    setHubs(list);

    // Persist new layout order
    const hubOrder = list.map((h) => h.key);
    try {
      await client.PUT("/api/v1/users/me/home-layout", {
        body: { homeLayout: hubOrder },
      });
    } catch {
      // Ignore
    }
  };

  const hasAnyItems = () => hubs().some((h) => h.items && h.items.length > 0);

  return (
    <div style={{ "max-width": "1600px", margin: "0 auto" }}>
      {/* Loading Skeletons */}
      <Show when={loading()}>
        <Skeleton height="380px" borderRadius="var(--radius-lg)" />
        <div style={{ "margin-top": "var(--space-8)" }}>
          <Skeleton width="200px" height="24px" />
          <div style={{ display: "flex", gap: "var(--space-4)", "margin-top": "var(--space-4)" }}>
            <For each={[1, 2, 3, 4, 5]}>
              {() => <Skeleton width="180px" height="270px" borderRadius="var(--radius-md)" />}
            </For>
          </div>
        </div>
      </Show>

      <Show when={!loading()}>
        {/* Featured Hero Spotlight */}
        <Show when={featuredItem()}>
          {(item) => (
            <Hero
              title={item().title}
              year={item().year || undefined}
              durationMs={item().durationMs || undefined}
              backdropUrl={item().posterId ? `/api/v1/images/${item().posterId}?w=1080` : undefined}
              detailsHref={`/item/${item().id}`}
              onPlay={() =>
                playItem({
                  itemId: item().id,
                  initialStartMs: Math.max(0, item().viewOffsetMs),
                  title: item().title,
                  subtitle: item().year ? String(item().year) : undefined,
                })
              }
            />
          )}
        </Show>

        {/* Hubs Content or Empty State */}
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
              <div style={{ "font-size": "44px", "margin-bottom": "14px" }}>🎬</div>
              <h2 style={{ margin: "0 0 8px", "font-size": "20px", "font-weight": "700" }}>
                Your media library is empty
              </h2>
              <p
                style={{ margin: "0 0 24px", color: "var(--text-secondary)", "font-size": "14px" }}
              >
                Create your first media library and run a scan to start watching movies and series.
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
          <For each={hubs()}>
            {(hub, index) => {
              const isContinue = () => hub.key === "continueWatching";
              const isUpNext = () => hub.key === "upNext";
              const hubTitle = () => {
                if (isContinue()) return t("continueWatching");
                if (isUpNext()) return t("upNext");
                return hub.title;
              };

              return (
                <HubRow
                  title={hubTitle()}
                  canMoveUp={index() > 0}
                  canMoveDown={index() < hubs().length - 1}
                  onMoveUp={() => handleMoveHub(index(), "up")}
                  onMoveDown={() => handleMoveHub(index(), "down")}
                  linkHref={hub.libraryId ? `/library/${hub.libraryId}` : undefined}
                >
                  <For each={hub.items}>
                    {(item) => {
                      if (isContinue() || isUpNext()) {
                        return (
                          <LandscapeCard
                            item={item}
                            badge={isContinue() ? "Resume" : isUpNext() ? "Up Next" : undefined}
                          />
                        );
                      }
                      return <PosterCard item={item} />;
                    }}
                  </For>
                </HubRow>
              );
            }}
          </For>
        </Show>
      </Show>
    </div>
  );
}

export default Home;

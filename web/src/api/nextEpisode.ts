import client from "./client";
import type { components } from "./schema";

type Episode = Pick<components["schemas"]["ItemSummary"], "id" | "title">;

// All lookups use authenticated item/children APIs, preserving library and rating filters.
export async function findNextEpisode(
  itemId: number,
  signal?: AbortSignal,
): Promise<Episode | null> {
  const current = await client.GET("/api/v1/items/{id}", {
    signal,
    params: { path: { id: itemId } },
  });
  if (current.error || !current.data) throw new Error("Could not load episode details.");
  const item = current.data;
  if (item.kind !== "episode" || item.parentId == null || item.idx == null) return null;
  const siblings = await client.GET("/api/v1/items/{id}/children", {
    signal,
    params: { path: { id: item.parentId } },
  });
  if (siblings.error || !siblings.data) throw new Error("Could not load next episode.");
  const next = siblings.data
    .filter(
      (entry) => entry.kind === "episode" && entry.idx != null && entry.idx > (item.idx as number),
    )
    .sort((a, b) => (a.idx as number) - (b.idx as number))[0];
  if (next) return next;
  const season = await client.GET("/api/v1/items/{id}", {
    signal,
    params: { path: { id: item.parentId } },
  });
  if (season.error || !season.data) throw new Error("Could not load season details.");
  if (season.data.kind !== "season" || season.data.parentId == null || season.data.idx == null)
    return null;
  const seasons = await client.GET("/api/v1/items/{id}/children", {
    signal,
    params: { path: { id: season.data.parentId } },
  });
  if (seasons.error || !seasons.data) throw new Error("Could not load seasons.");
  const following = seasons.data
    .filter(
      (entry) =>
        entry.kind === "season" && entry.idx != null && entry.idx > (season.data?.idx as number),
    )
    .sort((a, b) => (a.idx as number) - (b.idx as number));
  for (const entry of following) {
    const episodes = await client.GET("/api/v1/items/{id}/children", {
      signal,
      params: { path: { id: entry.id } },
    });
    if (episodes.error || !episodes.data) throw new Error("Could not load next season.");
    const first = episodes.data
      .filter((episode) => episode.kind === "episode" && episode.idx != null)
      .sort((a, b) => (a.idx as number) - (b.idx as number))[0];
    if (first) return first;
  }
  return null;
}

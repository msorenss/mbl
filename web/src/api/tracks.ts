import type { PlayRequest } from "../context/PlayerContext";
import client from "./client";

/** All tracks of an album or artist in play order, as queue entries. */
export async function collectTracks(itemId: number, kind: string): Promise<PlayRequest[]> {
  const children = async (id: number) => {
    const result = await client.GET("/api/v1/items/{id}/children", {
      params: { path: { id } },
    });
    if (!result.data) throw new Error("Could not load tracks.");
    return [...result.data].sort((a, b) => (a.idx ?? 0) - (b.idx ?? 0));
  };
  const albums =
    kind === "artist" ? (await children(itemId)).filter((c) => c.kind === "album") : [];
  const groups =
    kind === "artist"
      ? await Promise.all(
          albums.map(async (a) => ({ album: a.title, tracks: await children(a.id) })),
        )
      : [{ album: undefined, tracks: await children(itemId) }];
  return groups.flatMap((group) =>
    group.tracks
      .filter((t) => t.kind === "track")
      .map((t) => ({ itemId: t.id, title: t.title, subtitle: group.album, initialStartMs: 0 })),
  );
}

/** A random order; `random` is injectable for tests. */
export function shuffled<T>(list: T[], random = Math.random): T[] {
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

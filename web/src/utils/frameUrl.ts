/** Something with a video to take a picture from. */
interface Playable {
  id: number;
  kind: string;
  durationMs?: number | null;
  viewOffsetMs?: number | null;
}

/**
 * A picture from the video itself, for titles without artwork: where the viewer stopped, or
 * else a little way in (10 %, at most 5 minutes) so it is not the black opening frame.
 * Rounded to 10 s like the server's cache. `null` for things without one video.
 */
export function frameUrl(item: Playable, useResume = true): string | null {
  if (!["episode", "movie", "video"].includes(item.kind)) return null;
  const duration = item.durationMs ?? 0;
  if (duration <= 0) return null;
  const resume = useResume ? (item.viewOffsetMs ?? 0) : 0;
  const at = resume > 0 ? resume : Math.min(duration * 0.1, 300_000);
  return `/api/v1/items/${item.id}/snapshot?at=${Math.floor(at / 10_000) * 10_000}`;
}

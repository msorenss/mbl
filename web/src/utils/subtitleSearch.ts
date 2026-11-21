import client from "../api/client";

let cached: Promise<boolean> | undefined;

/** Whether an admin has set up subtitle search (asked once per page load). */
export function subtitleSearchAvailable(): Promise<boolean> {
  cached ??= client
    .GET("/api/v1/subtitles/status")
    .then((r) => r.data?.available ?? false)
    .catch(() => false);
  return cached;
}

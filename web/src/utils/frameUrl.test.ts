import { expect, it } from "vitest";
import { frameUrl } from "./frameUrl";

it("takes a picture where the viewer stopped, or a little way in, for videos only", () => {
  const episode = { id: 3, kind: "episode", durationMs: 3_600_000, viewOffsetMs: 1_254_379 };
  expect(frameUrl(episode)).toBe("/api/v1/items/3/snapshot?at=1250000");
  // Not watched yet, or asked not to use the resume point: 10 %, at most 5 minutes.
  expect(frameUrl({ ...episode, viewOffsetMs: 0 })).toBe("/api/v1/items/3/snapshot?at=300000");
  expect(frameUrl(episode, false)).toBe("/api/v1/items/3/snapshot?at=300000");
  expect(frameUrl({ id: 4, kind: "movie", durationMs: 600_000 })).toBe(
    "/api/v1/items/4/snapshot?at=60000",
  );
  expect(frameUrl({ id: 5, kind: "show", durationMs: 600_000 })).toBeNull();
  expect(frameUrl({ id: 6, kind: "episode", durationMs: 0 })).toBeNull();
});

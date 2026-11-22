import { expect, it } from "vitest";
import { subtitleUrl } from "./subtitleUrl";

it("asks the server to match the speech only when chosen", () => {
  expect(subtitleUrl("/api/v1/stream/subtitles/1/2.vtt", false)).toBe(
    "/api/v1/stream/subtitles/1/2.vtt",
  );
  expect(subtitleUrl("/api/v1/stream/subtitles/1/2.vtt", true)).toBe(
    "/api/v1/stream/subtitles/1/2.vtt?sync=true",
  );
  expect(subtitleUrl("/api/v1/stream/subtitles/1/2.vtt?token=abc", true)).toBe(
    "/api/v1/stream/subtitles/1/2.vtt?token=abc&sync=true",
  );
});

import { describe, expect, it, vi } from "vitest";
import client from "../api/client";

describe("Player flow and playback negotiation", () => {
  it("negotiates playback session and reports progress", async () => {
    const postSpy = vi.spyOn(client, "POST").mockImplementation(async (path: string) => {
      if (path === "/api/v1/playback") {
        return {
          data: {
            sessionId: "test-sess-123",
            method: "directPlay",
            url: "/api/v1/stream/direct/456?token=abc",
            startMs: 0,
            durationMs: 3600000,
            reasons: ["all codecs supported"],
            subtitles: [
              {
                index: 0,
                codec: "subrip",
                format: "webvtt",
                language: "eng",
                title: "English",
                isDefault: true,
                isForced: false,
                url: "/api/v1/stream/subtitles/456/0.vtt",
              },
            ],
            markers: [],
          },
          error: undefined,
          response: new Response(),
        };
      }
      if (path === "/api/v1/playback/{session}/progress") {
        return {
          data: undefined,
          error: undefined,
          response: new Response(null, { status: 204 }),
        };
      }
      if (path === "/api/v1/playback/{session}/stop") {
        return {
          data: undefined,
          error: undefined,
          response: new Response(null, { status: 204 }),
        };
      }
      return { data: undefined, error: undefined, response: new Response() };
    });

    const playRes = await client.POST("/api/v1/playback", {
      body: { itemId: 12, profile: "chrome" as unknown as Record<string, never> },
    });

    expect(postSpy).toHaveBeenCalled();
    expect(playRes.data?.sessionId).toBe("test-sess-123");
    expect(playRes.data?.method).toBe("directPlay");
    expect(playRes.data?.subtitles).toHaveLength(1);

    const progRes = await client.POST("/api/v1/playback/{session}/progress", {
      params: { path: { session: "test-sess-123" } },
      body: { positionMs: 15000, state: "playing" },
    });
    expect(progRes.response.status).toBe(204);
  });
});

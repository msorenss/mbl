import { describe, expect, it, vi } from "vitest";
import client from "../api/client";

describe("Home page data loading", () => {
  it("fetches home hubs and system information", async () => {
    vi.spyOn(client, "GET").mockImplementation(async (path: string) => {
      if (path === "/api/v1/hubs/home") {
        return {
          data: [
            {
              key: "continueWatching",
              title: "Continue Watching",
              items: [
                {
                  id: 101,
                  libraryId: 1,
                  title: "Big Test Movie",
                  kind: "movie",
                  year: 2024,
                  viewCount: 0,
                  viewOffsetMs: 50000,
                  durationMs: 120000,
                  addedAt: 1000,
                },
              ],
            },
          ],
          error: undefined,
          response: new Response(),
        };
      }
      if (path === "/api/v1/system/info") {
        return {
          data: {
            name: "mbl",
            version: "0.1.0",
            os: "linux",
            arch: "x86_64",
            platformProfile: "desktop",
          },
          error: undefined,
          response: new Response(),
        };
      }
      return { data: undefined, error: undefined, response: new Response() };
    });

    const hubsRes = await client.GET("/api/v1/hubs/home");
    expect(hubsRes.data).toBeDefined();
    expect(hubsRes.data?.[0].title).toBe("Continue Watching");
    expect(hubsRes.data?.[0].items[0].viewOffsetMs).toBe(50000);

    const infoRes = await client.GET("/api/v1/system/info");
    expect(infoRes.data?.version).toBe("0.1.0");
  });
});

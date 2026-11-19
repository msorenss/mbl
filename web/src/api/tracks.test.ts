import { afterEach, expect, it, vi } from "vitest";
import client from "./client";
import { collectTracks, shuffled } from "./tracks";

afterEach(() => vi.restoreAllMocks());

it("collects an artist's tracks album by album in track order", async () => {
  const children: Record<number, unknown[]> = {
    1: [
      { id: 20, kind: "album", title: "Second", idx: 2 },
      { id: 10, kind: "album", title: "First", idx: 1 },
    ],
    10: [
      { id: 12, kind: "track", title: "B", idx: 2 },
      { id: 11, kind: "track", title: "A", idx: 1 },
    ],
    20: [{ id: 21, kind: "track", title: "C", idx: 1 }],
  };
  vi.spyOn(client, "GET").mockImplementation(((
    _: string,
    init: { params: { path: { id: number } } },
  ) =>
    Promise.resolve({ data: children[init.params.path.id], response: new Response() })) as never);
  const tracks = await collectTracks(1, "artist");
  expect(tracks.map((t) => t.itemId)).toEqual([11, 12, 21]);
  expect(tracks[2].subtitle).toBe("Second");
});

it("shuffles without losing entries", () => {
  const values = [1, 2, 3, 4, 5];
  const out = shuffled(values, () => 0);
  expect([...out].sort()).toEqual(values);
  expect(out).not.toEqual(values);
});

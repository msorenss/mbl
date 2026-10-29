import { afterEach, expect, it, vi } from "vitest";
import client from "./client";
import { findNextEpisode } from "./nextEpisode";

afterEach(() => vi.restoreAllMocks());
it("selects the next numbered episode rather than API array order", async () => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: String(path).endsWith("children")
          ? [
              { id: 4, kind: "episode", idx: 4, title: "Four" },
              { id: 2, kind: "episode", idx: 2, title: "Two" },
            ]
          : { id: 1, kind: "episode", idx: 1, parentId: 10 },
        response: new Response(),
      }) as never,
  );
  expect(await findNextEpisode(1)).toMatchObject({ id: 2, title: "Two" });
});
it("crosses seasons in order and skips empty seasons", async () => {
  const get = vi.spyOn(client, "GET").mockImplementation(async (path, options) => {
    const id = (options as { params: { path: { id: number } } }).params.path.id;
    const data = String(path).endsWith("children")
      ? id === 10
        ? []
        : id === 100
          ? [
              { id: 30, kind: "season", idx: 3 },
              { id: 20, kind: "season", idx: 2 },
            ]
          : id === 20
            ? []
            : [
                { id: 32, kind: "episode", idx: 2, title: "Second" },
                { id: 31, kind: "episode", idx: 1, title: "First" },
              ]
      : id === 1
        ? { id: 1, kind: "episode", idx: 8, parentId: 10 }
        : { id: 10, kind: "season", idx: 1, parentId: 100 };
    return { data, response: new Response() } as never;
  });
  expect(await findNextEpisode(1)).toMatchObject({ id: 31 });
  expect(get).toHaveBeenCalledTimes(6);
});
it("does not skip forward if an authorized lookup fails", async () => {
  const get = vi
    .spyOn(client, "GET")
    .mockResolvedValueOnce({
      data: { id: 1, kind: "episode", idx: 1, parentId: 10 },
      response: new Response(),
    } as never)
    .mockResolvedValueOnce({ error: {}, response: new Response(null, { status: 403 }) } as never);
  await expect(findNextEpisode(1)).rejects.toThrow("Could not load next episode");
  expect(get).toHaveBeenCalledTimes(2);
});
it("does not invent a next episode for a movie", async () => {
  const get = vi
    .spyOn(client, "GET")
    .mockResolvedValue({ data: { id: 1, kind: "movie" }, response: new Response() } as never);
  expect(await findNextEpisode(1)).toBeNull();
  expect(get).toHaveBeenCalledTimes(1);
});

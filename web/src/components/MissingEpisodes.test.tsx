import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import MissingEpisodes from "./MissingEpisodes";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text);

it("lists missing episodes per season and requests the show", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      showId: 2,
      title: "Dark Matter",
      tmdbId: 77,
      missingCount: 3,
      seasons: [
        {
          season: 1,
          name: "Season 1",
          whole: false,
          episodes: [{ episode: 3, title: "Pilot 3", airDate: "2024-05-08" }],
        },
        { season: 2, name: null, whole: true, episodes: [{ episode: 1 }, { episode: 2 }] },
      ],
    },
    response: new Response(),
  } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ data: { id: 1 }, response: new Response() } as never);
  dispose = render(() => createComponent(MissingEpisodes, { showId: 2 }), document.body);
  button("Check for missing episodes")?.click();
  await tick();
  const text = document.body.textContent ?? "";
  expect(text).toContain("Missing episodes (3)");
  expect(text).toContain("E3 · Pilot 3 · 2024-05-08");
  expect(text).toContain("Season 2 · the whole season");

  button("Request the show")?.click();
  await tick();
  expect(post.mock.calls[0][1]).toMatchObject({
    body: { kind: "show", tmdbId: 77, title: "Dark Matter" },
  });
  expect(document.querySelector("[role=status]")?.textContent).toContain("Requested");
});

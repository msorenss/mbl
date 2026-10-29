import { createComponent, onMount } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Photos from "./Photos";

vi.mock("@solidjs/router", () => ({ useParams: () => ({ id: "1" }) }));
vi.mock("../components/PhotoImage", () => ({
  default: (props: { title: string; onReady?: () => void }) => {
    onMount(() => props.onReady?.());
    return <img alt={props.title} />;
  },
}));
let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
it("paginates a month and navigates the real viewer without jumping past the final slide", async () => {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  let advance: () => void = () => {};
  vi.spyOn(globalThis, "setInterval").mockImplementation((callback, delay) => {
    if (delay === 5000) advance = callback as () => void;
    return 1 as never;
  });
  const get = vi.spyOn(client, "GET").mockImplementation(async (path, options) => {
    const query = (options as { params: { query?: { cursor?: string } } }).params.query;
    const data =
      path === "/api/v1/libraries/{id}"
        ? { id: 1, name: "Album", kind: "photo" }
        : path === "/api/v1/libraries/{id}/timeline"
          ? [{ year: 2024, month: 7, count: 2 }]
          : {
              items: [
                {
                  id: query?.cursor ? 2 : 1,
                  title: query?.cursor ? "Second" : "First",
                  partId: query?.cursor ? 2 : 1,
                  takenAt: 1721044800000,
                },
              ],
              nextCursor: query?.cursor ? null : "more",
            };
    return { data, response: new Response() } as never;
  });
  dispose = render(() => createComponent(Photos, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")]
    .find((button) => button.textContent === "Load more photos")
    ?.click();
  await tick();
  expect(get).toHaveBeenCalledWith(
    "/api/v1/libraries/{id}/photos",
    expect.objectContaining({
      params: expect.objectContaining({
        query: expect.objectContaining({ cursor: "more", year: 2024, month: 7 }),
      }),
    }),
  );
  [...document.querySelectorAll("button")]
    .find((button) => button.textContent === "First")
    ?.click();
  await tick();
  expect(document.querySelector('[role="dialog"] img')?.getAttribute("alt")).toBe("First");
  [...document.querySelectorAll("button")]
    .find((button) => button.textContent === "Start slideshow")
    ?.click();
  advance();
  await tick();
  expect(document.querySelector('[role="dialog"] img')?.getAttribute("alt")).toBe("Second");
  advance();
  await tick();
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Start slideshow");
});
it("shows a failed timeline request and leaves a retry available", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 403 }),
  } as never);
  dispose = render(() => createComponent(Photos, {}), document.body);
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain(
    "Could not load photo library",
  );
  expect(
    [...document.querySelectorAll("button")].some(
      (button) => button.textContent === "Refresh timeline" && !button.disabled,
    ),
  ).toBe(true);
});

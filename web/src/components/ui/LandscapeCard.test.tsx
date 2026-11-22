import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import { LandscapeCard } from "./LandscapeCard";

vi.mock("../../context/PlayerContext", () => ({ usePlayer: () => ({ playItem: vi.fn() }) }));
vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; children: unknown }) => props.children,
}));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

const item = {
  id: 3,
  libraryId: 1,
  kind: "episode",
  parentId: 2,
  title: "How to Build the Box",
  year: null,
  idx: 9,
  parentIdx: 2,
  rating: null,
  durationMs: 3_609_523,
  addedAt: 0,
  posterId: 11,
  posterHash: null,
  posterBlurhash: null,
  viewOffsetMs: 671_541,
  viewCount: 0,
} as never;

const img = () => document.querySelector("img") as HTMLImageElement;

it("shows where the viewer stopped and falls back to the poster", async () => {
  dispose = render(() => createComponent(LandscapeCard, { item, snapshot: true }), document.body);
  expect(img().getAttribute("src")).toBe("/api/v1/items/3/snapshot?at=670000");
  img().dispatchEvent(new Event("error"));
  await Promise.resolve();
  expect(img().getAttribute("src")).toBe("/api/v1/images/11?w=480");
});

it("uses the poster when not asked for a snapshot or nothing was watched", () => {
  dispose = render(() => createComponent(LandscapeCard, { item }), document.body);
  expect(img().getAttribute("src")).toBe("/api/v1/images/11?w=480");
  dispose();
  const fresh = { ...(item as object), viewOffsetMs: 0 } as never;
  dispose = render(
    () => createComponent(LandscapeCard, { item: fresh, snapshot: true }),
    document.body,
  );
  expect(img().getAttribute("src")).toBe("/api/v1/images/11?w=480");
});

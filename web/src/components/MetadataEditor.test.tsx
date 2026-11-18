import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import MetadataEditor from "./MetadataEditor";

vi.mock("../i18n", () => ({ useI18n: () => ({ t: (s: string) => s }) }));

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const meta = {
  itemId: 9,
  kind: "movie",
  title: "Heat",
  sortTitle: "heat",
  originalTitle: null,
  year: 1995,
  summary: "Old summary",
  tagline: null,
  contentRating: "R",
  studio: null,
  genres: ["Crime"],
  tmdbId: 949,
  lockedFields: [],
  canMatch: true,
};

const button = (label: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(label));

it("saves only changed fields, locks them and reports the server's reason", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: meta, response: new Response() } as never);
  const patch = vi
    .spyOn(client, "PATCH")
    .mockResolvedValueOnce({
      error: { error: { message: "validation: Title is too long (at most 500 characters)" } },
      response: new Response(null, { status: 422 }),
    } as never)
    .mockResolvedValueOnce({
      data: { ...meta, summary: "New summary", lockedFields: ["summary"] },
      response: new Response(),
    } as never);
  const onSaved = vi.fn();
  dispose = render(() => createComponent(MetadataEditor, { itemId: 9, onSaved }), document.body);
  button("Edit metadata")?.click();
  await tick();
  await tick();
  const summary = document.querySelector("textarea") as HTMLTextAreaElement;
  expect(summary.value).toBe("Old summary");
  summary.value = "New summary";
  summary.dispatchEvent(new Event("input", { bubbles: true }));

  const submit = () =>
    document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  submit();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toBe(
    "Title is too long (at most 500 characters)",
  );
  submit();
  await tick();
  expect(patch).toHaveBeenLastCalledWith("/api/v1/items/{id}/metadata", {
    params: { path: { id: 9 } },
    body: { lockedFields: ["summary"], summary: "New summary" },
  });
  expect(onSaved).toHaveBeenCalledTimes(1);
  expect(document.querySelector('[role="status"]')?.textContent).toContain("Saved.");
});

it("matches to a chosen TMDB entry", async () => {
  vi.spyOn(client, "GET").mockImplementation(((path: string) =>
    Promise.resolve({
      data:
        path === "/api/v1/system/tmdb/search"
          ? { configured: true, results: [{ id: 500, title: "Heat", kind: "movie", year: 1986 }] }
          : meta,
      response: new Response(),
    })) as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ response: new Response(null, { status: 202 }) } as never);
  dispose = render(
    () => createComponent(MetadataEditor, { itemId: 9, onSaved: vi.fn() }),
    document.body,
  );
  button("Edit metadata")?.click();
  await tick();
  await tick();
  [...document.querySelectorAll<HTMLElement>('[role="radio"]')]
    .find((r) => r.textContent === "Match")
    ?.click();
  await tick();
  const forms = document.querySelectorAll("form");
  forms[forms.length - 1].dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  await tick();
  button("Use this")?.click();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/items/{id}/match", {
    params: { path: { id: 9 } },
    body: { tmdbId: 500 },
  });
  expect(document.querySelector('[role="status"]')?.textContent).toContain('Matching to "Heat"');
});

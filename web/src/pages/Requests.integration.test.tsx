import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Requests from "./Requests";

const auth = vi.hoisted(() => ({ admin: false }));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ id: 1, isAdmin: auth.admin }) }),
}));
let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  auth.admin = false;
});
function input(id: string, value: string) {
  const element = document.getElementById(id) as HTMLInputElement;
  element.value = value;
  element.dispatchEvent(new Event("input", { bubbles: true }));
}
it("keeps request entries on failure and reports success only after server acceptance", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: [], response: new Response() } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValueOnce({
      error: { message: "denied" },
      response: new Response(null, { status: 403 }),
    } as never)
    .mockResolvedValueOnce({
      data: { id: 4, title: "Arrival", kind: "movie", tmdbId: 329865, status: "pending" },
      response: new Response(),
    } as never);
  dispose = render(() => createComponent(Requests, {}), document.body);
  await tick();
  input("request-title", "Arrival");
  input("request-tmdb", "329865");
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("could not be submitted");
  expect((document.getElementById("request-title") as HTMLInputElement).value).toBe("Arrival");
  expect(document.body.textContent).not.toContain("submitted for review");
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(post).toHaveBeenLastCalledWith("/api/v1/requests", {
    body: { title: "Arrival", kind: "movie", tmdbId: 329865, year: undefined },
  });
  expect(document.body.textContent).toContain("submitted for review");
  expect(document.body.textContent).toContain("Status: Pending");
  expect([...document.querySelectorAll("button")].some((b) => b.textContent === "Approve")).toBe(
    false,
  );
});
it("does not change the displayed status if an admin update is rejected", async () => {
  auth.admin = true;
  mockRequests([{ id: 4, title: "Arrival", kind: "movie", tmdbId: 329865, status: "pending" }]);
  vi.spyOn(client, "PATCH").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 500 }),
  } as never);
  dispose = render(() => createComponent(Requests, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")].find((b) => b.textContent === "Approve")?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("could not be saved");
  expect(document.body.textContent).toContain("Status: Pending");
});

/** GET answers per path: the request list, or empty download-service settings. */
function mockRequests(rows: unknown[][] | unknown[]) {
  const lists = Array.isArray(rows[0]) ? (rows as unknown[][]) : [rows as unknown[]];
  let call = 0;
  return vi.spyOn(client, "GET").mockImplementation(((path: string) => {
    if (path === "/api/v1/requests") {
      const data = lists[Math.min(call++, lists.length - 1)];
      return Promise.resolve({ data, response: new Response() });
    }
    return Promise.resolve({
      data: { kind: "radarr", enabled: false, url: "", hasApiKey: false, rootFolder: "" },
      response: new Response(),
    });
  }) as never);
}

it("shows Radarr's reason when approval fails and confirms when the title was sent", async () => {
  auth.admin = true;
  const pending = { id: 4, title: "Arrival", kind: "movie", tmdbId: 329865, status: "pending" };
  mockRequests([[pending], [{ ...pending, status: "approved", arrId: 9 }]]);
  vi.spyOn(client, "PATCH")
    .mockResolvedValueOnce({
      error: { error: { message: "external service: Radarr: the API key was rejected" } },
      response: new Response(null, { status: 503 }),
    } as never)
    .mockResolvedValueOnce({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(() => createComponent(Requests, {}), document.body);
  await tick();
  const approve = () =>
    [...document.querySelectorAll("button")].find((b) => b.textContent === "Approve")?.click();
  approve();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toBe(
    "Radarr: the API key was rejected",
  );
  approve();
  await tick();
  await tick();
  expect(document.querySelector('[role="status"]')?.textContent).toContain(
    '"Arrival" was sent to Radarr.',
  );
  expect(document.body.textContent).toContain("Sent to Radarr");
});

it("says when nothing was sent and offers to send an approved request again", async () => {
  auth.admin = true;
  const pending = { id: 5, title: "Severance", kind: "show", tmdbId: 95396, status: "pending" };
  const approved = { ...pending, status: "approved" };
  mockRequests([[pending], [approved], [{ ...approved, arrId: 12 }]]);
  const patch = vi
    .spyOn(client, "PATCH")
    .mockResolvedValueOnce({ response: new Response(null, { status: 204 }) } as never)
    .mockResolvedValueOnce({
      error: { error: { message: "validation: Sonarr is turned off, so nothing was sent" } },
      response: new Response(null, { status: 422 }),
    } as never)
    .mockResolvedValueOnce({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(() => createComponent(Requests, {}), document.body);
  await tick();
  const button = (label: string) =>
    [...document.querySelectorAll("button")].find((b) => b.textContent === label);
  button("Approve")?.click();
  await tick();
  await tick();
  expect(document.querySelector('[role="status"]')?.textContent).toContain(
    "Approved. Nothing was sent to Sonarr because it is turned off.",
  );
  expect(document.body.textContent).toContain("Not sent to Sonarr");
  button("Send to Sonarr")?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Sonarr is turned off");
  button("Send to Sonarr")?.click();
  await tick();
  await tick();
  expect(patch).toHaveBeenCalledTimes(3);
  expect(document.querySelector('[role="status"]')?.textContent).toContain(
    '"Severance" was sent to Sonarr.',
  );
  expect(button("Send to Sonarr")).toBeUndefined();
});

it("searches music through Lidarr and requests the picked album by MusicBrainz ID", async () => {
  const get = vi.spyOn(client, "GET").mockImplementation(((path: string) => {
    if (path === "/api/v1/requests/music/search")
      return Promise.resolve({
        data: [
          {
            kind: "album",
            title: "Arrival",
            artist: "ABBA",
            year: 1976,
            musicbrainzId: "a3f1f4c5-6d0b-3c5e-8d2a-0123456789ab",
            imageUrl: null,
            inLidarr: false,
          },
        ],
        response: new Response(),
      });
    return Promise.resolve({ data: [], response: new Response() });
  }) as never);
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: { id: 8, title: "Arrival", kind: "album", tmdbId: 0, status: "pending", artist: "ABBA" },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(Requests, {}), document.body);
  await tick();
  const kind = document.getElementById("request-kind") as HTMLSelectElement;
  kind.value = "album";
  kind.dispatchEvent(new Event("change", { bubbles: true }));
  expect(document.getElementById("request-tmdb")).toBeNull();
  input("request-title", "arrival");
  [...document.querySelectorAll("button")]
    .find((b) => b.textContent?.includes("Search music"))
    ?.click();
  await tick();
  expect(get).toHaveBeenCalledWith("/api/v1/requests/music/search", {
    params: { query: { query: "arrival", kind: "album" } },
  });
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("Arrival"))?.click();
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/requests", {
    body: {
      title: "Arrival",
      kind: "album",
      musicbrainzId: "a3f1f4c5-6d0b-3c5e-8d2a-0123456789ab",
      artist: "ABBA",
      year: 1976,
    },
  });
  expect(document.body.textContent).toContain("Album · ABBA");
});

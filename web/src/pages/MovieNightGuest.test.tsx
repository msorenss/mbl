import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import MovieNightGuest from "./MovieNightGuest";

vi.mock("@solidjs/router", () => ({ useParams: () => ({ code: "guest-code" }) }));

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  localStorage.clear();
  vi.restoreAllMocks();
});

const view = (matched: unknown = null) => ({
  data: {
    seed: "mm_x",
    name: "Anna",
    memberCount: 2,
    status: matched ? "matched" : "active",
    startsAt: null,
    matched,
    titles: [{ id: 7, title: "Up", year: 2009, posterId: 3 }],
  },
  response: new Response(),
});

it("joins with a name and votes with the guest token", async () => {
  const get = vi
    .spyOn(client, "GET")
    .mockResolvedValueOnce(view() as never)
    .mockResolvedValue(view({ id: 7, title: "Up", year: 2009 }) as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValueOnce({
      data: { token: "tok", name: "Anna" },
      response: new Response(),
    } as never)
    .mockResolvedValueOnce({
      data: { matched: true, matchedItemId: 7 },
      response: new Response(),
    } as never);
  dispose = render(() => createComponent(MovieNightGuest, {}), document.body);
  await tick();

  const input = document.querySelector("input") as HTMLInputElement;
  input.value = "Anna";
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
  (document.querySelector("form") as HTMLFormElement).requestSubmit();
  await tick();
  await tick();
  expect(post.mock.calls[0][1]).toMatchObject({ body: { name: "Anna" } });
  expect(localStorage.getItem("mbl.movieNight.guest-code")).toBe("tok");
  expect(get.mock.calls[0][1]).toMatchObject({ params: { header: { "x-mbl-guest": "tok" } } });
  expect(document.body.textContent).toContain("Up");
  expect(document.querySelector("img")?.getAttribute("src")).toContain("token=tok");

  const like = [...document.querySelectorAll("button")].find((b) => b.textContent === "Like");
  like?.click();
  await tick();
  await tick();
  expect(post.mock.calls[1][1]).toMatchObject({ body: { itemId: 7, vote: 1 } });
  expect(document.body.textContent).toContain("It's a match!");
});

it("forgets an ended guest token and asks for a name again", async () => {
  localStorage.setItem("mbl.movieNight.guest-code", "old");
  vi.spyOn(client, "GET").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 401 }),
  } as never);
  dispose = render(() => createComponent(MovieNightGuest, {}), document.body);
  await tick();
  await tick();
  expect(localStorage.getItem("mbl.movieNight.guest-code")).toBeNull();
  expect(document.querySelector("form")).not.toBeNull();
  expect(document.body.textContent).toContain("turned off the guest link");
});

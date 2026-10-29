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
  expect(document.body.textContent).toContain("Status: pending");
  expect([...document.querySelectorAll("button")].some((b) => b.textContent === "Approve")).toBe(
    false,
  );
});
it("does not change the displayed status if an admin update is rejected", async () => {
  auth.admin = true;
  vi.spyOn(client, "GET").mockResolvedValue({
    data: [{ id: 4, title: "Arrival", kind: "movie", tmdbId: 329865, status: "pending" }],
    response: new Response(),
  } as never);
  vi.spyOn(client, "PATCH").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 500 }),
  } as never);
  dispose = render(() => createComponent(Requests, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")].find((b) => b.textContent === "Approve")?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("could not be saved");
  expect(document.body.textContent).toContain("Status: pending");
});

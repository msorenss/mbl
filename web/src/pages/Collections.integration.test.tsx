import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Collections from "./Collections";

vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; children: string }) => <a href={props.href}>{props.children}</a>,
}));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ id: 1, isAdmin: true }) }),
}));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
it("shows failed collection mutations instead of claiming they succeeded", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: [], response: new Response() } as never);
  vi.spyOn(client, "POST").mockResolvedValue({
    error: { error: { message: "Forbidden" } },
    response: new Response(null, { status: 403 }),
  } as never);
  dispose = render(() => createComponent(Collections, {}), document.body);
  await tick();
  const input = document.querySelector("input") as HTMLInputElement;
  input.value = "My collection";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("failed");
  expect(input.value).toBe("My collection");
});

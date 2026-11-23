import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import LinkedServersSettings from "./LinkedServersSettings";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
const tick = () => new Promise((r) => setTimeout(r, 0));

function type(label: string, value: string) {
  const input = [...document.querySelectorAll("label")]
    .find((l) => l.textContent?.startsWith(label))
    ?.querySelector("input") as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

it("links a server and explains why the other server refused", async () => {
  const get = vi
    .spyOn(client, "GET")
    .mockResolvedValue({ data: [], response: new Response() } as never);
  const post = vi.spyOn(client, "POST").mockResolvedValueOnce({
    error: {
      error: {
        code: "validation",
        message: "validation: http://pc did not accept the link: unauthorized",
      },
    },
    response: new Response(null, { status: 422 }),
  } as never);
  dispose = render(() => createComponent(LinkedServersSettings, {}), document.body);
  await tick();
  type("Name", "Windows");
  type("Address", "http://pc:8420");
  type("Token from the other server", "mbl_x");
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/system/linked-servers", {
    body: { name: "Windows", url: "http://pc:8420", token: "mbl_x", shared: false },
  });
  expect(document.body.textContent).toContain("did not accept the link: unauthorized");

  post.mockResolvedValueOnce({
    data: { id: "a1", name: "Windows", url: "http://pc:8420", shared: false },
    response: new Response(),
  } as never);
  get.mockResolvedValue({
    data: [{ id: "a1", name: "Windows", url: "http://pc:8420", shared: false }],
    response: new Response(),
  } as never);
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  await tick();
  expect(document.body.textContent).toContain("http://pc:8420");
  expect(document.body.textContent).not.toContain("unauthorized");
});

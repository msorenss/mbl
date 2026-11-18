import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Connect from "./Connect";

const params = vi.hoisted(() => ({ value: {} as Record<string, string> }));
vi.mock("@solidjs/router", () => ({ useSearchParams: () => [params.value, vi.fn()] }));

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  params.value = {};
});

function button(label: string) {
  return [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
}

it("looks up the device and approves it", async () => {
  params.value = { code: "123456" };
  const post = vi.spyOn(client, "POST").mockImplementation(async (path) =>
    path === "/api/v1/auth/quick-connect/lookup"
      ? ({
          data: { deviceName: "Living room TV", clientName: "mbl-tv", expiresInSecs: 500 },
          response: new Response(),
        } as never)
      : ({ response: new Response(null, { status: 204 }) } as never),
  );
  dispose = render(() => createComponent(Connect, {}), document.body);
  button("Continue")?.click();
  await tick();
  expect(document.body.textContent).toContain("Sign in Living room TV (mbl-tv)");
  button("Approve")?.click();
  await tick();
  expect(post).toHaveBeenLastCalledWith("/api/v1/auth/quick-connect/authorize", {
    body: { code: "123456" },
  });
  expect(document.querySelector('[role="status"]')?.textContent).toContain(
    "Living room TV is signed in",
  );
});

it("rejects a malformed code without calling the server", async () => {
  const post = vi.spyOn(client, "POST");
  dispose = render(() => createComponent(Connect, {}), document.body);
  const input = document.querySelector("input") as HTMLInputElement;
  input.value = "12a4";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  expect(post).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("6-digit code");
});

it("explains an unknown or expired code", async () => {
  params.value = { code: "654321" };
  vi.spyOn(client, "POST").mockResolvedValue({
    error: { error: { code: "not_found", message: "not found" } },
    response: new Response(null, { status: 404 }),
  } as never);
  dispose = render(() => createComponent(Connect, {}), document.body);
  button("Continue")?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain(
    "expire after 10 minutes",
  );
  expect(button("Approve")).toBeUndefined();
});

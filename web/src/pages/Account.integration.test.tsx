import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Account from "./Account";

vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ id: 1 }), logout: vi.fn() }),
}));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
it("does not enable two-factor authentication until the verification succeeds", async () => {
  let enabled = false;
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: path === "/api/v1/auth/sessions" ? [] : { totpEnabled: enabled },
        response: new Response(),
      }) as never,
  );
  const post = vi.spyOn(client, "POST").mockImplementation(async (path) => {
    if (path === "/api/v1/auth/totp/setup")
      return {
        data: { secret: "TESTSECRET", uri: "otpauth://test" },
        response: new Response(),
      } as never;
    enabled = true;
    return { data: true, response: new Response() } as never;
  });
  dispose = render(() => createComponent(Account, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("Set up"))?.click();
  await tick();
  expect(document.body.textContent).toContain("TESTSECRET");
  expect(enabled).toBe(false);
  const input = document.getElementById("verify-totp") as HTMLInputElement;
  input.value = "123456";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/auth/totp/verify", { body: { pin: "123456" } });
  expect(document.body.textContent).not.toContain("TESTSECRET");
  expect(document.body.textContent).toContain("Enabled");
});

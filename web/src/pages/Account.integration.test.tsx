import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Account from "./Account";

const { logout } = vi.hoisted(() => ({ logout: vi.fn(async () => {}) }));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ id: 1 }), logout }),
}));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  logout.mockClear();
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

const mockReads = () =>
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: path === "/api/v1/auth/sessions" ? [] : { totpEnabled: false },
        response: new Response(),
      }) as never,
  );
const submitDelete = async (password: string) => {
  const input = document.getElementById("delete-account-password") as HTMLInputElement;
  input.value = password;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.form?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
};
it("deletes the account with the password and signs out", async () => {
  mockReads();
  vi.spyOn(window, "confirm").mockReturnValue(true);
  const del = vi
    .spyOn(client, "DELETE")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(() => createComponent(Account, {}), document.body);
  await tick();
  await submitDelete("hunter2");
  expect(del).toHaveBeenCalledWith("/api/v1/users/me", { body: { password: "hunter2" } });
  expect(logout).toHaveBeenCalledOnce();
});
it("keeps the account and explains a rejected password", async () => {
  mockReads();
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 401 }),
  } as never);
  dispose = render(() => createComponent(Account, {}), document.body);
  await tick();
  await submitDelete("wrong");
  expect(document.querySelector("[role=alert]")?.textContent).toContain("Incorrect password");
  expect(logout).not.toHaveBeenCalled();
});
it("does nothing when the confirmation is cancelled", async () => {
  mockReads();
  vi.spyOn(window, "confirm").mockReturnValue(false);
  const del = vi.spyOn(client, "DELETE");
  dispose = render(() => createComponent(Account, {}), document.body);
  await tick();
  await submitDelete("hunter2");
  expect(del).not.toHaveBeenCalled();
  expect(logout).not.toHaveBeenCalled();
});

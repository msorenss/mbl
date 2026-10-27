import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import { Login } from "./Login";

const auth = vi.hoisted(() => ({
  login: vi.fn(async () => true),
  user: () => null,
  needsSetup: () => false,
  loading: () => false,
}));
vi.mock("@solidjs/router", () => ({ useNavigate: () => vi.fn() }));
vi.mock("../context/AuthContext", () => ({ useAuth: () => auth }));
vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast: vi.fn() }) }));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.clearAllMocks();
});
it("submits the authentication code with credentials", async () => {
  dispose = render(() => createComponent(Login, {}), document.body);
  for (const [id, value] of [
    ["username", "admin"],
    ["password", "secret"],
    ["totp-code", "123456"],
  ]) {
    const input = document.getElementById(id) as HTMLInputElement;
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await Promise.resolve();
  expect(auth.login).toHaveBeenCalledWith("admin", "secret", "123456");
});

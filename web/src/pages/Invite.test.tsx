import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Invite from "./Invite";

const state = vi.hoisted(() => ({ signedIn: false, navigate: vi.fn(), refresh: vi.fn() }));
vi.mock("@solidjs/router", () => ({
  useParams: () => ({ code: "single-use" }),
  useNavigate: () => state.navigate,
  A: (props: { children: unknown }) => props.children,
}));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => (state.signedIn ? { id: 1 } : null), refreshUser: state.refresh }),
}));
let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  localStorage.clear();
  vi.restoreAllMocks();
  vi.clearAllMocks();
  state.signedIn = false;
});
it("keeps failed registration on the form and navigates only after success", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { expiresAt: Date.now() + 60000 },
    response: new Response(),
  } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValueOnce({ error: {}, response: new Response(null, { status: 409 }) } as never)
    .mockResolvedValueOnce({
      data: { token: "new-token", user: { id: 2 } },
      response: new Response(),
    } as never);
  dispose = render(() => createComponent(Invite, {}), document.body);
  await tick();
  for (const [id, value] of [
    ["invite-username", "guest"],
    ["invite-password", "password123"],
  ]) {
    const input = document.getElementById(id) as HTMLInputElement;
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Registration failed");
  expect(state.navigate).not.toHaveBeenCalled();
  expect((document.getElementById("invite-username") as HTMLInputElement).value).toBe("guest");
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(post).toHaveBeenLastCalledWith("/api/v1/auth/register-invite", {
    body: { code: "single-use", username: "guest", password: "password123" },
  });
  expect(state.refresh).toHaveBeenCalled();
  expect(state.navigate).toHaveBeenCalledWith("/", { replace: true });
});
it("does not replace an already signed-in account", async () => {
  state.signedIn = true;
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { expiresAt: Date.now() + 60000 },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(Invite, {}), document.body);
  await tick();
  expect(document.querySelector("form")).toBeNull();
  expect(document.body.textContent).toContain("already signed in");
});

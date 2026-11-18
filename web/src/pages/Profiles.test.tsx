import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Profiles from "./Profiles";

const auth = vi.hoisted(() => ({
  switchProfile: vi.fn(),
  logout: vi.fn(),
  navigate: vi.fn(),
}));
vi.mock("@solidjs/router", () => ({ useNavigate: () => auth.navigate }));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({
    user: () => ({ id: 1, username: "parent", displayName: "Parent", isAdmin: true }),
    logout: auth.logout,
    switchProfile: auth.switchProfile,
  }),
}));
vi.mock("../i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const profile = (id: number, name: string, isManaged: boolean, hasPin: boolean) => ({
  id,
  username: name.toLowerCase(),
  displayName: name,
  isAdmin: false,
  isManaged,
  hasPin,
});

beforeEach(() => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: [
      profile(1, "Parent", false, false),
      profile(2, "Kid", true, true),
      profile(3, "Toddler", true, false),
      profile(4, "Partner", false, false),
    ],
    response: new Response(),
  } as never);
});
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  auth.switchProfile.mockReset();
  auth.logout.mockReset();
  auth.navigate.mockReset();
});

function card(name: string) {
  return [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(name));
}

it("asks for the PIN of a protected profile and switches", async () => {
  auth.switchProfile.mockResolvedValueOnce("Incorrect PIN").mockResolvedValueOnce(null);
  dispose = render(() => createComponent(Profiles, {}), document.body);
  await tick();
  card("Kid")?.click();
  await tick();
  const input = document.querySelector<HTMLInputElement>('input[aria-label="PIN"]');
  expect(input).not.toBeNull();
  if (!input) return;
  input.value = "1234";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.form?.requestSubmit();
  await tick();
  expect(auth.switchProfile).toHaveBeenCalledWith(2, "1234");
  expect(document.body.textContent).toContain("Incorrect PIN");
  input.form?.requestSubmit();
  await tick();
  expect(auth.navigate).toHaveBeenCalledWith("/", { replace: true });
});

it("enters a profile without PIN directly and sends full accounts to sign-in", async () => {
  auth.switchProfile.mockResolvedValue(null);
  dispose = render(() => createComponent(Profiles, {}), document.body);
  await tick();
  card("Toddler")?.click();
  await tick();
  expect(auth.switchProfile).toHaveBeenCalledWith(3, undefined);
  card("Partner")?.click();
  await tick();
  expect(auth.logout).toHaveBeenCalled();
  expect(auth.navigate).toHaveBeenCalledWith("/login?username=partner", { replace: true });
});

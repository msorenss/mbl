import { createComponent, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Libraries from "./Libraries";

vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; children: JSX.Element }) => <a href={props.href}>{props.children}</a>,
}));
vi.mock("../context/AuthContext", () => ({ useAuth: () => ({ user: () => ({ isAdmin: true }) }) }));
vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast: vi.fn() }) }));
vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "en" }) }));
let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
async function mount(existing = false) {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data:
          path === "/api/v1/libraries"
            ? existing
              ? [{ id: 1, name: "Test", kind: "show", paths: ["/"] }]
              : []
            : { path: null, parent: null, entries: [{ name: "/", path: "/" }] },
        response: new Response(),
      }) as never,
  );
  dispose = render(() => createComponent(Libraries, {}), document.body);
  await tick();
}
it("focuses the library dialog, closes with Escape and restores focus to its trigger", async () => {
  await mount();
  const trigger = [...document.querySelectorAll("button")].find((b) =>
    b.textContent?.includes("Add Library"),
  )!;
  trigger.focus();
  trigger.click();
  await tick();
  const dialog = document.querySelector('[role="dialog"]');
  expect(dialog).not.toBeNull();
  expect(dialog?.contains(document.activeElement)).toBe(true);
  document.activeElement?.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
  );
  await tick();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(trigger);
});
it("warns about an existing filesystem root and shows the actual save error", async () => {
  await mount(true);
  vi.spyOn(client, "PATCH").mockResolvedValue({
    error: { error: { code: "conflict", message: "/series overlaps / (library 2)" } },
    response: new Response(null, { status: 409 }),
  } as never);
  [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Edit")?.click();
  await tick();
  expect(document.body.textContent).toContain("A whole filesystem or drive is selected");
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain(
    "/series overlaps / (library 2)",
  );
  expect(document.body.textContent).not.toContain("[object Object]");
});

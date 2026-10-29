import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import Invites from "./Invites";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
it("requires explicit library selection and keeps a link visible when revocation fails", async () => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data:
          path === "/api/v1/invites"
            ? [{ id: 5, code: "secret", expiresAt: Date.now() + 60000, usedAt: null }]
            : [{ id: 3, name: "Movies" }],
        response: new Response(),
      }) as never,
  );
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: { id: 6, code: "created", expiresAt: Date.now() + 60000 },
    response: new Response(),
  } as never);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 500 }),
  } as never);
  dispose = render(() => createComponent(Invites, {}), document.body);
  await tick();
  const submit = () =>
    document
      .querySelector("form")
      ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  submit();
  await tick();
  expect(post).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("at least one library");
  const checkbox = document.querySelector('input[type="checkbox"]') as HTMLInputElement;
  checkbox.click();
  submit();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/invites", {
    body: { libraryIds: [3], contentRatingCeiling: undefined, ttlHours: 72 },
  });
  [...document.querySelectorAll("button")]
    .find((button) => button.textContent === "Revoke invitation")
    ?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("could not be revoked");
  expect(
    [...document.querySelectorAll("input")].some((input) =>
      input.value.includes("/invite/created"),
    ),
  ).toBe(true);
});

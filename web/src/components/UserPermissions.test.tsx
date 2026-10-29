import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import UserPermissions from "./UserPermissions";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const user = { id: 2, username: "child", displayName: "Child", isAdmin: false, createdAt: 0 };
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
const click = (text: string) =>
  [...document.querySelectorAll("button")].find((button) => button.textContent === text)?.click();
it("does not offer saving after a failed permissions load", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 403 }),
  } as never);
  dispose = render(() => createComponent(UserPermissions, { user }), document.body);
  click("Permissions");
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Could not load");
  expect(
    [...document.querySelectorAll("button")].some(
      (button) => button.textContent === "Save permissions",
    ),
  ).toBe(false);
  expect(document.body.textContent).toContain("Retry loading permissions");
});
it("sends explicit null to remove a rating and preserves choices on save rejection", async () => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data:
          path === "/api/v1/libraries"
            ? [{ id: 3, name: "Movies" }]
            : { isManaged: true, contentRatingCeiling: "PG", libraryIds: [3], hasPin: true },
        response: new Response(),
      }) as never,
  );
  const put = vi
    .spyOn(client, "PUT")
    .mockResolvedValue({ error: {}, response: new Response(null, { status: 500 }) } as never);
  dispose = render(() => createComponent(UserPermissions, { user }), document.body);
  click("Permissions");
  await tick();
  const select = document.getElementById("rating-2") as HTMLSelectElement;
  select.value = "";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(put).toHaveBeenCalledWith("/api/v1/users/{id}/permissions", {
    params: { path: { id: 2 } },
    body: { isManaged: true, contentRatingCeiling: null, libraryIds: [3] },
  });
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("could not be saved");
  expect(select.value).toBe("");
  expect(document.body.textContent).not.toContain("Permissions saved.");
});

import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import DlnaSettings from "./DlnaSettings";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

function mockLoad(settings: Record<string, unknown>) {
  vi.spyOn(client, "GET").mockImplementation(((path: string) =>
    Promise.resolve({
      data:
        path === "/api/v1/libraries"
          ? [
              { id: 1, name: "Movies" },
              { id: 2, name: "Kids" },
            ]
          : settings,
      response: new Response(),
    })) as never);
}

const checkbox = (text: string) =>
  [...document.querySelectorAll("label")]
    .find((l) => l.textContent === text)
    ?.querySelector<HTMLInputElement>("input") as HTMLInputElement;

it("shares the chosen libraries with an age limit and warns when none is chosen", async () => {
  mockLoad({ dlnaEnabled: false, dlnaFriendlyName: "Lounge", dlnaLibraryIds: [] });
  const patch = vi.spyOn(client, "PATCH").mockResolvedValue({
    data: { dlnaEnabled: true, dlnaLibraryIds: [2], dlnaMaxRating: "PG" },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(DlnaSettings, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("Off");

  const share = checkbox("Share over DLNA");
  share.checked = true;
  share.dispatchEvent(new Event("change", { bubbles: true }));
  expect(document.body.textContent).toContain("Choose at least one library.");

  const kids = checkbox("Kids");
  kids.checked = true;
  kids.dispatchEvent(new Event("change", { bubbles: true }));
  const select = document.getElementById("dlna-rating") as HTMLSelectElement;
  select.value = "PG";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  expect(document.body.textContent).toContain("Shared on the network");

  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  expect(patch).toHaveBeenCalledWith("/api/v1/system/settings", {
    body: {
      dlnaEnabled: true,
      dlnaFriendlyName: "Lounge",
      dlnaLibraryIds: [2],
      dlnaMaxRating: "PG",
    },
  });
  expect(document.querySelector('[role="status"]')?.textContent).toBe(
    "Saved. TVs on your network can now find the server.",
  );
});

it("shows the server's reason when saving fails", async () => {
  mockLoad({ dlnaEnabled: true, dlnaFriendlyName: "", dlnaLibraryIds: [1] });
  vi.spyOn(client, "PATCH").mockResolvedValue({
    error: { error: { message: "validation: Library 1 does not exist" } },
    response: new Response(null, { status: 422 }),
  } as never);
  dispose = render(() => createComponent(DlnaSettings, {}), document.body);
  await tick();
  expect(checkbox("Movies").checked).toBe(true);
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("Library 1 does not exist");
});

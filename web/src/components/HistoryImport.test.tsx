import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import HistoryImport from "./HistoryImport";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
async function selectFile(data: unknown) {
  const file = { name: "history.json", size: 100, text: async () => JSON.stringify(data) };
  const input = document.getElementById("history-file") as HTMLInputElement;
  Object.defineProperty(input, "files", { configurable: true, value: [file] });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
}
it("validates export shape and retains a valid file after a rejected import", async () => {
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValueOnce({ error: {}, response: new Response(null, { status: 422 }) } as never)
    .mockResolvedValueOnce({
      data: { totalItems: 2, matchedItems: 1, updatedWatchState: 1, unmatched: ["Missing movie"] },
      response: new Response(),
    } as never);
  dispose = render(() => createComponent(HistoryImport, {}), document.body);
  await selectFile({ items: [{ name: "wrong provider" }] });
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("title");
  expect(document.querySelector("button")?.disabled).toBe(true);
  const body = { items: [{ title: "Arrival", viewCount: 1 }, { title: "Missing movie" }] };
  await selectFile(body);
  document.querySelector("button")?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Import failed");
  expect(document.body.textContent).not.toContain("watch states updated");
  document.querySelector("button")?.click();
  await tick();
  expect(post).toHaveBeenLastCalledWith("/api/v1/import/plex", { body });
  expect(document.body.textContent).toContain("1 of 2 matched");
  expect(document.body.textContent).toContain("Missing movie");
});

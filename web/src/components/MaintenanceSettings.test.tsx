import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import MaintenanceSettings from "./MaintenanceSettings";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text);

const report = (dryRun: boolean) => ({
  dryRun,
  startedAt: 1,
  durationMs: 5,
  databaseBytes: 10 * 1024 * 1024,
  unusedBytes: 1024 * 1024,
  vacuumed: false,
  backup: "mbl-auto-1.db",
  backupsRemoved: 0,
  oldJobs: 2,
  goneItems: 1,
  emptyItems: 1,
  goneDays: 30,
  examples: ["Heat"],
  cleaned: !dryRun,
  unreachableLibraries: ["USB disk"],
  error: null,
});

it("previews, then removes gone titles only after a confirmation", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      enabled: true,
      hour: 4,
      intervalDays: 1,
      keepBackups: 7,
      cleanupAfterDays: 0,
      utcOffsetMinutes: 60,
      lastRun: null,
      lastReport: null,
    },
    response: new Response(),
  } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValueOnce({ data: report(true), response: new Response() } as never)
    .mockResolvedValueOnce({ data: report(false), response: new Response() } as never);
  const confirm = vi.spyOn(window, "confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
  dispose = render(() => createComponent(MaintenanceSettings, {}), document.body);
  await tick();

  button("Preview")?.click();
  await tick();
  expect(post.mock.calls[0][1]).toMatchObject({ body: { dryRun: true, cleanup: true } });
  const text = document.body.textContent ?? "";
  expect(text).toContain("Would remove 1 titles gone for 30+ days and 1 empty seasons or shows.");
  expect(text).toContain("Heat");
  expect(text).toContain("USB disk");

  button("Remove gone titles")?.click();
  await tick();
  expect(confirm).toHaveBeenCalledWith("Remove 2 titles? This cannot be undone.");
  expect(post).toHaveBeenCalledTimes(1);

  button("Remove gone titles")?.click();
  await tick();
  expect(post.mock.calls[1][1]).toMatchObject({ body: { dryRun: false, cleanup: true } });
  expect(document.body.textContent).toContain("Removed 1 titles and 1 empty seasons or shows.");
});

import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { OnlineProviderSettings } from "./OnlineProviderSettings";

vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "en" }) }));
let dispose: (() => void) | undefined;
const tick = () => new Promise((r) => setTimeout(r, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
it("saves the selected provider while preserving current enabled state and unrelated settings", async () => {
  const config = { enabled: false, settings: { untouched: "keep" } };
  vi.spyOn(client, "GET").mockResolvedValue({ data: config, response: new Response() } as never);
  vi.spyOn(client, "PATCH").mockResolvedValue({ data: config, response: new Response() } as never);
  dispose = render(() => createComponent(OnlineProviderSettings, {}), document.body);
  await tick();
  const select = document.querySelector("select")!;
  select.value = "searxng";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  const input = document.querySelector("input")!;
  input.value = "https://search.example.org";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  document
    .querySelector("form")!
    .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await tick();
  expect(client.PATCH).toHaveBeenCalledWith(
    "/api/v1/plugins/{id}",
    expect.objectContaining({
      body: {
        enabled: false,
        settings: {
          untouched: "keep",
          searchProvider: "searxng",
          searxngUrl: "https://search.example.org",
        },
      },
    }),
  );
  expect(document.body.textContent).toContain("Search settings saved");
});
it("displays a loading error and aborts on close", async () => {
  let signal: AbortSignal | undefined;
  vi.spyOn(client, "GET").mockImplementation((_p, options) => {
    signal = (options as { signal: AbortSignal }).signal;
    return Promise.resolve({
      error: { error: { message: "Permission denied" } },
      response: new Response(null, { status: 403 }),
    }) as never;
  });
  dispose = render(() => createComponent(OnlineProviderSettings, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("Permission denied");
  expect(document.querySelector("button")?.disabled).toBe(true);
  dispose();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
});

import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import WebhookSettings from "./WebhookSettings";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const hook = {
  id: 3,
  name: "Home Assistant",
  url: "http://ha.local/api/webhook/mbl",
  events: ["playback.started"],
  secret: "********",
  enabled: true,
  createdAt: 1,
};

function button(label: string) {
  return [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === label);
}

it("lists webhooks, runs a test and reports a failing receiver", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: [hook], response: new Response() } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValueOnce({ data: { ok: true, status: 200 }, response: new Response() } as never)
    .mockResolvedValueOnce({
      data: { ok: false, status: 500, error: "receiver answered 500" },
      response: new Response(),
    } as never);
  dispose = render(() => createComponent(WebhookSettings, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("Home Assistant");
  expect(document.body.textContent).toContain("Playback started");
  expect(document.body.textContent).toContain("Signed");
  button("Test")?.click();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/webhooks/{id}/test", { params: { path: { id: 3 } } });
  expect(document.querySelector('[role="status"]')?.textContent).toContain("answered 200");
  button("Test")?.click();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("receiver answered 500");
});

it("adds a webhook with the chosen events and shows the server's reason on failure", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: [], response: new Response() } as never);
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    error: { error: { code: "validation", message: "Webhook URL is not a valid URL" } },
    response: new Response(null, { status: 422 }),
  } as never);
  dispose = render(() => createComponent(WebhookSettings, {}), document.body);
  await tick();
  const inputs = document.querySelectorAll<HTMLInputElement>("form input");
  inputs[0].value = "HA";
  inputs[0].dispatchEvent(new Event("input", { bubbles: true }));
  inputs[1].value = "http://ha.local/hook";
  inputs[1].dispatchEvent(new Event("input", { bubbles: true }));
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/webhooks", {
    body: {
      name: "HA",
      url: "http://ha.local/hook",
      events: ["playback.started", "playback.stopped"],
      secret: undefined,
    },
  });
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("not a valid URL");
});

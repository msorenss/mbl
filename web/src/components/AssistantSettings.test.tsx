import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import AssistantSettings from "./AssistantSettings";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const flush = () => new Promise((r) => setTimeout(r, 0));

const select = (id: string, value: string) => {
  const el = document.getElementById(id) as HTMLSelectElement;
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
};

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === text);

it("picks a preset, tests the connection and saves without showing the key", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      enabled: false,
      provider: "openai_compatible",
      baseUrl: "",
      hasApiKey: false,
      model: "",
      effort: null,
    },
    response: new Response(),
  } as never);
  const post = vi.spyOn(client, "POST").mockImplementation((async (path: string) => {
    if (path === "/api/v1/system/assistant/models")
      return { data: ["llama3.2", "qwen3"], response: new Response() };
    return {
      data: {
        ok: true,
        model: "llama3.2",
        latencyMs: 420,
        reply: "OK",
        toolsSupported: false,
        error: null,
      },
      response: new Response(),
    };
  }) as never);
  const put = vi.spyOn(client, "PUT").mockResolvedValue({
    data: {
      enabled: true,
      provider: "openai_compatible",
      baseUrl: "http://localhost:11434/v1",
      hasApiKey: false,
      model: "llama3.2",
      effort: null,
    },
    response: new Response(),
  } as never);

  dispose = render(() => createComponent(AssistantSettings, {}), document.body);
  await flush();
  // Nothing configured yet: Claude is suggested.
  expect((document.getElementById("assistant-url") as HTMLInputElement).value).toBe(
    "https://api.anthropic.com",
  );

  select("assistant-preset", "ollama");
  expect((document.getElementById("assistant-url") as HTMLInputElement).value).toBe(
    "http://localhost:11434/v1",
  );
  expect((document.getElementById("assistant-key") as HTMLInputElement).placeholder).toBe(
    "Not needed for a local server",
  );

  button("Fetch models")?.click();
  await flush();
  expect((document.getElementById("assistant-model") as HTMLInputElement).value).toBe("llama3.2");
  expect(document.body.textContent).toContain("Found 2 models.");

  button("Test connection")?.click();
  await flush();
  expect(post).toHaveBeenLastCalledWith("/api/v1/system/assistant/test", {
    body: {
      provider: "openai_compatible",
      baseUrl: "http://localhost:11434/v1",
      apiKey: undefined,
      model: "llama3.2",
      effort: undefined,
    },
  });
  expect(document.body.textContent).toContain("Connected to llama3.2 in 420 ms. Reply: OK");
  expect(document.body.textContent).toContain("This model did not use tools.");

  const enable = [...document.querySelectorAll("label")]
    .find((l) => l.textContent?.includes("Turn on the assistant"))
    ?.querySelector("input") as HTMLInputElement;
  enable.click();
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await flush();
  expect(put).toHaveBeenCalledWith("/api/v1/system/assistant", {
    body: expect.objectContaining({ enabled: true, model: "llama3.2", clearApiKey: false }),
  });
  expect(document.body.textContent).toContain("Saved. The assistant is on.");
});

it("explains when the settings cannot be loaded", async () => {
  vi.spyOn(client, "GET").mockRejectedValue(new Error("offline"));
  dispose = render(() => createComponent(AssistantSettings, {}), document.body);
  await flush();
  expect(document.body.textContent).toContain("Could not load the assistant settings.");
  expect(document.querySelector("form")).toBeNull();
});

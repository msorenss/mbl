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
  // Every fetched model can be chosen, not only those matching what is typed.
  const list = document.querySelector("select[aria-label='Fetched models']") as HTMLSelectElement;
  expect([...list.options].map((o) => o.value)).toEqual(["", "llama3.2", "qwen3"]);
  list.value = "qwen3";
  list.dispatchEvent(new Event("change", { bubbles: true }));
  expect((document.getElementById("assistant-model") as HTMLInputElement).value).toBe("qwen3");
  list.value = "llama3.2";
  list.dispatchEvent(new Event("change", { bubbles: true }));

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
  // Admins may also let the assistant do everything they can; users can never get that.
  (
    document.querySelector(
      "input[aria-label='Everything an admin can do (administrators)']",
    ) as HTMLInputElement
  ).click();
  const userAdmin = document.querySelector(
    "input[aria-label='Everything an admin can do (other users)']",
  ) as HTMLInputElement;
  expect(userAdmin.disabled).toBe(true);
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await flush();
  expect(put).toHaveBeenCalledWith("/api/v1/system/assistant", {
    body: expect.objectContaining({
      enabled: true,
      model: "llama3.2",
      clearApiKey: false,
      adminScopes: ["read", "playback", "admin"],
      userScopes: ["read", "playback"],
    }),
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

it("runs an agent program such as Claude Code instead of a model", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      enabled: false,
      provider: "openai_compatible",
      baseUrl: "",
      hasApiKey: false,
      model: "",
      effort: null,
      command: "",
    },
    response: new Response(),
  } as never);
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: {
      ok: true,
      model: "claude",
      latencyMs: 9000,
      reply: "OK",
      toolsSupported: true,
      error: null,
    },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(AssistantSettings, {}), document.body);
  await flush();
  select("assistant-preset", "claude-code");
  // A command replaces address, key and model.
  expect(document.getElementById("assistant-url")).toBeNull();
  expect(document.getElementById("assistant-model")).toBeNull();
  const command = document.getElementById("assistant-command") as HTMLInputElement;
  expect(command.value).toContain("claude -p --mcp-config {mcp_config}");
  expect(document.body.textContent).toContain("{mcp_config}");

  button("Test connection")?.click();
  await flush();
  expect(post).toHaveBeenCalledWith("/api/v1/system/assistant/test", {
    body: expect.objectContaining({ provider: "command", command: command.value }),
  });
  expect(document.body.textContent).toContain("Tools work");
});

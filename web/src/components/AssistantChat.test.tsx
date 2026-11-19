import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import AssistantChat, { sseParser } from "./AssistantChat";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const flush = () => new Promise((r) => setTimeout(r, 0));

function sse(...chunks: string[]) {
  const encoder = new TextEncoder();
  return new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
}

it("parses events split across chunks", () => {
  const seen: [string, unknown][] = [];
  const feed = sseParser((name, data) => seen.push([name, data]));
  feed('event: delta\ndata: {"te');
  feed('xt":"Hi"}\n\n: keep-alive\n\nevent: done\r\ndata: {"ok":true}\r\n\r\n');
  expect(seen).toEqual([
    ["delta", { text: "Hi" }],
    ["done", { ok: true }],
  ]);
});

it("stays hidden while the assistant is off", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { available: false },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(AssistantChat, {}), document.body);
  await flush();
  expect(document.querySelector("button")).toBeNull();
});

it("streams the answer and keeps the conversation", async () => {
  const get = vi.spyOn(client, "GET").mockImplementation((async (path: string) => {
    if (path === "/api/v1/assistant/status")
      return { data: { available: true }, response: new Response() };
    return {
      data: [{ id: 7, title: "Anything with Al Pacino?", createdAt: 1, updatedAt: 2 }],
      response: new Response(),
    };
  }) as never);
  const post = vi.spyOn(client, "POST").mockImplementation((async () => ({
    data: sse(
      'event: conversation\ndata: {"id":7,"title":"Anything with Al Pacino?"}\n\n',
      'event: delta\ndata: {"text":"Heat (1995) "}\n\n',
      'event: delta\ndata: {"text":"is in your library."}\n\n',
      'event: done\ndata: {"message":{"id":2,"role":"assistant","content":"Heat (1995) is in your library."},"tools":["find_by_person"]}\n\n',
    ),
    response: new Response(null, { status: 200 }),
  })) as never);

  dispose = render(() => createComponent(AssistantChat, {}), document.body);
  await flush();
  const toggle = document.querySelector("button[aria-label='Assistant']") as HTMLButtonElement;
  toggle.click();
  await flush();
  expect(get).toHaveBeenCalledWith("/api/v1/assistant/conversations");
  expect(document.body.textContent).toContain("Ask about your library");

  const box = document.querySelector("textarea") as HTMLTextAreaElement;
  box.value = "Anything with Al Pacino?";
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await flush();
  await flush();
  expect(post).toHaveBeenCalledWith(
    "/api/v1/assistant/chat",
    expect.objectContaining({
      body: { conversationId: undefined, text: "Anything with Al Pacino?" },
      parseAs: "stream",
    }),
  );
  expect(document.body.textContent).toContain("Heat (1995) is in your library.");
  expect(box.value).toBe("");
  // The conversation continues with the id the server gave.
  expect((document.querySelector("select") as HTMLSelectElement).value).toBe("7");
});

it("shows why the assistant could not answer", async () => {
  vi.spyOn(client, "GET").mockImplementation((async (path: string) =>
    path === "/api/v1/assistant/status"
      ? { data: { available: true }, response: new Response() }
      : { data: [], response: new Response() }) as never);
  vi.spyOn(client, "POST").mockImplementation((async () => ({
    data: sse(
      'event: conversation\ndata: {"id":1,"title":"x"}\n\n',
      'event: error\ndata: {"message":"The API key was rejected (401 Unauthorized)"}\n\n',
    ),
    response: new Response(null, { status: 200 }),
  })) as never);
  dispose = render(() => createComponent(AssistantChat, {}), document.body);
  await flush();
  (document.querySelector("button[aria-label='Assistant']") as HTMLButtonElement).click();
  await flush();
  const box = document.querySelector("textarea") as HTMLTextAreaElement;
  box.value = "Hello";
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await flush();
  await flush();
  expect(document.body.textContent).toContain("The API key was rejected");
});

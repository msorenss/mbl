import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import MicButton from "./MicButton";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  delete (window as unknown as { webkitSpeechRecognition?: unknown }).webkitSpeechRecognition;
});

class FakeRecognition {
  static last: FakeRecognition | undefined;
  lang = "";
  interimResults = true;
  continuous = false;
  onresult: ((e: unknown) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  onend: (() => void) | null = null;
  started = false;
  /** A real recognition that has ended ignores `stop`. */
  ended = false;
  constructor() {
    FakeRecognition.last = this;
  }
  start() {
    this.started = true;
  }
  stop() {
    if (this.ended) return;
    this.ended = true;
    this.onresult?.({ results: [[{ transcript: "vem är det där" }]] });
    this.onend?.();
  }
}

it("uses the browser's recognition when the server has none, and sends the text", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { serverRecognition: false, engine: null },
    response: new Response(),
  } as never);
  (window as unknown as { webkitSpeechRecognition: unknown }).webkitSpeechRecognition =
    FakeRecognition;
  const onText = vi.fn();
  dispose = render(() => createComponent(MicButton, { language: "sv", onText }), document.body);
  await tick();
  await tick();
  const button = document.querySelector("button") as HTMLButtonElement;
  expect(button.title).toContain("your browser");

  // A short click starts; the next press stops.
  button.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  button.dispatchEvent(new Event("pointerup", { bubbles: true }));
  await tick();
  expect(FakeRecognition.last?.started).toBe(true);
  expect(FakeRecognition.last?.lang).toBe("sv");
  expect(button.textContent).toContain("Listening…");
  button.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  await tick();
  await tick();
  expect(onText).toHaveBeenCalledWith("vem är det där");
});

it("stops by itself when the speaker pauses", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { serverRecognition: false, engine: null },
    response: new Response(),
  } as never);
  (window as unknown as { webkitSpeechRecognition: unknown }).webkitSpeechRecognition =
    FakeRecognition;
  const onText = vi.fn();
  dispose = render(() => createComponent(MicButton, { language: "sv", onText }), document.body);
  await tick();
  await tick();
  const button = document.querySelector("button") as HTMLButtonElement;
  button.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  button.dispatchEvent(new Event("pointerup", { bubbles: true }));
  await tick();
  // The browser ends recognition after a pause, without a second press.
  expect(FakeRecognition.last?.continuous).toBe(false);
  const recognition = FakeRecognition.last as FakeRecognition;
  recognition.onresult?.({ results: [[{ transcript: "pausa filmen" }]] });
  recognition.ended = true;
  recognition.onend?.();
  await tick();
  await tick();
  expect(onText).toHaveBeenCalledWith("pausa filmen");
  expect(button.textContent).toContain("Talk");
});

it("is hidden when nothing can recognize speech", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { serverRecognition: false, engine: null },
    response: new Response(),
  } as never);
  dispose = render(
    () => createComponent(MicButton, { language: "sv", onText: () => undefined }),
    document.body,
  );
  await tick();
  await tick();
  expect(document.querySelector("button")).toBeNull();
});

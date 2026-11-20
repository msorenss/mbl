import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { onAssistantAsk, setAssistantAvailable } from "../utils/assistantBridge";
import { CommandPalette } from "./CommandPalette";

vi.mock("@solidjs/router", () => ({ useNavigate: () => vi.fn() }));
vi.mock("../context/AuthContext", () => ({ useAuth: () => ({ user: () => undefined }) }));
vi.mock("../context/ThemeContext", () => ({
  useTheme: () => ({
    theme: () => "dark",
    setTheme: vi.fn(),
    accent: () => "blue",
    setAccent: vi.fn(),
    locale: () => "en",
    setLocale: vi.fn(),
  }),
}));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  setAssistantAvailable(false);
  vi.restoreAllMocks();
});

const flush = () => new Promise((r) => setTimeout(r, 0));

async function type(text: string) {
  const input = document.querySelector("input") as HTMLInputElement;
  input.value = text;
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
  await flush();
}

it("asks the assistant when it is on", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: [], response: new Response() } as never);
  const asked: string[] = [];
  const stop = onAssistantAsk((text) => asked.push(text));
  setAssistantAvailable(true);
  const onClose = vi.fn();
  dispose = render(() => createComponent(CommandPalette, { open: true, onClose }), document.body);
  await type("Anything with Al Pacino?");
  expect(document.body.textContent).toContain("Ask the assistant: “Anything with Al Pacino?”");
  // Nothing else matches the question, so Enter asks.
  document
    .querySelector("input")
    ?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(asked).toEqual(["Anything with Al Pacino?"]);
  expect(onClose).toHaveBeenCalled();
  stop();
});

it("offers nothing to ask while the assistant is off", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: [], response: new Response() } as never);
  dispose = render(
    () => createComponent(CommandPalette, { open: true, onClose: () => {} }),
    document.body,
  );
  await type("Anything with Al Pacino?");
  expect(document.body.textContent).not.toContain("Ask the assistant");
});

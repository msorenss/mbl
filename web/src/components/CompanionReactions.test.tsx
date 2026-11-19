import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import CompanionReactions from "./CompanionReactions";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it("shows new companion reactions for the session", async () => {
  vi.useFakeTimers();
  const now = Date.now();
  const get = vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      companions: ["Hermes"],
      reactions: [
        {
          id: 7,
          sessionId: "s1",
          botName: "Hermes",
          timestampMs: 1000,
          commentary: "Watch the door!",
          createdAt: now + 10,
        },
      ],
    },
    response: new Response(),
  } as never);
  dispose = render(
    () => createComponent(CompanionReactions, { sessionId: "s1", open: false, onClose: () => {} }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(4000);
  expect(get).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/reactions", {
    params: { path: { id: "s1" }, query: { after: now } },
  });
  expect(document.body.textContent).toContain("Hermes: Watch the door!");
  // The next poll asks only for newer entries.
  await vi.advanceTimersByTimeAsync(4000);
  expect(get).toHaveBeenLastCalledWith("/api/v1/companion/sessions/{id}/reactions", {
    params: { path: { id: "s1" }, query: { after: now, afterId: 7 } },
  });
  // The same entry is not shown twice.
  expect(document.body.textContent?.match(/Watch the door!/g)).toHaveLength(1);
});

it("stays hidden without a session", async () => {
  vi.useFakeTimers();
  const get = vi.spyOn(client, "GET");
  dispose = render(
    () =>
      createComponent(CompanionReactions, { sessionId: undefined, open: false, onClose: () => {} }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(8000);
  expect(get).not.toHaveBeenCalled();
  expect(document.body.textContent).toBe("");
});

const feed = (reactions: unknown[] = [], mode = "on") =>
  ({
    data: { companions: ["Claude"], mode, reactions },
    response: new Response(),
  }) as never;

it("lets the viewer write to companions and choose a mode", async () => {
  vi.useFakeTimers();
  localStorage.removeItem("mbl.companion.mode");
  vi.spyOn(client, "GET").mockResolvedValue(feed());
  const put = vi
    .spyOn(client, "PUT")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: {
      id: 3,
      sessionId: "s1",
      botName: "Viewer",
      timestampMs: 5000,
      commentary: "Who is Amanda?",
      createdAt: Date.now(),
      fromViewer: true,
    },
    response: new Response(),
  } as never);
  dispose = render(
    () => createComponent(CompanionReactions, { sessionId: "s1", open: true, onClose: () => {} }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(4000);
  expect(document.querySelector("section strong")?.textContent).toBe("Chat · Claude");

  const input = document.querySelector("input") as HTMLInputElement;
  input.value = "Who is Amanda?";
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(post).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/messages", {
    params: { path: { id: "s1" } },
    body: { text: "Who is Amanda?" },
  });
  expect(document.body.textContent).toContain("You: Who is Amanda?");
  expect(input.value).toBe("");

  const repliesOnly = [...document.querySelectorAll("button")].find(
    (b) => b.textContent === "Replies only",
  );
  repliesOnly?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(put).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/mode", {
    params: { path: { id: "s1" } },
    body: { mode: "replies", frequency: "medium" },
  });
  expect(repliesOnly?.getAttribute("aria-pressed")).toBe("true");
  expect(localStorage.getItem("mbl.companion.mode")).toBe("replies");
});

it("keeps the old mode and explains when the change fails", async () => {
  vi.useFakeTimers();
  localStorage.removeItem("mbl.companion.mode");
  vi.spyOn(client, "GET").mockResolvedValue(feed());
  vi.spyOn(client, "PUT").mockResolvedValue({
    response: new Response(null, { status: 404 }),
  } as never);
  dispose = render(
    () => createComponent(CompanionReactions, { sessionId: "s1", open: true, onClose: () => {} }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(4000);
  const off = [...document.querySelectorAll("button")].find(
    (b) => b.textContent === "Comments off",
  );
  off?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(off?.getAttribute("aria-pressed")).toBe("false");
  expect(document.body.textContent).toContain("Could not change the setting.");
  expect(localStorage.getItem("mbl.companion.mode")).toBeNull();
});

it("sends a stored mode to a new session and hides companion comments when off", async () => {
  vi.useFakeTimers();
  localStorage.setItem("mbl.companion.mode", "off");
  const now = Date.now();
  vi.spyOn(client, "GET").mockResolvedValue(
    feed(
      [
        {
          id: 1,
          sessionId: "s2",
          botName: "Claude",
          timestampMs: 0,
          commentary: "Fun fact",
          createdAt: now + 5,
        },
      ],
      "off",
    ),
  );
  const put = vi
    .spyOn(client, "PUT")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(
    () => createComponent(CompanionReactions, { sessionId: "s2", open: true, onClose: () => {} }),
    document.body,
  );
  expect(put).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/mode", {
    params: { path: { id: "s2" } },
    body: { mode: "off", frequency: "medium" },
  });
  await vi.advanceTimersByTimeAsync(4000);
  expect(document.body.textContent).not.toContain("Fun fact");
  localStorage.removeItem("mbl.companion.mode");
});

it("opens the chat without companions, sets how often and asks for a comment", async () => {
  vi.useFakeTimers();
  localStorage.removeItem("mbl.companion.mode");
  localStorage.removeItem("mbl.companion.frequency");
  let companions: string[] = [];
  let assistantAvailable = false;
  vi.spyOn(client, "GET").mockImplementation(
    async () =>
      ({
        data: {
          companions,
          mode: "on",
          frequency: "medium",
          listening: companions,
          assistantAvailable,
          reactions: [],
        },
        response: new Response(),
      }) as never,
  );
  const put = vi
    .spyOn(client, "PUT")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(
    () => createComponent(CompanionReactions, { sessionId: "s3", open: true, onClose: () => {} }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(0);
  const button = (text: string) =>
    [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === text);
  expect(document.body.textContent).toContain("No companion has joined yet.");
  expect(button("Comment now")?.hasAttribute("disabled")).toBe(true);

  // The built-in assistant counts as someone who will answer.
  assistantAvailable = true;
  await vi.advanceTimersByTimeAsync(2000);
  expect(document.body.textContent).not.toContain("No companion has joined yet.");
  expect(button("Comment now")?.hasAttribute("disabled")).toBe(false);
  assistantAvailable = false;
  companions = ["Claude"];
  await vi.advanceTimersByTimeAsync(2000);
  expect(document.body.textContent).not.toContain("No companion has joined yet.");
  button("High")?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(put).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/mode", {
    params: { path: { id: "s3" } },
    body: { mode: "on", frequency: "high" },
  });
  expect(localStorage.getItem("mbl.companion.frequency")).toBe("high");
  button("Comment now")?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(post).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/nudge", {
    params: { path: { id: "s3" } },
  });
  button("What just happened?")?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(post).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/messages", {
    params: { path: { id: "s3" } },
    body: { text: "What just happened?" },
  });
  localStorage.removeItem("mbl.companion.frequency");
});

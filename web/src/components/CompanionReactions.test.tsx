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
  dispose = render(() => createComponent(CompanionReactions, { sessionId: "s1" }), document.body);
  await vi.advanceTimersByTimeAsync(4000);
  expect(get).toHaveBeenCalledWith("/api/v1/companion/sessions/{id}/reactions", {
    params: { path: { id: "s1" }, query: { after: now } },
  });
  expect(document.body.textContent).toContain("Hermes: Watch the door!");
  // The next poll asks only for newer reactions.
  await vi.advanceTimersByTimeAsync(4000);
  expect(get).toHaveBeenLastCalledWith("/api/v1/companion/sessions/{id}/reactions", {
    params: { path: { id: "s1" }, query: { after: now + 10 } },
  });
});

it("stays hidden without a session", async () => {
  vi.useFakeTimers();
  const get = vi.spyOn(client, "GET");
  dispose = render(
    () => createComponent(CompanionReactions, { sessionId: undefined }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(8000);
  expect(get).not.toHaveBeenCalled();
  expect(document.body.textContent).toBe("");
});

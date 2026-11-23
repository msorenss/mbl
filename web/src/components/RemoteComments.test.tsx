import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";

const showToast = vi.hoisted(() => vi.fn());
const speak = vi.hoisted(() => vi.fn());
vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast }) }));
vi.mock("../utils/speech", () => ({
  readAloud: () => false,
  setReadAloud: vi.fn(),
  readAloudProblem: async () => null,
  speak,
}));

import { CommentWatcher, ReadCommentsAloud } from "./RemoteComments";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.useRealTimers();
  vi.clearAllMocks();
});

it("shows what companions say about the controlled player, or reads it aloud", async () => {
  vi.useFakeTimers();
  const fetch = vi
    .fn()
    // First answer: where the feed stands; nothing old is shown.
    .mockResolvedValueOnce({
      sessionId: "s",
      comments: [{ id: 3, botName: "MBL", commentary: "old" }],
      lastId: 3,
    })
    .mockResolvedValueOnce({
      sessionId: "s",
      comments: [{ id: 4, botName: "MBL", commentary: "Watch the door" }],
      lastId: 4,
    })
    .mockResolvedValueOnce({
      sessionId: "s",
      comments: [{ id: 5, botName: "MBL", commentary: "Told you" }],
      lastId: 5,
    })
    .mockResolvedValue({ sessionId: "s", comments: [], lastId: 5 });
  dispose = render(
    () => [
      createComponent(ReadCommentsAloud, {}),
      createComponent(CommentWatcher, { player: "Living room", fetch }),
    ],
    document.body,
  );
  await vi.advanceTimersByTimeAsync(10);
  expect(fetch).toHaveBeenLastCalledWith(undefined);
  expect(showToast).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(3000);
  expect(fetch).toHaveBeenLastCalledWith(3);
  expect(showToast).toHaveBeenCalledWith({
    title: "MBL · Living room",
    message: "Watch the door",
    type: "info",
  });
  // With reading aloud on, the next comment is spoken instead of popping up.
  const box = document.querySelector("input[type=checkbox]") as HTMLInputElement;
  box.click();
  await vi.advanceTimersByTimeAsync(3000);
  expect(fetch).toHaveBeenLastCalledWith(4);
  expect(speak).toHaveBeenCalledWith("Told you", expect.any(String));
  expect(showToast).toHaveBeenCalledTimes(1);
});

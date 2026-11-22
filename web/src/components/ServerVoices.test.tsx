import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import ServerVoices from "./ServerVoices";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const status = (programInstalled: boolean, download: unknown = null) => ({
  supported: true,
  programInstalled,
  voices: [
    { language: "sv", name: "Lisa (svenska)", installed: false },
    { language: "en", name: "Amy (English, US)", installed: true },
  ],
  download,
});

const button = (text: string) =>
  [...document.querySelectorAll("button")].filter((b) => b.textContent === text);

it("installs Piper first, then voices, after asking", async () => {
  let calls = 0;
  vi.spyOn(client, "GET").mockImplementation((async () => {
    calls += 1;
    return {
      data:
        calls === 1
          ? status(false)
          : status(false, { what: "program", receivedBytes: 13, totalBytes: 26, finished: false }),
      response: new Response(),
    };
  }) as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ response: new Response(null, { status: 202 }) } as never);
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  dispose = render(() => createComponent(ServerVoices, {}), document.body);
  await tick();
  // Voices wait for the program.
  const downloads = button("Download");
  expect(downloads).toHaveLength(2);
  expect(downloads[1].disabled).toBe(true);
  expect(button("Remove")).toHaveLength(1);

  downloads[0].click();
  for (let i = 0; i < 5; i++) await tick();
  expect(confirm.mock.calls[0][0]).toContain("Piper (about 25 MB)");
  expect(post.mock.calls[0][0]).toBe("/api/v1/speech/piper");
  expect(document.body.textContent).toContain("Downloading 50 %");
});

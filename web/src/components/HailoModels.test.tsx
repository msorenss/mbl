import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import HailoModels from "./HailoModels";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text);

const status = (download: unknown = null) => ({
  npu: "hailo10H",
  supported: true,
  reason: null,
  hailortVersion: "5.4.0",
  directory: "/var/lib/mbl/models",
  freeBytes: 60 * 1024 ** 3,
  models: [
    {
      file: "Qwen2-VL-2B-Instruct.hef",
      role: "vision",
      name: "Qwen2-VL 2B",
      description: "Describes the picture.",
      recommended: true,
      sizeBytes: 3255444179,
      installed: false,
      active: false,
    },
    {
      file: "Whisper-Base.hef",
      role: "speech",
      name: "Whisper Base",
      description: "Speech to text.",
      recommended: true,
      sizeBytes: 130206034,
      installed: true,
      active: true,
    },
  ],
  download,
});

it("downloads after a confirmation and shows the progress", async () => {
  vi.spyOn(client, "GET")
    .mockResolvedValueOnce({ data: status(), response: new Response() } as never)
    .mockResolvedValue({
      data: status({
        file: "Qwen2-VL-2B-Instruct.hef",
        receivedBytes: 1627722090,
        totalBytes: 3255444179,
        finished: false,
        error: null,
      }),
      response: new Response(),
    } as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ response: new Response(null, { status: 202 }) } as never);
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  dispose = render(() => createComponent(HailoModels, {}), document.body);
  await tick();

  const text = document.body.textContent ?? "";
  expect(text).toContain("HailoRT: 5.4.0");
  expect(text).toContain("3.0 GB");
  expect(text).toContain("In use");
  expect(button("Remove")).toBeDefined();

  button("Download")?.click();
  for (let i = 0; i < 5; i++) await tick();
  expect(confirm.mock.calls[0][0]).toContain("Qwen2-VL-2B-Instruct.hef (3.0 GB)");
  expect(post.mock.calls[0][1]).toMatchObject({
    params: { path: { file: "Qwen2-VL-2B-Instruct.hef" } },
  });
  expect(document.body.textContent).toContain("Downloading 50 %");
});

it("explains why nothing can be installed", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      ...status(),
      npu: "hailo8",
      supported: false,
      reason: "These models need a Hailo-10H; a Hailo-8 runs only detection models such as YOLO.",
    },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(HailoModels, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("These models need a Hailo-10H");
  expect(button("Download")?.disabled).toBe(true);
});

import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import ChannelEditor from "./ChannelEditor";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const channel = {
  id: 4,
  name: "One",
  channelNumber: "1",
  epgChannelId: "one.se",
  isFavorite: false,
  isHidden: false,
  isVirtual: false,
  streamUrl: "",
  createdAt: 0,
  updatedAt: 0,
};
const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text) as HTMLButtonElement;
const field = (label: string) =>
  [...document.querySelectorAll("label")]
    .find((l) => l.textContent?.trim().startsWith(label))
    ?.querySelector("input") as HTMLInputElement;
const type = (input: HTMLInputElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
};

it("saves edits and unlinks the guide with an empty ID", async () => {
  const patch = vi
    .spyOn(client, "PATCH")
    .mockResolvedValue({ data: channel, response: new Response() } as never);
  const changed = vi.fn();
  dispose = render(
    () => createComponent(ChannelEditor, { channel, onChanged: changed }),
    document.body,
  );
  button("Edit").click();
  await tick();
  type(field("Name"), "Channel One");
  type(field("Guide channel ID"), " ");
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  expect(patch).toHaveBeenCalledWith("/api/v1/livetv/channels/{id}", {
    params: { path: { id: 4 } },
    body: {
      name: "Channel One",
      channelNumber: "1",
      epgChannelId: "",
      isFavorite: false,
      isHidden: false,
    },
  });
  expect(changed).toHaveBeenCalled();
});

it("asks before deleting and refuses an empty name", async () => {
  const del = vi
    .spyOn(client, "DELETE")
    .mockResolvedValue({ data: { success: true }, response: new Response() } as never);
  const patch = vi.spyOn(client, "PATCH");
  dispose = render(
    () => createComponent(ChannelEditor, { channel, onChanged: () => {} }),
    document.body,
  );
  button("Edit").click();
  await tick();
  type(field("Name"), "  ");
  document.querySelector("form")?.dispatchEvent(new Event("submit", { cancelable: true }));
  await tick();
  expect(document.body.textContent).toContain("Name and number are required.");
  expect(patch).not.toHaveBeenCalled();

  button("Delete").click();
  await tick();
  expect(del).not.toHaveBeenCalled();
  button("Delete channel for good").click();
  await tick();
  expect(del).toHaveBeenCalledWith("/api/v1/livetv/channels/{id}", { params: { path: { id: 4 } } });
});

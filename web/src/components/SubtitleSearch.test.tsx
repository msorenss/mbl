import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import SubtitleSearch from "./SubtitleSearch";

vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "sv" }) }));

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text) as
    | HTMLButtonElement
    | undefined;

it("searches in the viewer's language and downloads the chosen subtitle", async () => {
  const get = vi.spyOn(client, "GET").mockResolvedValue({
    data: [
      {
        fileId: 8,
        language: "sv",
        release: "Heat.WEB",
        fileName: "heat.srt",
        downloads: 99,
        rating: 0,
        hearingImpaired: true,
      },
    ],
    response: new Response(),
  } as never);
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: { partId: 1, streamIndex: 3, language: "sv", fileName: "Heat.sv.srt", remaining: 4 },
    response: new Response(),
  } as never);
  const downloaded = vi.fn();
  dispose = render(
    () =>
      createComponent(SubtitleSearch, {
        itemId: 5,
        title: "Heat",
        onClose: () => undefined,
        onDownloaded: downloaded,
      }),
    document.body,
  );
  await tick();
  button("Search")?.click();
  await tick();
  expect(get.mock.calls[0][1]).toMatchObject({
    params: { path: { id: 5 }, query: { language: "sv" } },
  });
  expect(document.body.textContent).toContain("Heat.WEB");
  expect(document.body.textContent).toContain("Hearing impaired");

  button("Download")?.click();
  await tick();
  expect(post.mock.calls[0][1]).toMatchObject({
    body: { fileId: 8, language: "sv", hearingImpaired: true },
  });
  expect(downloaded).toHaveBeenCalledWith(expect.objectContaining({ streamIndex: 3 }));
  expect(document.body.textContent).toContain("Saved as Heat.sv.srt.");
});

it("shows the server's reason when searching is not possible", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    error: { message: "validation: Subtitle search is not set up" },
    response: new Response(null, { status: 422 }),
  } as never);
  dispose = render(
    () => createComponent(SubtitleSearch, { itemId: 5, title: "Heat", onClose: () => undefined }),
    document.body,
  );
  await tick();
  button("Search")?.click();
  await tick();
  expect(document.querySelector("[role=alert]")?.textContent).toBe("Subtitle search is not set up");
});

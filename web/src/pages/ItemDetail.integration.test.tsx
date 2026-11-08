import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { ItemDetail } from "./ItemDetail";

const state = vi.hoisted(() => ({
  id: "1",
  revision: (): number => 0,
  play: vi.fn(),
  navigate: vi.fn(),
}));
vi.mock("@solidjs/router", () => ({
  useParams: () => ({
    get id() {
      return state.id;
    },
  }),
  useNavigate: () => state.navigate,
}));
vi.mock("../context/PlayerContext", () => ({
  usePlayer: () => ({ playItem: state.play, playbackRevision: () => state.revision() }),
}));
vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast: vi.fn() }) }));
vi.mock("../i18n", () => ({ useI18n: () => ({ t: (s: string) => s }) }));
vi.mock("../api/deviceProfile", () => ({
  getDeviceProfileInput: () => ({
    name: "chrome",
    browserCodecs: { mp4: ["ac3"], webm: [], hls: [] },
  }),
}));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  state.play.mockClear();
  state.navigate.mockClear();
  state.revision = () => 0;
});
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
it("opens seasons instead of playing their nonexistent media and reloads watch state after stop", async () => {
  const [revision, setRevision] = createSignal(0);
  state.revision = revision;
  const get = vi.spyOn(client, "GET").mockImplementation(async (path: string) => {
    if (path.endsWith("/children"))
      return {
        data: [{ id: 2, kind: "season", title: "Season 2", idx: 2, viewOffsetMs: 0 }],
        response: new Response(),
      } as never;
    if (path.endsWith("playback-preview")) return { response: new Response() } as never;
    return {
      data: {
        id: 1,
        kind: "show",
        title: "Dark Matter",
        versions: [],
        artwork: [],
        watch: { viewOffsetMs: 0, viewCount: 0 },
        genres: [],
        studios: [],
        people: [],
        tags: [],
        viewOffsetMs: 0,
      },
      response: new Response(),
    } as never;
  });
  dispose = render(() => createComponent(ItemDetail, {}), document.body);
  await tick();
  expect(get).not.toHaveBeenCalledWith("/api/v1/items/{id}/playback-preview", expect.anything());
  expect(document.body.textContent).toContain("seasons");
  expect(document.querySelector('a[href="/item/2"]')).toBeTruthy();
  const season = [...document.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === "Season 2",
  );
  expect(season).toBeTruthy();
  season?.click();
  expect(state.navigate).toHaveBeenCalledWith("/item/2");
  expect(state.play).not.toHaveBeenCalled();
  const before = get.mock.calls.filter((c) => c[0] === "/api/v1/items/{id}").length;
  setRevision(1);
  await tick();
  expect(get.mock.calls.filter((c) => c[0] === "/api/v1/items/{id}").length).toBe(before + 1);
});

import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { LibraryDetail } from "./LibraryDetail";

const route = vi.hoisted(() => ({ id: (): string => "1" }));
vi.mock("@solidjs/router", () => ({
  useNavigate: () => vi.fn(),
  useParams: () => ({
    get id() {
      return route.id();
    },
  }),
}));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ isAdmin: false }) }),
}));
vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast: vi.fn() }) }));
vi.mock("@tanstack/solid-virtual", () => ({
  createVirtualizer: () => ({ getVirtualItems: () => [], getTotalSize: () => 0 }),
}));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
it("loads the next page and reloads library metadata after route changes", async () => {
  const [id, setId] = createSignal("1");
  route.id = id;
  const get = vi.spyOn(client, "GET").mockImplementation(async (path, options) => {
    if (path === "/api/v1/libraries/{id}")
      return {
        data: { id: Number(id()), name: `Library ${id()}`, kind: "movie" },
        response: new Response(),
      } as never;
    const cursor = (options as { params?: { query?: { cursor?: string } } })?.params?.query?.cursor;
    return {
      data: { items: [], nextCursor: cursor ? null : "page-two" },
      response: new Response(),
    } as never;
  });
  dispose = render(() => createComponent(LibraryDetail, {}), document.body);
  await tick();
  const more = [...document.querySelectorAll("button")].find((b) =>
    b.textContent?.includes("Load more"),
  );
  expect(more).toBeTruthy();
  more?.click();
  await tick();
  expect(get).toHaveBeenCalledWith(
    "/api/v1/libraries/{id}/items",
    expect.objectContaining({
      params: expect.objectContaining({ query: expect.objectContaining({ cursor: "page-two" }) }),
    }),
  );
  setId("2");
  await tick();
  expect(document.body.textContent).toContain("Library 2");
  expect(get).toHaveBeenCalledWith(
    "/api/v1/libraries/{id}/items",
    expect.objectContaining({
      params: expect.objectContaining({
        path: { id: 2 },
        query: expect.objectContaining({ cursor: undefined }),
      }),
    }),
  );
});

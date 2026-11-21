import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import NavEditor from "./NavEditor";

const theme = vi.hoisted(() => ({
  order: [] as string[],
  hidden: [] as string[],
  setNavLayout: vi.fn(),
}));
vi.mock("../context/ThemeContext", async () => {
  const { createSignal } = await import("solid-js");
  const [order, setOrder] = createSignal<string[]>([]);
  const [hidden, setHidden] = createSignal<string[]>([]);
  return {
    useTheme: () => ({
      navOrder: order,
      hiddenNav: hidden,
      setNavLayout: (o: string[], h: string[]) => {
        theme.setNavLayout(o, h);
        setOrder(o);
        setHidden(h);
      },
    }),
  };
});
vi.mock("../i18n", () => ({ useI18n: () => ({ t: (s: string) => s }) }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  theme.setNavLayout.mockClear();
});

const links = [
  { href: "/", icon: "home" as const, label: () => "Home" },
  { href: "/libraries", icon: "libraries" as const, label: () => "Libraries" },
  { href: "/livetv", icon: "livetv" as const, label: () => "Live TV" },
];

const labels = () =>
  [...document.querySelectorAll("li")].map((li) => li.textContent?.replace(/[⋮↑↓]/g, "").trim());

it("hides entries, reorders them and resets", async () => {
  const [open] = createSignal(true);
  const onClose = vi.fn();
  dispose = render(
    () => open() && createComponent(NavEditor, { groups: [{ title: "Browse", links }], onClose }),
    document.body,
  );
  expect(labels()).toEqual(["Home", "Libraries", "Live TV"]);
  // Home always stays: it has no visibility toggle.
  expect(document.querySelector("input[aria-label='Show in menu: Home']")).toBeNull();

  (document.querySelector("input[aria-label='Show in menu: Live TV']") as HTMLInputElement).click();
  expect(theme.setNavLayout).toHaveBeenLastCalledWith([], ["/livetv"]);

  (document.querySelector("button[aria-label='Move up: Live TV']") as HTMLButtonElement).click();
  expect(theme.setNavLayout).toHaveBeenLastCalledWith(["/", "/livetv", "/libraries"], ["/livetv"]);
  expect(labels()).toEqual(["Home", "Live TV", "Libraries"]);

  [...document.querySelectorAll("button")]
    .find((b) => b.textContent === "Reset to default")
    ?.click();
  expect(theme.setNavLayout).toHaveBeenLastCalledWith([], []);
  expect(labels()).toEqual(["Home", "Libraries", "Live TV"]);
});

import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import WebTv from "./WebTv";

vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ isAdmin: true }) }),
}));

vi.mock("../context/ThemeContext", () => ({
  useTheme: () => ({ locale: () => "en" }),
}));

vi.mock("../components/OnlineSearch", () => ({
  OnlineSearch: (props: { admin?: boolean }) => (
    <div data-testid="online-search">OnlineSearch embedded admin={String(props.admin)}</div>
  ),
}));

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

it("renders WebTV heading, description and embeds OnlineSearch", () => {
  dispose = render(() => createComponent(WebTv, {}), document.body);

  expect(document.querySelector("h1")?.textContent).toBe("WebTV");
  expect(document.querySelector('[data-testid="online-search"]')?.textContent).toContain(
    "admin=true",
  );
});

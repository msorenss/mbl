import { render } from "solid-js/web";
import { describe, expect, it } from "vitest";
import { ThemeProvider, useTheme } from "./ThemeContext";

function TestConsumer() {
  const { theme, setTheme, accent, setAccent } = useTheme();
  return (
    <div>
      <span id="theme-val">{theme()}</span>
      <span id="accent-val">{accent()}</span>
      <button type="button" id="set-oled" onClick={() => setTheme("oled")}>
        OLED
      </button>
      <button type="button" id="set-light" onClick={() => setTheme("light")}>
        Light
      </button>
      <button type="button" id="set-hc" onClick={() => setTheme("high-contrast")}>
        HC
      </button>
      <button type="button" id="set-cyan" onClick={() => setAccent("cyan")}>
        Cyan
      </button>
      <button type="button" id="set-rose" onClick={() => setAccent("rose")}>
        Rose
      </button>
    </div>
  );
}

describe("ThemeContext", () => {
  it("initializes with dark theme and amber accent on documentElement", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    render(
      () => (
        <ThemeProvider>
          <TestConsumer />
        </ThemeProvider>
      ),
      container,
    );

    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(document.documentElement.getAttribute("data-accent")).toBe("amber");
  });

  it("switches through all 4 themes and applies data-theme", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    render(
      () => (
        <ThemeProvider>
          <TestConsumer />
        </ThemeProvider>
      ),
      container,
    );

    const oledBtn = document.getElementById("set-oled") as HTMLButtonElement;
    oledBtn.click();
    expect(document.documentElement.getAttribute("data-theme")).toBe("oled");

    const lightBtn = document.getElementById("set-light") as HTMLButtonElement;
    lightBtn.click();
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");

    const hcBtn = document.getElementById("set-hc") as HTMLButtonElement;
    hcBtn.click();
    expect(document.documentElement.getAttribute("data-theme")).toBe("high-contrast");
  });

  it("switches accents and applies data-accent", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    render(
      () => (
        <ThemeProvider>
          <TestConsumer />
        </ThemeProvider>
      ),
      container,
    );

    const cyanBtn = document.getElementById("set-cyan") as HTMLButtonElement;
    cyanBtn.click();
    expect(document.documentElement.getAttribute("data-accent")).toBe("cyan");

    const roseBtn = document.getElementById("set-rose") as HTMLButtonElement;
    roseBtn.click();
    expect(document.documentElement.getAttribute("data-accent")).toBe("rose");
  });
});

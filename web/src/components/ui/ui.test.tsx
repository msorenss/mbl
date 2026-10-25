import { render } from "solid-js/web";
import { describe, expect, it, vi } from "vitest";
import { Badge } from "./Badge";
import { Button } from "./Button";
import { Chip } from "./Chip";
import { Hero } from "./Hero";
import { PinPad } from "./PinPad";
import { ProgressRing } from "./ProgressRing";
import { SegmentedControl } from "./SegmentedControl";
import { Skeleton } from "./Skeleton";

describe("UI Component Library", () => {
  it("renders Button with proper variant and disabled state", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const handleClick = vi.fn();

    render(
      () => (
        <Button variant="danger" size="lg" onClick={handleClick}>
          Delete Item
        </Button>
      ),
      container,
    );

    const btn = container.querySelector("button") as HTMLButtonElement;
    expect(btn).toBeDefined();
    expect(btn.textContent).toContain("Delete Item");
    btn.click();
    expect(handleClick).toHaveBeenCalledTimes(1);
  });

  it("renders Badge with direct-play and media tags", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);

    render(
      () => (
        <div>
          <Badge variant="direct-play">Direct Play</Badge>
          <Badge variant="accent">4K HDR</Badge>
        </div>
      ),
      container,
    );

    expect(container.textContent).toContain("Direct Play");
    expect(container.textContent).toContain("4K HDR");
  });

  it("renders Chip with interactive click and clear handlers", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const onClear = vi.fn();

    render(() => <Chip label="Action" active onClear={onClear} />, container);

    expect(container.textContent).toContain("Action");
    const clearBtn = container.querySelector('[role="button"]') as HTMLElement;
    clearBtn.click();
    expect(onClear).toHaveBeenCalledTimes(1);
  });

  it("renders SegmentedControl and switches active option", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const onChange = vi.fn();

    render(
      () => (
        <SegmentedControl
          value="dark"
          options={[
            { value: "dark", label: "Dark" },
            { value: "oled", label: "OLED" },
          ]}
          onChange={onChange}
        />
      ),
      container,
    );

    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBe(2);
    buttons[1].click();
    expect(onChange).toHaveBeenCalledWith("oled");
  });

  it("renders ProgressRing with correct calculation and label", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);

    render(() => <ProgressRing progress={0.75} label="75%" />, container);

    expect(container.textContent).toContain("75%");
    expect(container.querySelector("svg")).toBeDefined();
  });

  it("renders PinPad and emits onComplete after 4 digits", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const onComplete = vi.fn();

    render(() => <PinPad onComplete={onComplete} />, container);

    const buttons = container.querySelectorAll("button");
    // Find digit buttons
    const b1 = Array.from(buttons).find((b) => b.textContent?.trim() === "1");
    const b2 = Array.from(buttons).find((b) => b.textContent?.trim() === "2");
    const b3 = Array.from(buttons).find((b) => b.textContent?.trim() === "3");
    const b4 = Array.from(buttons).find((b) => b.textContent?.trim() === "4");

    b1?.click();
    b2?.click();
    b3?.click();
    b4?.click();

    expect(onComplete).toHaveBeenCalledWith("1234");
  });

  it("renders Skeleton with animation attributes", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);

    render(() => <Skeleton width="200px" height="300px" />, container);

    const skel = container.querySelector(".mbl-skeleton") as HTMLElement;
    expect(skel).toBeDefined();
    expect(skel.style.width).toBe("200px");
    expect(skel.style.height).toBe("300px");
  });

  it("renders Hero with ambient styling and title", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const onPlay = vi.fn();

    render(
      () => (
        <Hero
          title="Interstellar"
          tagline="Mankind was born on Earth. It was never meant to die here."
          year={2014}
          genres={["Sci-Fi", "Adventure"]}
          badges={["4K", "IMAX"]}
          onPlay={onPlay}
        />
      ),
      container,
    );

    expect(container.textContent).toContain("Interstellar");
    expect(container.textContent).toContain("2014");
    expect(container.textContent).toContain("Sci-Fi");
    expect(container.textContent).toContain("Play Now");
  });
});

import { A } from "@solidjs/router";
import { createSignal, type JSX } from "solid-js";

export interface HubRowProps {
  title: string;
  linkHref?: string;
  linkLabel?: string;
  children: JSX.Element;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  canMoveUp?: boolean;
  canMoveDown?: boolean;
}

export function HubRow(props: HubRowProps) {
  let scrollContainerRef: HTMLDivElement | undefined;
  const [canScrollLeft, setCanScrollLeft] = createSignal(false);
  const [canScrollRight, setCanScrollRight] = createSignal(true);

  const checkScroll = () => {
    if (!scrollContainerRef) return;
    const { scrollLeft, scrollWidth, clientWidth } = scrollContainerRef;
    setCanScrollLeft(scrollLeft > 10);
    setCanScrollRight(scrollLeft + clientWidth < scrollWidth - 10);
  };

  const scrollBy = (offset: number) => {
    if (!scrollContainerRef) return;
    scrollContainerRef.scrollBy({ left: offset, behavior: "smooth" });
  };

  return (
    <section style={{ "margin-bottom": "var(--space-8)" }} class="mbl-hub-row">
      {/* Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "var(--space-3)",
          padding: "0 var(--space-4)",
        }}
      >
        <div style={{ display: "flex", "align-items": "center", gap: "var(--space-3)" }}>
          <h2
            style={{
              margin: 0,
              "font-size": "var(--font-size-xl)",
              "font-weight": "var(--font-weight-bold)",
              color: "var(--text-primary)",
            }}
          >
            {props.title}
          </h2>
          {props.linkHref && (
            <A
              href={props.linkHref}
              style={{
                color: "var(--accent)",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-medium)",
              }}
            >
              {props.linkLabel || "View All ›"}
            </A>
          )}
        </div>

        {/* Action Controls & Scroll Buttons */}
        <div style={{ display: "flex", "align-items": "center", gap: "var(--space-2)" }}>
          {(props.onMoveUp || props.onMoveDown) && (
            <div style={{ display: "flex", gap: "2px", "margin-right": "var(--space-2)" }}>
              {props.onMoveUp && (
                <button
                  type="button"
                  title="Move row up"
                  disabled={!props.canMoveUp}
                  onClick={() => props.onMoveUp?.()}
                  style={{
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    color: "var(--text-secondary)",
                    "border-radius": "var(--radius-sm)",
                    padding: "2px 6px",
                    cursor: props.canMoveUp ? "pointer" : "default",
                    opacity: props.canMoveUp ? "1" : "0.3",
                  }}
                >
                  ↑
                </button>
              )}
              {props.onMoveDown && (
                <button
                  type="button"
                  title="Move row down"
                  disabled={!props.canMoveDown}
                  onClick={() => props.onMoveDown?.()}
                  style={{
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    color: "var(--text-secondary)",
                    "border-radius": "var(--radius-sm)",
                    padding: "2px 6px",
                    cursor: props.canMoveDown ? "pointer" : "default",
                    opacity: props.canMoveDown ? "1" : "0.3",
                  }}
                >
                  ↓
                </button>
              )}
            </div>
          )}

          <button
            type="button"
            aria-label="Scroll left"
            disabled={!canScrollLeft()}
            onClick={() => scrollBy(-400)}
            style={{
              width: "32px",
              height: "32px",
              "border-radius": "var(--radius-full)",
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              cursor: canScrollLeft() ? "pointer" : "default",
              opacity: canScrollLeft() ? "1" : "0.3",
              transition: "all var(--duration-fast) var(--ease-standard)",
            }}
          >
            ‹
          </button>
          <button
            type="button"
            aria-label="Scroll right"
            disabled={!canScrollRight()}
            onClick={() => scrollBy(400)}
            style={{
              width: "32px",
              height: "32px",
              "border-radius": "var(--radius-full)",
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              cursor: canScrollRight() ? "pointer" : "default",
              opacity: canScrollRight() ? "1" : "0.3",
              transition: "all var(--duration-fast) var(--ease-standard)",
            }}
          >
            ›
          </button>
        </div>
      </div>

      {/* Items Scroll Track */}
      <div
        ref={scrollContainerRef}
        onScroll={checkScroll}
        style={{
          display: "flex",
          gap: "var(--space-4)",
          "overflow-x": "auto",
          padding: "var(--space-2) var(--space-4)",
          "scrollbar-width": "none",
          "-ms-overflow-style": "none",
        }}
      >
        {props.children}
      </div>
    </section>
  );
}

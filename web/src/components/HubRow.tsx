import { For, Show } from "solid-js";
import type { components } from "../api/schema";
import { PosterCard } from "./PosterCard";

type Hub = components["schemas"]["Hub"];

export interface HubRowProps {
  hub: Hub;
}

export function HubRow(props: HubRowProps) {
  let scrollContainerRef: HTMLDivElement | undefined;

  const scrollLeft = () => {
    if (scrollContainerRef) {
      scrollContainerRef.scrollBy({ left: -400, behavior: "smooth" });
    }
  };

  const scrollRight = () => {
    if (scrollContainerRef) {
      scrollContainerRef.scrollBy({ left: 400, behavior: "smooth" });
    }
  };

  const items = () => props.hub.items || [];

  return (
    <Show when={items().length > 0}>
      <div style={{ "margin-bottom": "36px" }}>
        {/* Hub Header */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            "margin-bottom": "14px",
          }}
        >
          <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
            {props.hub.title}
          </h2>

          <div style={{ display: "flex", gap: "6px" }}>
            <button
              type="button"
              onClick={scrollLeft}
              title="Scroll left"
              style={{
                width: "28px",
                height: "28px",
                "border-radius": "50%",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                color: "var(--text-secondary)",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                cursor: "pointer",
                "font-size": "12px",
              }}
            >
              &larr;
            </button>
            <button
              type="button"
              onClick={scrollRight}
              title="Scroll right"
              style={{
                width: "28px",
                height: "28px",
                "border-radius": "50%",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                color: "var(--text-secondary)",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                cursor: "pointer",
                "font-size": "12px",
              }}
            >
              &rarr;
            </button>
          </div>
        </div>

        {/* Horizontal Scroll Area */}
        <div
          ref={scrollContainerRef}
          style={{
            display: "flex",
            gap: "16px",
            "overflow-x": "auto",
            "padding-bottom": "12px",
            "scrollbar-width": "thin",
            "scrollbar-color": "var(--border) transparent",
          }}
        >
          <For each={items()}>
            {(item) => (
              <div style={{ width: "160px", "flex-shrink": 0 }}>
                <PosterCard item={item} />
              </div>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
}
export default HubRow;

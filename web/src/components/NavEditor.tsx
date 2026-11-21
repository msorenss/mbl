import { createSignal, For, Show } from "solid-js";
import { useTheme } from "../context/ThemeContext";
import { ALWAYS_VISIBLE, arrangeNav, moveInOrder } from "../utils/navLayout";
import { AppIcon, type AppIconName, Button, Dialog } from "./ui";

interface Link {
  href: string;
  icon: AppIconName;
  label: () => string;
}

/**
 * Edit the menu: show or hide entries and change their order within each group. Saved to
 * the user's preferences, so every device shows the same menu.
 */
export default function NavEditor(props: {
  groups: { title: string; links: Link[] }[];
  onClose: () => void;
}) {
  const { navOrder, hiddenNav, setNavLayout } = useTheme();
  const [dragging, setDragging] = createSignal<string>();

  const groupOrder = (links: Link[]) =>
    arrangeNav(links, navOrder(), hiddenNav(), true).map((l) => l.href);

  const move = (links: Link[], href: string, step: -1 | 1) =>
    setNavLayout(moveInOrder(groupOrder(links), navOrder(), href, step), hiddenNav());

  const drop = (links: Link[], target: string) => {
    const source = dragging();
    setDragging(undefined);
    if (!source || source === target) return;
    const order = groupOrder(links);
    if (!order.includes(source)) return;
    const without = order.filter((h) => h !== source);
    without.splice(without.indexOf(target), 0, source);
    const others = navOrder().filter((h) => !order.includes(h));
    setNavLayout([...others, ...without], hiddenNav());
  };

  const toggle = (href: string) => {
    const hidden = hiddenNav();
    setNavLayout(
      navOrder(),
      hidden.includes(href) ? hidden.filter((h) => h !== href) : [...hidden, href],
    );
  };

  const iconButton = {
    background: "none",
    border: "1px solid var(--border)",
    "border-radius": "var(--radius-sm)",
    color: "var(--text-primary)",
    cursor: "pointer",
    padding: "2px 8px",
  } as const;

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
      title="Edit menu"
      description="Choose which entries the menu shows and in which order. Drag an entry, or use the arrows."
    >
      <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
        <For each={props.groups}>
          {(group) => (
            <section>
              <h3
                style={{
                  margin: "0 0 6px",
                  "font-size": "12px",
                  "text-transform": "uppercase",
                  color: "var(--text-muted)",
                }}
              >
                {group.title}
              </h3>
              <ul
                style={{
                  margin: 0,
                  padding: 0,
                  "list-style": "none",
                  display: "flex",
                  "flex-direction": "column",
                  gap: "4px",
                }}
              >
                <For each={arrangeNav(group.links, navOrder(), hiddenNav(), true)}>
                  {(link, index) => {
                    const hidden = () =>
                      hiddenNav().includes(link.href) && !ALWAYS_VISIBLE.has(link.href);
                    return (
                      <li
                        draggable={true}
                        onDragStart={() => setDragging(link.href)}
                        onDragOver={(e) => e.preventDefault()}
                        onDrop={() => drop(group.links, link.href)}
                        style={{
                          display: "flex",
                          "align-items": "center",
                          gap: "8px",
                          padding: "6px 8px",
                          "border-radius": "var(--radius-md)",
                          border: "1px solid var(--border)",
                          background:
                            dragging() === link.href ? "var(--accent-light)" : "var(--bg-surface)",
                          opacity: hidden() ? 0.5 : 1,
                          cursor: "grab",
                        }}
                      >
                        <span aria-hidden="true" style={{ color: "var(--text-muted)" }}>
                          ⋮⋮
                        </span>
                        <AppIcon name={link.icon} size={16} />
                        <span style={{ flex: 1 }}>{link.label()}</span>
                        <button
                          type="button"
                          style={iconButton}
                          disabled={index() === 0}
                          aria-label={`Move up: ${link.label()}`}
                          onClick={() => move(group.links, link.href, -1)}
                        >
                          ↑
                        </button>
                        <button
                          type="button"
                          style={iconButton}
                          disabled={index() === group.links.length - 1}
                          aria-label={`Move down: ${link.label()}`}
                          onClick={() => move(group.links, link.href, 1)}
                        >
                          ↓
                        </button>
                        <Show
                          when={!ALWAYS_VISIBLE.has(link.href)}
                          fallback={<span style={{ width: "34px" }} />}
                        >
                          <input
                            type="checkbox"
                            checked={!hidden()}
                            aria-label={`Show in menu: ${link.label()}`}
                            onChange={() => toggle(link.href)}
                          />
                        </Show>
                      </li>
                    );
                  }}
                </For>
              </ul>
            </section>
          )}
        </For>
        <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
          <Button variant="secondary" onClick={() => setNavLayout([], [])}>
            Reset to default
          </Button>
          <Button onClick={() => props.onClose()}>Done</Button>
        </div>
      </div>
    </Dialog>
  );
}

import { useNavigate } from "@solidjs/router";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useAuth } from "../context/AuthContext";
import {
  type AccentColor,
  type AppLocale,
  type ThemeMode,
  useTheme,
} from "../context/ThemeContext";
import { useI18n } from "../i18n";

type ItemSummary = components["schemas"]["ItemSummary"];

export interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

export function CommandPalette(props: CommandPaletteProps) {
  const navigate = useNavigate();
  const { user } = useAuth();
  const { theme, setTheme, accent, setAccent, locale, setLocale } = useTheme();
  const { t } = useI18n();

  const [query, setQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<ItemSummary[]>([]);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [isSearching, setIsSearching] = createSignal(false);
  let inputRef: HTMLInputElement | undefined;

  // Search debounce
  let debounceTimer: any;
  createEffect(() => {
    const q = query().trim();
    clearTimeout(debounceTimer);
    if (!q) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }
    setIsSearching(true);
    debounceTimer = setTimeout(async () => {
      try {
        const res = await client.GET("/api/v1/search", { params: { query: { q } } });
        if (res.data) {
          const flat = (res.data as any[]).flatMap((g) => g.items || []);
          setSearchResults(flat.slice(0, 8));
        }
      } catch {
        setSearchResults([]);
      } finally {
        setIsSearching(false);
      }
    }, 200);
  });

  const staticActions = () => {
    const actions: {
      id: string;
      category: string;
      label: string;
      icon: string;
      run: () => void;
    }[] = [
      {
        id: "nav-home",
        category: "Navigation",
        label: t("home"),
        icon: "🏠",
        run: () => navigate("/"),
      },
      {
        id: "nav-libraries",
        category: "Navigation",
        label: t("libraries"),
        icon: "📚",
        run: () => navigate("/libraries"),
      },
      {
        id: "nav-search",
        category: "Navigation",
        label: t("search"),
        icon: "🔍",
        run: () => navigate("/search"),
      },
      {
        id: "nav-profiles",
        category: "Navigation",
        label: t("switchProfile"),
        icon: "👤",
        run: () => navigate("/profiles"),
      },
    ];

    if (user()?.isAdmin) {
      actions.push(
        {
          id: "admin-users",
          category: "Admin",
          label: t("users"),
          icon: "👥",
          run: () => navigate("/admin/users"),
        },
        {
          id: "admin-ai",
          category: "Admin",
          label: t("aiAgents"),
          icon: "🤖",
          run: () => navigate("/admin/ai-agents"),
        },
        {
          id: "admin-hw",
          category: "Admin",
          label: t("hardware"),
          icon: "⚡",
          run: () => navigate("/admin/hardware"),
        },
      );
    }

    // Themes
    const themes: { id: ThemeMode; label: string }[] = [
      { id: "dark", label: t("dark") },
      { id: "oled", label: t("oled") },
      { id: "light", label: t("light") },
      { id: "high-contrast", label: t("highContrast") },
    ];
    for (const th of themes) {
      actions.push({
        id: `theme-${th.id}`,
        category: t("theme"),
        label: `${th.label}${theme() === th.id ? " ✓" : ""}`,
        icon: "🎨",
        run: () => setTheme(th.id),
      });
    }

    // Accents
    const accents: { id: AccentColor; label: string }[] = [
      { id: "amber", label: t("amber") },
      { id: "indigo", label: t("indigo") },
      { id: "emerald", label: t("emerald") },
      { id: "rose", label: t("rose") },
      { id: "cyan", label: t("cyan") },
      { id: "violet", label: t("violet") },
    ];
    for (const ac of accents) {
      actions.push({
        id: `accent-${ac.id}`,
        category: t("accent"),
        label: `${ac.label}${accent() === ac.id ? " ✓" : ""}`,
        icon: "✨",
        run: () => setAccent(ac.id),
      });
    }

    // Languages
    const locales: { id: AppLocale; label: string }[] = [
      { id: "en", label: "English" },
      { id: "sv", label: "Svenska" },
      { id: "de", label: "Deutsch" },
      { id: "fr", label: "Français" },
      { id: "es", label: "Español" },
    ];
    for (const loc of locales) {
      actions.push({
        id: `lang-${loc.id}`,
        category: t("language"),
        label: `${loc.label}${locale() === loc.id ? " ✓" : ""}`,
        icon: "🌐",
        run: () => setLocale(loc.id),
      });
    }

    const q = query().toLowerCase().trim();
    if (!q) return actions;
    return actions.filter(
      (a) => a.label.toLowerCase().includes(q) || a.category.toLowerCase().includes(q),
    );
  };

  const allItems = () => {
    const results = searchResults().map((item) => ({
      id: `media-${item.id}`,
      category: item.kind === "movie" ? "Movies" : item.kind === "show" ? "TV Shows" : "Media",
      label: `${item.title}${item.year ? ` (${item.year})` : ""}`,
      icon: item.kind === "movie" ? "🎬" : "📺",
      run: () => navigate(`/item/${item.id}`),
    }));
    return [...results, ...staticActions()];
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (!props.open) return;
    if (e.key === "Escape") {
      props.onClose();
      return;
    }
    const items = allItems();
    if (items.length === 0) return;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % items.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + items.length) % items.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const selected = items[selectedIndex()];
      if (selected) {
        selected.run();
        props.onClose();
      }
    }
  };

  createEffect(() => {
    if (props.open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef?.focus(), 50);
    }
  });

  return (
    <Show when={props.open}>
      <div
        role="dialog"
        aria-modal="true"
        onKeyDown={handleKeyDown}
        style={{
          position: "fixed",
          inset: 0,
          "z-index": 200,
          display: "flex",
          "align-items": "flex-start",
          "justify-content": "center",
          "padding-top": "15vh",
          background: "rgba(0, 0, 0, 0.75)",
          "backdrop-filter": "blur(4px)",
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onClose();
        }}
      >
        <div
          style={{
            width: "100%",
            "max-width": "580px",
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-lg)",
            "box-shadow": "var(--shadow-xl)",
            overflow: "hidden",
            display: "flex",
            "flex-direction": "column",
          }}
        >
          {/* Search Input Bar */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "var(--space-3)",
              padding: "var(--space-4) var(--space-5)",
              "border-bottom": "1px solid var(--border)",
            }}
          >
            <span style={{ "font-size": "18px", opacity: 0.6 }}>🔍</span>
            <input
              ref={inputRef}
              type="text"
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
              placeholder={t("searchPlaceholder")}
              style={{
                flex: 1,
                background: "transparent",
                border: "none",
                outline: "none",
                color: "var(--text-primary)",
                "font-size": "var(--font-size-md)",
              }}
            />
            <kbd
              style={{
                padding: "2px 6px",
                "border-radius": "var(--radius-sm)",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                color: "var(--text-muted)",
                "font-size": "var(--font-size-xs)",
              }}
            >
              ESC
            </kbd>
          </div>

          {/* Results List */}
          <div
            style={{
              "max-height": "380px",
              "overflow-y": "auto",
              padding: "var(--space-2)",
            }}
          >
            <Show
              when={allItems().length > 0}
              fallback={
                <div
                  style={{
                    padding: "var(--space-6)",
                    "text-align": "center",
                    color: "var(--text-muted)",
                    "font-size": "var(--font-size-sm)",
                  }}
                >
                  {isSearching() ? "Searching..." : "No results found"}
                </div>
              }
            >
              <For each={allItems()}>
                {(item, index) => {
                  const isSelected = () => index() === selectedIndex();
                  return (
                    <button
                      type="button"
                      role="option"
                      aria-selected={isSelected()}
                      onMouseEnter={() => setSelectedIndex(index())}
                      onClick={() => {
                        item.run();
                        props.onClose();
                      }}
                      style={{
                        width: "100%",
                        border: "none",
                        display: "flex",
                        "align-items": "center",
                        "justify-content": "space-between",
                        padding: "10px var(--space-4)",
                        "border-radius": "var(--radius-sm)",
                        cursor: "pointer",
                        background: isSelected() ? "var(--bg-surface-hover)" : "transparent",
                        color: isSelected() ? "var(--accent)" : "var(--text-primary)",
                        transition: "background var(--duration-fast) var(--ease-standard)",
                        "text-align": "left",
                      }}
                    >
                      <div
                        style={{ display: "flex", "align-items": "center", gap: "var(--space-3)" }}
                      >
                        <span style={{ "font-size": "16px" }}>{item.icon}</span>
                        <span
                          style={{
                            "font-size": "var(--font-size-sm)",
                            "font-weight": "var(--font-weight-medium)",
                          }}
                        >
                          {item.label}
                        </span>
                      </div>
                      <span
                        style={{
                          "font-size": "var(--font-size-xs)",
                          color: "var(--text-muted)",
                          "text-transform": "uppercase",
                        }}
                      >
                        {item.category}
                      </span>
                    </button>
                  );
                }}
              </For>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}

export function useCommandPalette() {
  const [isOpen, setIsOpen] = createSignal(false);

  const open = () => setIsOpen(true);
  const close = () => setIsOpen(false);
  const toggle = () => setIsOpen((prev) => !prev);

  onMount(() => {
    const handleGlobalKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", handleGlobalKey);
    onCleanup(() => window.removeEventListener("keydown", handleGlobalKey));
  });

  return { isOpen, open, close, toggle };
}

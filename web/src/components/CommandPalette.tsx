import { useNavigate } from "@solidjs/router";
import { createEffect, createSignal, For, type JSX, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useAuth } from "../context/AuthContext";
import { type AccentColor, type ThemeMode, useTheme } from "../context/ThemeContext";
import { SUPPORTED_LOCALES, useI18n } from "../i18n";
import { AppIcon } from "./ui";

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
  let searchVersion = 0;
  let debounceTimer: ReturnType<typeof setTimeout>;
  createEffect(() => {
    const q = query().trim();
    const version = ++searchVersion;
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
        if (version !== searchVersion) return;
        if (res.data) {
          const flat = res.data.flatMap((g) => g.items || []);
          setSearchResults(flat.slice(0, 8));
        }
      } catch {
        if (version === searchVersion) setSearchResults([]);
      } finally {
        if (version === searchVersion) setIsSearching(false);
      }
    }, 200);
  });

  const staticActions = () => {
    const actions: {
      id: string;
      category: string;
      label: string;
      icon: JSX.Element;
      run: () => void;
    }[] = [
      {
        id: "nav-home",
        category: t("browse"),
        label: t("home"),
        icon: <AppIcon name="home" size={16} />,
        run: () => navigate("/"),
      },
      {
        id: "nav-libraries",
        category: t("browse"),
        label: t("libraries"),
        icon: <AppIcon name="libraries" size={16} />,
        run: () => navigate("/libraries"),
      },
      {
        id: "nav-search",
        category: t("browse"),
        label: t("search"),
        icon: <AppIcon name="search" size={16} />,
        run: () => navigate("/search"),
      },
      {
        id: "nav-profiles",
        category: t("personal"),
        label: t("switchProfile"),
        icon: <AppIcon name="profiles" size={16} />,
        run: () => navigate("/profiles"),
      },
    ];

    actions.push(
      {
        id: "nav-api",
        category: t("browse"),
        label: t("apiReference"),
        icon: <AppIcon name="api" size={16} />,
        run: () => window.location.assign("/api/docs"),
      },
      {
        id: "nav-requests",
        category: t("personal"),
        label: t("mediaRequests"),
        icon: <AppIcon name="requests" size={16} />,
        run: () => navigate("/requests"),
      },
      {
        id: "nav-collections",
        category: t("personal"),
        label: t("collections"),
        icon: <AppIcon name="collections" size={16} />,
        run: () => navigate("/collections"),
      },
      {
        id: "nav-account",
        category: t("personal"),
        label: t("accountSecurity"),
        icon: <AppIcon name="account" size={16} />,
        run: () => navigate("/account"),
      },
      {
        id: "nav-live",
        category: t("browse"),
        label: t("liveTv"),
        icon: <AppIcon name="livetv" size={16} />,
        run: () => navigate("/livetv"),
      },
      {
        id: "nav-webtv",
        category: t("browse"),
        label: t("webtv"),
        icon: <AppIcon name="webtv" size={16} />,
        run: () => navigate("/webtv"),
      },
    );
    if (user()?.isAdmin) {
      actions.push({
        id: "admin-remote",
        category: t("admin"),
        label: t("remoteAccess"),
        icon: <AppIcon name="remote" size={16} />,
        run: () => navigate("/admin/remote"),
      });
      actions.push(
        {
          id: "admin-nodes",
          category: t("admin"),
          label: t("distributedNodes"),
          icon: <AppIcon name="workers" size={16} />,
          run: () => navigate("/admin/workers"),
        },
        {
          id: "admin-users",
          category: t("admin"),
          label: t("users"),
          icon: <AppIcon name="users" size={16} />,
          run: () => navigate("/admin/users"),
        },
        {
          id: "admin-ai",
          category: t("admin"),
          label: t("aiAgents"),
          icon: <AppIcon name="ai-agents" size={16} />,
          run: () => navigate("/admin/ai-agents"),
        },
        {
          id: "admin-hw",
          category: t("admin"),
          label: t("hardware"),
          icon: <AppIcon name="hardware" size={16} />,
          run: () => navigate("/admin/hardware"),
        },
        {
          id: "admin-settings",
          category: t("admin"),
          label: t("serverSettings"),
          icon: <AppIcon name="settings" size={16} />,
          run: () => navigate("/admin/settings"),
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
        icon: <AppIcon name="theme" size={16} />,
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
        icon: <AppIcon name="accent" size={16} />,
        run: () => setAccent(ac.id),
      });
    }

    // Languages (all 23 supported languages)
    for (const loc of SUPPORTED_LOCALES) {
      actions.push({
        id: `lang-${loc.code}`,
        category: t("language"),
        label: `${loc.nativeName} (${loc.name})${locale() === loc.code ? " ✓" : ""}`,
        icon: <AppIcon name="language" size={16} />,
        run: () => setLocale(loc.code),
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
      category:
        item.kind === "movie" ? t("movies") : item.kind === "show" ? t("tvShows") : t("libraries"),
      label: `${item.title}${item.year ? ` (${item.year})` : ""}`,
      icon: <AppIcon name={item.kind === "movie" ? "collections" : "livetv"} size={16} />,
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
        aria-label={t("quickSearch")}
        onKeyDown={handleKeyDown}
        style={{
          position: "fixed",
          inset: 0,
          "z-index": 200,
          display: "flex",
          "align-items": "flex-start",
          "justify-content": "center",
          padding:
            "max(10dvh, calc(env(safe-area-inset-top) + 16px)) max(14px, env(safe-area-inset-right)) 16px max(14px, env(safe-area-inset-left))",
          background: "rgba(0, 0, 0, 0.74)",
          "backdrop-filter": "blur(6px)",
          animation: "fadeIn var(--duration-fast) var(--ease-decelerate)",
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onClose();
        }}
      >
        <div
          class="mbl-dialog-content"
          style={{
            width: "100%",
            "max-width": "580px",
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-xl)",
            "box-shadow": "var(--shadow-xl)",
            overflow: "hidden",
            display: "flex",
            "flex-direction": "column",
            animation: "mblDialogIn var(--duration-normal) var(--ease-decelerate)",
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
            <span style={{ display: "inline-flex", opacity: 0.65 }} aria-hidden="true">
              <AppIcon name="search" size={18} />
            </span>
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
                "box-shadow": "none",
                padding: "2px 0",
                color: "var(--text-primary)",
                "font-size": "var(--font-size-md)",
              }}
            />
            <kbd
              class="mbl-kbd-hint"
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
            role="listbox"
            style={{
              "max-height": "min(400px, 60dvh)",
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
                  {isSearching() ? t("loading") : t("noResults")}
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
                        <span style={{ display: "inline-flex", "flex-shrink": 0 }}>
                          {item.icon}
                        </span>
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

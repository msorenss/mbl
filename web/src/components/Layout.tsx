import { A, useBeforeLeave, useLocation, useNavigate } from "@solidjs/router";
import { createEffect, createSignal, For, type JSX, onCleanup, onMount, Show } from "solid-js";
import { useAuth } from "../context/AuthContext";
import {
  type AccentColor,
  type AppLocale,
  type ThemeMode,
  useTheme,
} from "../context/ThemeContext";
import { useToast } from "../context/ToastContext";
import { localizeDomSubtree, SUPPORTED_LOCALES, useI18n } from "../i18n";
import {
  canInstallPwa,
  isIosSafariInstallCandidate,
  isStandaloneApp,
  promptInstallPwa,
} from "../utils/pwa";
import { CommandPalette, useCommandPalette } from "./CommandPalette";
import { ErrorBoundary } from "./ErrorBoundary";
import {
  AppIcon,
  type AppIconName,
  Button,
  DropdownMenu,
  type DropdownMenuItem,
  SegmentedControl,
  Select,
  Sheet,
  Switch,
} from "./ui";

type SidebarRailMode = "auto" | "expanded" | "collapsed";

interface NavItemDef {
  href: string;
  icon: AppIconName;
  label: () => string;
}

export function Layout(props: { children?: JSX.Element }) {
  const { user, logout } = useAuth();
  const { showToast } = useToast();
  const navigate = useNavigate();
  const location = useLocation();
  const { theme, setTheme, accent, setAccent, locale, setLocale, reducedMotion, setReducedMotion } =
    useTheme();
  const { t } = useI18n();
  const { isOpen: isCommandOpen, open: openCommand, close: closeCommand } = useCommandPalette();

  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [mobileMoreOpen, setMobileMoreOpen] = createSignal(false);
  const [isMobile, setIsMobile] = createSignal(false);
  const [isTvMode, setIsTvMode] = createSignal(false);
  const [railMode, setRailMode] = createSignal<SidebarRailMode>(
    (typeof localStorage !== "undefined"
      ? (localStorage.getItem("mbl.sidebarMode") as SidebarRailMode | null)
      : null) || "auto",
  );

  let mainRef: HTMLElement | undefined;

  const toggleSidebar = () => {
    const tabletViewport =
      typeof window !== "undefined" && window.innerWidth >= 769 && window.innerWidth <= 1080;
    const cur = railMode();
    const currentlyCompact = cur === "collapsed" || (cur === "auto" && tabletViewport);
    const next: SidebarRailMode = currentlyCompact ? "expanded" : "collapsed";
    setRailMode(next);
    try {
      localStorage.setItem("mbl.sidebarMode", next);
    } catch {
      // Ignore storage quota / private mode errors
    }
  };

  onMount(() => {
    const mobileMql = window.matchMedia?.("(max-width: 768px)");
    if (mobileMql) {
      setIsMobile(mobileMql.matches);
      const onMobileChange = (e: MediaQueryListEvent) => setIsMobile(e.matches);
      mobileMql.addEventListener?.("change", onMobileChange);
      onCleanup(() => mobileMql.removeEventListener?.("change", onMobileChange));
    }

    // Detect TV mode from query parameter ?tv=1 or user agent
    const params = new URLSearchParams(window.location.search);
    const tvParam = params.get("tv") === "1";
    const tvUa = /smart-tv|googletv|appletv|hbbtv|pov_tv|netcast.tv/i.test(navigator.userAgent);
    if (tvParam || tvUa) {
      setIsTvMode(true);
      document.documentElement.setAttribute("data-tv", "true");
    }

    // Spatial navigation for TV mode
    const handleTvKeys = (e: KeyboardEvent) => {
      if (!isTvMode()) return;
      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
        const focusable = Array.from(
          document.querySelectorAll<HTMLElement>(
            'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
          ),
        ).filter((el) => el.offsetParent !== null);
        const curIdx = focusable.indexOf(document.activeElement as HTMLElement);
        if (curIdx === -1 && focusable.length > 0) {
          focusable[0].focus();
        }
      }
    };
    window.addEventListener("keydown", handleTvKeys);
    onCleanup(() => window.removeEventListener("keydown", handleTvKeys));

    let domObserver: MutationObserver | undefined;
    if (typeof MutationObserver !== "undefined") {
      let scheduled = false;
      let isLocalizing = false;
      domObserver = new MutationObserver(() => {
        if (isLocalizing || scheduled) return;
        scheduled = true;
        queueMicrotask(() => {
          scheduled = false;
          isLocalizing = true;
          try {
            localizeDomSubtree(document.body, locale());
          } finally {
            isLocalizing = false;
          }
        });
      });
      domObserver.observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
        attributes: true,
        attributeFilter: ["placeholder", "title", "aria-label"],
      });
      onCleanup(() => domObserver?.disconnect());
    }
  });

  createEffect(() => {
    const curLoc = locale();
    void location.pathname;
    if (typeof document !== "undefined" && document.body) {
      queueMicrotask(() => localizeDomSubtree(document.body, curLoc));
    }
  });

  // Progressive enhancement: Same-document View Transitions API on route navigation
  const docWithVt = (typeof document !== "undefined" ? document : undefined) as
    | (Document & {
        startViewTransition?: (cb: () => void) => { finished: Promise<void> };
      })
    | undefined;

  useBeforeLeave((e) => {
    if (e.defaultPrevented || !docWithVt?.startViewTransition) return;
    const prefersReduced =
      reducedMotion() ||
      (typeof window !== "undefined" &&
        (window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ?? false));
    if (prefersReduced) return;

    if (typeof e.to === "string" && e.to.split("?")[0] === e.from.pathname) {
      return;
    }

    e.preventDefault();
    const transition = docWithVt.startViewTransition(() => {
      e.retry(true);
    });
    transition.finished
      .then(() => {
        if (mainRef) {
          mainRef.scrollTop = 0;
          if (!document.activeElement || document.activeElement === document.body) {
            mainRef.focus({ preventScroll: true });
          }
        }
      })
      .catch(() => {});
  });

  // Fallback scroll & focus restoration when View Transitions are skipped or unsupported
  let prevPathname = location.pathname;
  createEffect(() => {
    const cur = location.pathname;
    if (cur !== prevPathname) {
      prevPathname = cur;
      setMobileMoreOpen(false);
      if (mainRef) {
        mainRef.scrollTop = 0;
        queueMicrotask(() => {
          if (mainRef && (!document.activeElement || document.activeElement === document.body)) {
            mainRef.focus({ preventScroll: true });
          }
        });
      }
    }
  });

  const handleLogout = async () => {
    setMobileMoreOpen(false);
    setSettingsOpen(false);
    await logout();
    showToast({ title: "Logged out", message: "You have been logged out", type: "info" });
    navigate("/login");
  };

  const handleInstallApp = async () => {
    const installed = await promptInstallPwa();
    if (installed) {
      showToast({
        title: t("appInstalled"),
        message: "MBL is ready on your home screen.",
        type: "success",
      });
    }
  };

  const isNavActive = (path: string): boolean => {
    const cur = location.pathname;
    if (path === "/") return cur === "/";
    if (path === "/libraries") {
      return (
        cur === "/libraries" ||
        cur.startsWith("/library/") ||
        cur.startsWith("/photos/") ||
        cur.startsWith("/item/")
      );
    }
    return cur === path || cur.startsWith(`${path}/`);
  };

  const browseLinks: NavItemDef[] = [
    { href: "/", icon: "home", label: () => t("home") },
    { href: "/libraries", icon: "libraries", label: () => t("libraries") },
    { href: "/search", icon: "search", label: () => t("search") },
    { href: "/livetv", icon: "livetv", label: () => t("liveTv") },
    { href: "/webtv", icon: "webtv", label: () => t("webtv") },
  ];

  const personalLinks: NavItemDef[] = [
    { href: "/collections", icon: "collections", label: () => t("collections") },
    { href: "/requests", icon: "requests", label: () => t("mediaRequests") },
    { href: "/account", icon: "account", label: () => t("accountSecurity") },
  ];

  const adminLinks: NavItemDef[] = [
    { href: "/admin/users", icon: "users", label: () => t("users") },
    { href: "/admin/ai-agents", icon: "ai-agents", label: () => t("aiAgents") },
    { href: "/admin/hardware", icon: "hardware", label: () => t("hardware") },
    { href: "/admin/remote", icon: "remote", label: () => t("remoteAccess") },
    { href: "/admin/workers", icon: "workers", label: () => t("distributedNodes") },
    { href: "/admin/settings", icon: "settings", label: () => t("serverSettings") },
    { href: "/admin/logs", icon: "logs", label: () => t("diagnosticLogs") },
  ];

  const userMenuItems = (): DropdownMenuItem[] => {
    const items: DropdownMenuItem[] = [
      {
        id: "switch-profile",
        label: t("switchProfile"),
        icon: <AppIcon name="profiles" size={18} />,
        onSelect: () => navigate("/profiles"),
      },
      {
        id: "account",
        label: t("accountSecurity"),
        icon: <AppIcon name="account" size={18} />,
        onSelect: () => navigate("/account"),
      },
      {
        id: "appearance",
        label: `${t("appearance")} & ${t("theme")}`,
        icon: <AppIcon name="appearance" size={18} />,
        onSelect: () => setSettingsOpen(true),
      },
    ];

    if (canInstallPwa()) {
      items.push({
        id: "install-pwa",
        label: t("installApp"),
        icon: <AppIcon name="install" size={18} />,
        onSelect: () => {
          void handleInstallApp();
        },
      });
    }

    items.push(
      {
        id: "api-docs",
        label: t("apiReference"),
        icon: <AppIcon name="api" size={18} />,
        separatorBefore: true,
        onSelect: () => window.location.assign("/api/docs"),
      },
      {
        id: "logout",
        label: t("logout"),
        icon: <AppIcon name="logout" size={18} />,
        danger: true,
        separatorBefore: true,
        onSelect: () => {
          void handleLogout();
        },
      },
    );
    return items;
  };

  const renderSidebarLink = (item: NavItemDef) => (
    <A
      href={item.href}
      class="mbl-nav-link"
      data-active={isNavActive(item.href) ? "true" : "false"}
      title={item.label()}
    >
      <span class="mbl-nav-icon" aria-hidden="true">
        <AppIcon name={item.icon} size={20} />
      </span>
      <span
        class="mbl-sidebar-label"
        style={{
          overflow: "hidden",
          "text-overflow": "ellipsis",
          "white-space": "nowrap",
        }}
      >
        {item.label()}
      </span>
    </A>
  );

  const isMoreRouteActive = () =>
    personalLinks.some((l) => isNavActive(l.href)) ||
    isNavActive("/webtv") ||
    isNavActive("/profiles") ||
    adminLinks.some((l) => isNavActive(l.href));

  return (
    <div
      style={{
        display: "flex",
        height: "100dvh",
        width: "100%",
        overflow: "hidden",
        background: "var(--bg-base)",
      }}
    >
      {/* Desktop & Tablet Sidebar / Navigation Rail (hidden on <= 768px) */}
      <aside
        class="desktop-sidebar mbl-shell-sidebar"
        data-rail-mode={railMode()}
        style={{
          width: "248px",
          background: "var(--bg-surface)",
          "border-right": "1px solid var(--border)",
          display: "flex",
          "flex-direction": "column",
          "flex-shrink": 0,
          transition: "width var(--duration-normal) var(--ease-standard)",
        }}
      >
        <div
          class="mbl-sidebar-brand"
          style={{
            padding: "16px",
            display: "flex",
            "align-items": "center",
            gap: "11px",
            "border-bottom": "1px solid var(--border)",
            height: "56px",
            "flex-shrink": 0,
          }}
        >
          <A
            href="/"
            style={{
              display: "flex",
              "align-items": "center",
              gap: "11px",
              overflow: "hidden",
              color: "inherit",
            }}
          >
            <div
              style={{
                width: "32px",
                height: "32px",
                background:
                  "linear-gradient(135deg, var(--accent) 0%, color-mix(in srgb, var(--accent) 75%, #000) 100%)",
                "border-radius": "var(--radius-md)",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "font-weight": "800",
                color: "#fff",
                "font-size": "15px",
                "flex-shrink": 0,
                "box-shadow": "var(--shadow-sm)",
              }}
            >
              M
            </div>
            <div class="mbl-sidebar-label" style={{ overflow: "hidden" }}>
              <div
                style={{
                  "font-weight": "800",
                  "font-size": "16px",
                  "letter-spacing": "0.04em",
                  "line-height": "1.15",
                }}
              >
                MBL
              </div>
              <div
                style={{
                  "font-size": "11px",
                  color: "var(--text-muted)",
                  "white-space": "nowrap",
                }}
              >
                Media Box Lounge
              </div>
            </div>
          </A>
        </div>

        <nav
          aria-label="Primary navigation"
          style={{
            padding: "12px 10px",
            display: "flex",
            "flex-direction": "column",
            gap: "3px",
            flex: 1,
            "overflow-y": "auto",
          }}
        >
          <div
            class="mbl-sidebar-section-title"
            style={{
              padding: "4px 12px 6px",
              "font-size": "10px",
              "font-weight": "700",
              color: "var(--text-muted)",
              "text-transform": "uppercase",
              "letter-spacing": "0.08em",
            }}
          >
            {t("browse")}
          </div>
          <For each={browseLinks}>{renderSidebarLink}</For>

          <div class="mbl-sidebar-rail-divider" aria-hidden="true" />
          <div
            class="mbl-sidebar-section-title"
            style={{
              "margin-top": "14px",
              padding: "4px 12px 6px",
              "font-size": "10px",
              "font-weight": "700",
              color: "var(--text-muted)",
              "text-transform": "uppercase",
              "letter-spacing": "0.08em",
            }}
          >
            {t("personal")}
          </div>
          <For each={personalLinks}>{renderSidebarLink}</For>

          <Show when={user()?.isAdmin}>
            <div class="mbl-sidebar-rail-divider" aria-hidden="true" />
            <div
              class="mbl-sidebar-section-title"
              style={{
                "margin-top": "14px",
                padding: "4px 12px 6px",
                "font-size": "10px",
                "font-weight": "700",
                color: "var(--text-muted)",
                "text-transform": "uppercase",
                "letter-spacing": "0.08em",
              }}
            >
              {t("admin")}
            </div>
            <For each={adminLinks}>{renderSidebarLink}</For>
          </Show>
        </nav>

        {/* User bar at bottom of sidebar */}
        <div
          class="mbl-sidebar-user"
          style={{
            padding: "12px 14px",
            "border-top": "1px solid var(--border)",
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            gap: "8px",
          }}
        >
          <A
            href="/profiles"
            title={t("switchProfile")}
            style={{
              display: "flex",
              "align-items": "center",
              gap: "10px",
              overflow: "hidden",
              "min-width": 0,
            }}
          >
            <div
              style={{
                width: "30px",
                height: "30px",
                "border-radius": "var(--radius-full)",
                background: "var(--accent)",
                color: "#fff",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "font-weight": "700",
                "font-size": "13px",
                "flex-shrink": 0,
              }}
            >
              {user()?.displayName?.charAt(0).toUpperCase() ||
                user()?.username?.charAt(0).toUpperCase() ||
                "U"}
            </div>
            <div class="mbl-sidebar-label" style={{ overflow: "hidden" }}>
              <div
                style={{
                  "font-size": "13px",
                  "font-weight": "600",
                  "white-space": "nowrap",
                  "text-overflow": "ellipsis",
                  overflow: "hidden",
                  color: "var(--text-primary)",
                }}
              >
                {user()?.displayName || user()?.username}
              </div>
              <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                {user()?.isAdmin ? t("admin") : t("onlineStatus")}
              </div>
            </div>
          </A>
          <div style={{ display: "flex", gap: "4px", "flex-shrink": 0 }}>
            <button
              type="button"
              onClick={() => setSettingsOpen(true)}
              title={`${t("appearance")} & ${t("settings")}`}
              aria-label={t("settings")}
              style={{
                background: "transparent",
                border: "1px solid transparent",
                color: "var(--text-secondary)",
                width: "30px",
                height: "30px",
                display: "inline-flex",
                "align-items": "center",
                "justify-content": "center",
                "border-radius": "var(--radius-sm)",
                cursor: "pointer",
              }}
            >
              <AppIcon name="settings" size={18} />
            </button>
            <button
              type="button"
              onClick={handleLogout}
              title={t("logout")}
              aria-label={t("logout")}
              style={{
                background: "transparent",
                border: "1px solid transparent",
                color: "var(--text-secondary)",
                width: "30px",
                height: "30px",
                display: "inline-flex",
                "align-items": "center",
                "justify-content": "center",
                "border-radius": "var(--radius-sm)",
                cursor: "pointer",
              }}
            >
              <AppIcon name="logout" size={18} />
            </button>
          </div>
        </div>

        {/* Version info footer */}
        <div
          class="mbl-sidebar-footer-meta"
          style={{
            padding: "4px 16px 10px",
            "font-size": "11px",
            color: "var(--text-muted)",
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            opacity: 0.75,
            "user-select": "none",
          }}
          title={`MBL v${__MBL_VERSION__} (commit ${__MBL_GIT_COMMIT__}, built ${__MBL_BUILD_DATE__})`}
        >
          <span>v{__MBL_VERSION__}</span>
          <span style={{ "font-family": "var(--font-mono)", "font-size": "10px" }}>
            {__MBL_GIT_COMMIT__}
          </span>
        </div>
      </aside>

      {/* Main Content Shell */}
      <div
        style={{
          flex: 1,
          display: "flex",
          "flex-direction": "column",
          height: "100dvh",
          "min-width": 0,
          overflow: "hidden",
        }}
      >
        {/* Top Bar */}
        <header
          class="mbl-shell-header"
          style={{
            height: "56px",
            background: "color-mix(in srgb, var(--bg-surface) 88%, transparent)",
            "backdrop-filter": "blur(14px)",
            "border-bottom": "1px solid var(--border)",
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            gap: "12px",
            padding: "0 20px",
            "flex-shrink": 0,
            "z-index": 30,
          }}
        >
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "10px",
              flex: "1 1 auto",
              "min-width": 0,
            }}
          >
            {/* Desktop/Tablet Sidebar Rail Toggle */}
            <button
              type="button"
              class="mbl-desktop-only"
              onClick={toggleSidebar}
              title={t("toggleSidebar")}
              aria-label={t("toggleSidebar")}
              style={{
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                color: "var(--text-secondary)",
                width: "36px",
                height: "36px",
                "border-radius": "var(--radius-md)",
                display: "inline-flex",
                "align-items": "center",
                "justify-content": "center",
                cursor: "pointer",
                "flex-shrink": 0,
              }}
            >
              <AppIcon name="menu" size={18} />
            </button>

            {/* Mobile Brand Badge */}
            <A
              href="/"
              class="mbl-mobile-brand"
              style={{
                display: "none",
                "align-items": "center",
                gap: "8px",
                "flex-shrink": 0,
                color: "inherit",
              }}
            >
              <div
                style={{
                  width: "32px",
                  height: "32px",
                  background: "var(--accent)",
                  "border-radius": "var(--radius-md)",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  "font-weight": "800",
                  color: "#fff",
                  "font-size": "15px",
                }}
              >
                M
              </div>
            </A>

            {/* Quick Search / Command Palette Trigger */}
            <button
              type="button"
              class="mbl-header-search-btn"
              onClick={openCommand}
              style={{
                display: "flex",
                "align-items": "center",
                gap: "var(--space-3)",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                padding: "7px 14px",
                "border-radius": "var(--radius-full)",
                color: "var(--text-muted)",
                "font-size": "var(--font-size-sm)",
                cursor: "pointer",
                width: "min(340px, 100%)",
                transition: "border-color var(--duration-fast) var(--ease-standard)",
              }}
            >
              <span aria-hidden="true" style={{ display: "inline-flex" }}>
                <AppIcon name="search" size={16} />
              </span>
              <span
                style={{
                  flex: 1,
                  "text-align": "left",
                  overflow: "hidden",
                  "text-overflow": "ellipsis",
                  "white-space": "nowrap",
                }}
              >
                {t("quickSearch")}
              </span>
              <kbd
                class="mbl-kbd-hint"
                style={{
                  padding: "1px 6px",
                  "border-radius": "var(--radius-sm)",
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border)",
                  "font-size": "10px",
                  "font-family": "var(--font-mono)",
                }}
              >
                Ctrl+K
              </kbd>
            </button>
          </div>

          {/* Right Header Actions */}
          <div style={{ display: "flex", "align-items": "center", gap: "10px", "flex-shrink": 0 }}>
            <Show when={canInstallPwa()}>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void handleInstallApp()}
                class="mbl-desktop-only"
              >
                <AppIcon name="install" size={16} /> {t("installApp")}
              </Button>
            </Show>

            <button
              type="button"
              onClick={() => setSettingsOpen(true)}
              title={`${t("appearance")} & ${t("theme")}`}
              aria-label={`${t("appearance")} & ${t("theme")}`}
              style={{
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                color: "var(--text-primary)",
                height: "36px",
                padding: "0 12px",
                "border-radius": "var(--radius-full)",
                "font-size": "var(--font-size-xs)",
                "font-weight": "var(--font-weight-medium)",
                display: "inline-flex",
                "align-items": "center",
                gap: "6px",
                cursor: "pointer",
              }}
            >
              <span aria-hidden="true" style={{ display: "inline-flex" }}>
                <AppIcon name="appearance" size={16} />
              </span>
              <span class="mbl-desktop-only">{t("appearance")}</span>
            </button>

            <DropdownMenu
              triggerLabel={user()?.displayName || user()?.username || "Account menu"}
              header={
                <div>
                  <div
                    style={{
                      "font-size": "var(--font-size-sm)",
                      "font-weight": "var(--font-weight-bold)",
                      color: "var(--text-primary)",
                    }}
                  >
                    {user()?.displayName || user()?.username}
                  </div>
                  <div
                    style={{
                      "font-size": "var(--font-size-xs)",
                      color: "var(--text-muted)",
                      display: "flex",
                      "align-items": "center",
                      gap: "6px",
                      "margin-top": "2px",
                    }}
                  >
                    <span
                      style={{
                        width: "7px",
                        height: "7px",
                        "border-radius": "50%",
                        background: "var(--success)",
                      }}
                    />
                    <span>{t("onlineStatus")}</span>
                  </div>
                </div>
              }
              items={userMenuItems()}
              trigger={
                <span
                  style={{
                    width: "34px",
                    height: "34px",
                    "border-radius": "var(--radius-full)",
                    background: "var(--accent)",
                    color: "#fff",
                    display: "inline-flex",
                    "align-items": "center",
                    "justify-content": "center",
                    "font-weight": "700",
                    "font-size": "13px",
                    border: "2px solid color-mix(in srgb, var(--accent) 45%, var(--bg-surface))",
                  }}
                >
                  {user()?.displayName?.charAt(0).toUpperCase() ||
                    user()?.username?.charAt(0).toUpperCase() ||
                    "U"}
                </span>
              }
            />
          </div>
        </header>

        {/* Scrollable View Area */}
        <main
          ref={mainRef}
          tabIndex={-1}
          class="mbl-shell-main"
          style={{
            flex: 1,
            overflow: "auto",
            padding: "24px",
            outline: "none",
            "-webkit-overflow-scrolling": "touch",
          }}
        >
          <ErrorBoundary>{props.children}</ErrorBoundary>
        </main>

        {/* Mobile Bottom Navigation Bar (shown on <= 768px) */}
        <nav
          aria-label="Mobile navigation"
          class="mobile-bottom-nav"
          style={{
            display: "none",
            position: "fixed",
            bottom: 0,
            left: 0,
            right: 0,
            height: "calc(62px + env(safe-area-inset-bottom))",
            "padding-bottom": "env(safe-area-inset-bottom)",
            "padding-left": "env(safe-area-inset-left)",
            "padding-right": "env(safe-area-inset-right)",
            background: "color-mix(in srgb, var(--bg-surface) 90%, transparent)",
            "backdrop-filter": "blur(16px)",
            "border-top": "1px solid var(--border)",
            "align-items": "center",
            "justify-content": "space-around",
            "z-index": 50,
          }}
        >
          <For
            each={
              [
                { href: "/", icon: "home", label: () => t("home") },
                { href: "/libraries", icon: "libraries", label: () => t("libraries") },
                { href: "/search", icon: "search", label: () => t("search") },
                { href: "/livetv", icon: "livetv", label: () => t("liveTv") },
              ] as NavItemDef[]
            }
          >
            {(tab) => {
              const active = () => isNavActive(tab.href);
              return (
                <A
                  href={tab.href}
                  aria-current={active() ? "page" : undefined}
                  style={{
                    flex: 1,
                    display: "flex",
                    "flex-direction": "column",
                    "align-items": "center",
                    "justify-content": "center",
                    gap: "2px",
                    height: "100%",
                    "font-size": "11px",
                    "font-weight": active() ? "700" : "500",
                    color: active() ? "var(--accent)" : "var(--text-secondary)",
                    transition: "color var(--duration-fast) var(--ease-standard)",
                  }}
                >
                  <span
                    aria-hidden="true"
                    style={{
                      display: "inline-flex",
                      padding: "3px 12px",
                      "border-radius": "var(--radius-full)",
                      background: active() ? "var(--accent-light)" : "transparent",
                      transition: "background-color var(--duration-fast) var(--ease-standard)",
                    }}
                  >
                    <AppIcon name={tab.icon} size={18} />
                  </span>
                  <span>{tab.label()}</span>
                </A>
              );
            }}
          </For>

          <button
            type="button"
            onClick={() => setMobileMoreOpen(true)}
            aria-expanded={mobileMoreOpen()}
            style={{
              flex: 1,
              display: "flex",
              "flex-direction": "column",
              "align-items": "center",
              "justify-content": "center",
              gap: "2px",
              height: "100%",
              background: "transparent",
              border: "none",
              "font-size": "11px",
              "font-weight": isMoreRouteActive() || mobileMoreOpen() ? "700" : "500",
              color:
                isMoreRouteActive() || mobileMoreOpen() ? "var(--accent)" : "var(--text-secondary)",
              cursor: "pointer",
            }}
          >
            <span
              aria-hidden="true"
              style={{
                display: "inline-flex",
                padding: "3px 12px",
                "border-radius": "var(--radius-full)",
                background:
                  isMoreRouteActive() || mobileMoreOpen() ? "var(--accent-light)" : "transparent",
              }}
            >
              <AppIcon name="menu" size={18} />
            </span>
            <span>{t("more")}</span>
          </button>
        </nav>
      </div>

      {/* Command Palette Modal */}
      <CommandPalette open={isCommandOpen()} onClose={closeCommand} />

      {/* Mobile "More" Navigation Drawer (Bottom Sheet) */}
      <Sheet
        open={mobileMoreOpen()}
        onOpenChange={setMobileMoreOpen}
        side="bottom"
        title={t("more")}
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-4)" }}>
          {/* Quick Grid for Secondary User Destinations */}
          <div
            style={{
              display: "grid",
              "grid-template-columns": "repeat(2, minmax(0, 1fr))",
              gap: "var(--space-2)",
            }}
          >
            <For
              each={
                [
                  { href: "/webtv", icon: "webtv", label: () => t("webtv") },
                  { href: "/collections", icon: "collections", label: () => t("collections") },
                  { href: "/requests", icon: "requests", label: () => t("mediaRequests") },
                  { href: "/profiles", icon: "profiles", label: () => t("switchProfile") },
                  { href: "/account", icon: "account", label: () => t("accountSecurity") },
                ] as NavItemDef[]
              }
            >
              {(item) => (
                <A
                  href={item.href}
                  onClick={() => setMobileMoreOpen(false)}
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "10px",
                    padding: "12px 14px",
                    background: isNavActive(item.href)
                      ? "var(--accent-light)"
                      : "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-md)",
                    color: "var(--text-primary)",
                    "font-size": "var(--font-size-sm)",
                    "font-weight": "var(--font-weight-medium)",
                  }}
                >
                  <span style={{ display: "inline-flex" }} aria-hidden="true">
                    <AppIcon name={item.icon} size={18} />
                  </span>
                  <span
                    style={{
                      overflow: "hidden",
                      "text-overflow": "ellipsis",
                      "white-space": "nowrap",
                    }}
                  >
                    {item.label()}
                  </span>
                </A>
              )}
            </For>

            <button
              type="button"
              onClick={() => {
                setMobileMoreOpen(false);
                setSettingsOpen(true);
              }}
              style={{
                display: "flex",
                "align-items": "center",
                gap: "10px",
                padding: "12px 14px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
                color: "var(--text-primary)",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-medium)",
                cursor: "pointer",
                "text-align": "left",
              }}
            >
              <span style={{ display: "inline-flex" }} aria-hidden="true">
                <AppIcon name="appearance" size={18} />
              </span>
              <span>{t("appearance")}</span>
            </button>
          </div>

          <Show when={user()?.isAdmin}>
            <div>
              <div
                style={{
                  "font-size": "11px",
                  "font-weight": "700",
                  color: "var(--text-muted)",
                  "text-transform": "uppercase",
                  "letter-spacing": "0.08em",
                  "margin-bottom": "var(--space-2)",
                }}
              >
                {t("admin")}
              </div>
              <div
                style={{
                  display: "grid",
                  "grid-template-columns": "repeat(2, minmax(0, 1fr))",
                  gap: "var(--space-2)",
                }}
              >
                <For each={adminLinks}>
                  {(item) => (
                    <A
                      href={item.href}
                      onClick={() => setMobileMoreOpen(false)}
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "10px",
                        padding: "11px 12px",
                        background: isNavActive(item.href)
                          ? "var(--accent-light)"
                          : "var(--bg-surface)",
                        border: "1px solid var(--border)",
                        "border-radius": "var(--radius-md)",
                        color: "var(--text-primary)",
                        "font-size": "var(--font-size-sm)",
                        "font-weight": "var(--font-weight-medium)",
                      }}
                    >
                      <span style={{ display: "inline-flex" }} aria-hidden="true">
                        <AppIcon name={item.icon} size={18} />
                      </span>
                      <span
                        style={{
                          overflow: "hidden",
                          "text-overflow": "ellipsis",
                          "white-space": "nowrap",
                        }}
                      >
                        {item.label()}
                      </span>
                    </A>
                  )}
                </For>
              </div>
            </div>
          </Show>

          <Show when={canInstallPwa() || isIosSafariInstallCandidate()}>
            <div
              style={{
                padding: "var(--space-3) var(--space-4)",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
                display: "flex",
                "flex-direction": "column",
                gap: "var(--space-2)",
              }}
            >
              <div style={{ "font-weight": "var(--font-weight-semibold)", "font-size": "14px" }}>
                📲 {t("installApp")}
              </div>
              <div style={{ "font-size": "var(--font-size-xs)", color: "var(--text-secondary)" }}>
                {canInstallPwa() ? t("installAppDesc") : t("installAppIosHint")}
              </div>
              <Show when={canInstallPwa()}>
                <Button variant="primary" size="sm" onClick={() => void handleInstallApp()}>
                  {t("installApp")}
                </Button>
              </Show>
            </div>
          </Show>

          <div style={{ display: "flex", gap: "var(--space-2)" }}>
            <Button
              variant="secondary"
              size="md"
              style={{ flex: 1 }}
              onClick={() => window.location.assign("/api/docs")}
            >
              📖 {t("apiReference")}
            </Button>
            <Button
              variant="danger"
              size="md"
              style={{ flex: 1 }}
              onClick={() => void handleLogout()}
            >
              🚪 {t("logout")}
            </Button>
          </div>
        </div>
      </Sheet>

      {/* Appearance & Preferences Sheet (Right drawer on desktop/tablet, Bottom sheet on mobile) */}
      <Sheet
        open={settingsOpen()}
        onOpenChange={setSettingsOpen}
        side={isMobile() ? "bottom" : "right"}
        title={`${t("appearance")} & ${t("settings")}`}
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-5)" }}>
          {/* Theme Selector */}
          <div class="mbl-card-section" style={{ padding: "var(--space-4)" }}>
            <span
              style={{
                display: "block",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-semibold)",
                "margin-bottom": "var(--space-3)",
              }}
            >
              {t("theme")}
            </span>
            <SegmentedControl<ThemeMode>
              value={theme()}
              onChange={setTheme}
              ariaLabel={t("theme")}
              options={[
                { value: "dark", label: t("dark") },
                { value: "oled", label: t("oled") },
                { value: "light", label: t("light") },
                { value: "high-contrast", label: "HC" },
              ]}
            />
          </div>

          {/* Accent Color Selector */}
          <div class="mbl-card-section" style={{ padding: "var(--space-4)" }}>
            <span
              style={{
                display: "block",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-semibold)",
                "margin-bottom": "var(--space-3)",
              }}
            >
              {t("accent")}
            </span>
            <div
              style={{
                display: "grid",
                "grid-template-columns": "repeat(3, minmax(0, 1fr))",
                gap: "var(--space-2)",
              }}
            >
              {(["amber", "indigo", "emerald", "rose", "cyan", "violet"] as AccentColor[]).map(
                (ac) => {
                  const isSel = () => accent() === ac;
                  return (
                    <button
                      type="button"
                      aria-pressed={isSel()}
                      onClick={() => setAccent(ac)}
                      style={{
                        display: "flex",
                        "align-items": "center",
                        "justify-content": "center",
                        gap: "6px",
                        padding: "8px 10px",
                        "border-radius": "var(--radius-md)",
                        border: isSel()
                          ? "2px solid var(--text-primary)"
                          : "1px solid var(--border)",
                        background: `var(--color-accent-${ac}-base)`,
                        color: "#fff",
                        "font-size": "12px",
                        "font-weight": "700",
                        cursor: "pointer",
                        transition: "transform var(--duration-fast) var(--ease-standard)",
                      }}
                    >
                      <span>{t(ac)}</span>
                      <Show when={isSel()}>
                        <span aria-hidden="true">✓</span>
                      </Show>
                    </button>
                  );
                },
              )}
            </div>
          </div>

          {/* Language Selector (23 Languages) */}
          <div class="mbl-card-section" style={{ padding: "var(--space-4)" }}>
            <Select
              id="mbl-appearance-language"
              label={t("language")}
              value={locale()}
              options={SUPPORTED_LOCALES.map((loc) => ({
                value: loc.code,
                label: `${loc.nativeName} (${loc.code.toUpperCase()})`,
              }))}
              onChange={(e) => setLocale(e.currentTarget.value as AppLocale)}
            />
          </div>

          {/* Reduced Motion Switch */}
          <div class="mbl-card-section" style={{ padding: "var(--space-4)" }}>
            <Switch
              checked={reducedMotion()}
              onChange={setReducedMotion}
              label={t("reducedMotion")}
            />
          </div>

          {/* Installable Web App (PWA) Section */}
          <Show when={canInstallPwa() || isIosSafariInstallCandidate() || isStandaloneApp()}>
            <div class="mbl-card-section" style={{ padding: "var(--space-4)" }}>
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "space-between",
                  gap: "var(--space-3)",
                }}
              >
                <div>
                  <div
                    style={{
                      "font-size": "var(--font-size-sm)",
                      "font-weight": "var(--font-weight-semibold)",
                    }}
                  >
                    📲 {isStandaloneApp() ? t("appInstalled") : t("installApp")}
                  </div>
                  <Show when={!isStandaloneApp()}>
                    <div
                      style={{
                        "font-size": "var(--font-size-xs)",
                        color: "var(--text-secondary)",
                        "margin-top": "4px",
                      }}
                    >
                      {canInstallPwa() ? t("installAppDesc") : t("installAppIosHint")}
                    </div>
                  </Show>
                </div>
                <Show when={canInstallPwa()}>
                  <Button variant="primary" size="sm" onClick={() => void handleInstallApp()}>
                    {t("installApp")}
                  </Button>
                </Show>
              </div>
            </div>
          </Show>

          {/* Footer Quick Links */}
          <div style={{ display: "flex", gap: "var(--space-2)" }}>
            <A href="/profiles" onClick={() => setSettingsOpen(false)} style={{ flex: 1 }}>
              <Button variant="outline" size="md" style={{ width: "100%" }}>
                👤 {t("switchProfile")}
              </Button>
            </A>
            <Button
              variant="secondary"
              size="md"
              onClick={() => window.location.assign("/api/docs")}
            >
              📖 {t("apiReference")}
            </Button>
          </div>
        </div>
      </Sheet>
    </div>
  );
}

export default Layout;

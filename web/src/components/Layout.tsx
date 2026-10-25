import { A, useLocation, useNavigate } from "@solidjs/router";
import { createSignal, type JSX, onCleanup, onMount, Show } from "solid-js";
import { useAuth } from "../context/AuthContext";
import {
  type AccentColor,
  type AppLocale,
  type ThemeMode,
  useTheme,
} from "../context/ThemeContext";
import { useToast } from "../context/ToastContext";
import { useI18n } from "../i18n";
import { CommandPalette, useCommandPalette } from "./CommandPalette";
import { ErrorBoundary } from "./ErrorBoundary";
import { Button, SegmentedControl, Sheet } from "./ui";

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
  const [isTvMode, setIsTvMode] = createSignal(false);

  onMount(() => {
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
  });

  const handleLogout = async () => {
    await logout();
    showToast({ title: "Logged out", message: "You have been logged out", type: "info" });
    navigate("/login");
  };

  const navLinkStyle = (path: string) => {
    const isActive = location.pathname === path;
    return {
      display: "flex",
      "align-items": "center",
      gap: "10px",
      padding: "10px 14px",
      "border-radius": "var(--radius-md)",
      color: isActive ? "var(--text-primary)" : "var(--text-secondary)",
      background: isActive ? "var(--accent-light)" : "transparent",
      "font-weight": isActive ? "600" : "400",
      transition: "background var(--duration-fast), color var(--duration-fast)",
    };
  };

  return (
    <div
      style={{
        display: "flex",
        height: "100vh",
        width: "100vw",
        overflow: "hidden",
        background: "var(--bg-base)",
      }}
    >
      {/* Desktop Sidebar (hidden on mobile) */}
      <aside
        class="desktop-sidebar"
        style={{
          width: "240px",
          background: "var(--bg-surface)",
          "border-right": "1px solid var(--border)",
          display: "flex",
          "flex-direction": "column",
          "flex-shrink": 0,
        }}
      >
        <div
          style={{
            padding: "20px 16px",
            display: "flex",
            "align-items": "center",
            gap: "10px",
            "border-bottom": "1px solid var(--border)",
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
              "font-size": "16px",
            }}
          >
            M
          </div>
          <div>
            <div style={{ "font-weight": "700", "font-size": "17px", "letter-spacing": "0.5px" }}>
              MBL
            </div>
            <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>Media Server</div>
          </div>
        </div>

        <nav
          style={{
            padding: "16px 12px",
            display: "flex",
            "flex-direction": "column",
            gap: "4px",
            flex: 1,
            "overflow-y": "auto",
          }}
        >
          <A href="/" style={navLinkStyle("/")}>
            <span>🏠</span>
            <span>{t("home")}</span>
          </A>
          <A href="/libraries" style={navLinkStyle("/libraries")}>
            <span>📚</span>
            <span>{t("libraries")}</span>
          </A>
          <A href="/search" style={navLinkStyle("/search")}>
            <span>🔍</span>
            <span>{t("search")}</span>
          </A>

          <Show when={user()?.isAdmin}>
            <div
              style={{
                "margin-top": "20px",
                "margin-bottom": "6px",
                padding: "0 14px",
                "font-size": "11px",
                "font-weight": "700",
                color: "var(--text-muted)",
                "text-transform": "uppercase",
                "letter-spacing": "0.8px",
              }}
            >
              {t("admin")}
            </div>
            <A href="/admin/users" style={navLinkStyle("/admin/users")}>
              <span>👥</span>
              <span>{t("users")}</span>
            </A>
            <A href="/admin/ai-agents" style={navLinkStyle("/admin/ai-agents")}>
              <span>🤖</span>
              <span>{t("aiAgents")}</span>
            </A>
          </Show>
        </nav>

        {/* User bar at bottom of sidebar */}
        <div
          style={{
            padding: "16px",
            "border-top": "1px solid var(--border)",
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
          }}
        >
          <A
            href="/profiles"
            style={{
              display: "flex",
              "align-items": "center",
              gap: "var(--space-2)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: "28px",
                height: "28px",
                "border-radius": "var(--radius-sm)",
                background: "var(--accent)",
                color: "#fff",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "font-weight": "bold",
                "font-size": "13px",
              }}
            >
              {user()?.displayName?.charAt(0).toUpperCase() || "U"}
            </div>
            <div style={{ overflow: "hidden" }}>
              <div
                style={{
                  "font-size": "13px",
                  "font-weight": "600",
                  "white-space": "nowrap",
                  "text-overflow": "ellipsis",
                  overflow: "hidden",
                }}
              >
                {user()?.displayName || user()?.username}
              </div>
            </div>
          </A>
          <div style={{ display: "flex", gap: "4px" }}>
            <button
              type="button"
              onClick={() => setSettingsOpen(true)}
              title={t("settings")}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                padding: "6px",
                "border-radius": "var(--radius-sm)",
                cursor: "pointer",
              }}
            >
              ⚙
            </button>
            <button
              type="button"
              onClick={handleLogout}
              title={t("logout")}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                padding: "6px",
                "border-radius": "var(--radius-sm)",
                cursor: "pointer",
              }}
            >
              🚪
            </button>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <div
        style={{
          flex: 1,
          display: "flex",
          "flex-direction": "column",
          height: "100vh",
          overflow: "hidden",
        }}
      >
        {/* Top bar */}
        <header
          style={{
            height: "56px",
            background: "var(--bg-surface)",
            "border-bottom": "1px solid var(--border)",
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "0 24px",
            "flex-shrink": 0,
          }}
        >
          {/* Quick Search / Command Palette Trigger */}
          <button
            type="button"
            onClick={openCommand}
            style={{
              display: "flex",
              "align-items": "center",
              gap: "var(--space-3)",
              background: "var(--bg-card)",
              border: "1px solid var(--border)",
              padding: "6px 14px",
              "border-radius": "var(--radius-md)",
              color: "var(--text-muted)",
              "font-size": "var(--font-size-sm)",
              cursor: "pointer",
              width: "280px",
            }}
          >
            <span>🔍</span>
            <span style={{ flex: 1, "text-align": "left" }}>Search or jump to...</span>
            <kbd
              style={{
                padding: "1px 5px",
                "border-radius": "var(--radius-sm)",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "font-size": "10px",
              }}
            >
              Ctrl+K
            </kbd>
          </button>

          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <button
              type="button"
              onClick={() => setSettingsOpen(true)}
              style={{
                background: "transparent",
                border: "1px solid var(--border)",
                color: "var(--text-primary)",
                padding: "6px 12px",
                "border-radius": "var(--radius-sm)",
                "font-size": "var(--font-size-xs)",
                display: "flex",
                "align-items": "center",
                gap: "var(--space-2)",
                cursor: "pointer",
              }}
            >
              <span>🎨</span> {t("theme")} & {t("settings")}
            </button>
            <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
              <span
                style={{
                  width: "8px",
                  height: "8px",
                  "border-radius": "50%",
                  background: "var(--success)",
                }}
              />
              <span style={{ "font-size": "12px", color: "var(--text-muted)" }}>Online</span>
            </div>
          </div>
        </header>

        {/* Scrollable View Area */}
        <main
          style={{
            flex: 1,
            overflow: "auto",
            padding: "24px",
            "padding-bottom": "80px", // space for mobile tab bar
          }}
        >
          <ErrorBoundary>{props.children}</ErrorBoundary>
        </main>

        {/* Mobile Bottom Navigation Bar */}
        <nav
          class="mobile-bottom-nav"
          style={{
            display: "none",
            position: "fixed",
            bottom: 0,
            left: 0,
            right: 0,
            height: "60px",
            background: "var(--bg-surface)",
            "border-top": "1px solid var(--border)",
            "align-items": "center",
            "justify-content": "space-around",
            "z-index": 50,
          }}
        >
          <A
            href="/"
            style={{
              display: "flex",
              "flex-direction": "column",
              "align-items": "center",
              "font-size": "11px",
              color: "var(--text-secondary)",
            }}
          >
            <span style={{ "font-size": "18px" }}>🏠</span>
            <span>{t("home")}</span>
          </A>
          <A
            href="/libraries"
            style={{
              display: "flex",
              "flex-direction": "column",
              "align-items": "center",
              "font-size": "11px",
              color: "var(--text-secondary)",
            }}
          >
            <span style={{ "font-size": "18px" }}>📚</span>
            <span>{t("libraries")}</span>
          </A>
          <A
            href="/search"
            style={{
              display: "flex",
              "flex-direction": "column",
              "align-items": "center",
              "font-size": "11px",
              color: "var(--text-secondary)",
            }}
          >
            <span style={{ "font-size": "18px" }}>🔍</span>
            <span>{t("search")}</span>
          </A>
          <A
            href="/profiles"
            style={{
              display: "flex",
              "flex-direction": "column",
              "align-items": "center",
              "font-size": "11px",
              color: "var(--text-secondary)",
            }}
          >
            <span style={{ "font-size": "18px" }}>👤</span>
            <span>{t("users")}</span>
          </A>
        </nav>
      </div>

      {/* Command Palette Modal */}
      <CommandPalette open={isCommandOpen()} onClose={closeCommand} />

      {/* Settings & Appearance Sheet */}
      <Sheet open={settingsOpen()} onOpenChange={setSettingsOpen} title={t("settings")}>
        <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-6)" }}>
          {/* Theme Selector */}
          <div>
            <span
              style={{
                display: "block",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-semibold)",
                "margin-bottom": "var(--space-2)",
              }}
            >
              {t("theme")}
            </span>
            <SegmentedControl<ThemeMode>
              value={theme()}
              onChange={setTheme}
              options={[
                { value: "dark", label: t("dark") },
                { value: "oled", label: t("oled") },
                { value: "light", label: t("light") },
                { value: "high-contrast", label: "HC" },
              ]}
            />
          </div>

          {/* Accent Color Selector */}
          <div>
            <span
              style={{
                display: "block",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-semibold)",
                "margin-bottom": "var(--space-2)",
              }}
            >
              {t("accent")}
            </span>
            <div style={{ display: "flex", gap: "var(--space-2)", "flex-wrap": "wrap" }}>
              {(["amber", "indigo", "emerald", "rose", "cyan", "violet"] as AccentColor[]).map(
                (ac) => {
                  const isSel = () => accent() === ac;
                  return (
                    <button
                      type="button"
                      onClick={() => setAccent(ac)}
                      style={{
                        padding: "6px 12px",
                        "border-radius": "var(--radius-sm)",
                        border: isSel()
                          ? "2px solid var(--text-primary)"
                          : "1px solid var(--border)",
                        background: `var(--color-accent-${ac}-base)`,
                        color: "#fff",
                        "font-size": "11px",
                        "font-weight": "bold",
                        cursor: "pointer",
                      }}
                    >
                      {t(ac as any)}
                    </button>
                  );
                },
              )}
            </div>
          </div>

          {/* Language Selector */}
          <div>
            <span
              style={{
                display: "block",
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-semibold)",
                "margin-bottom": "var(--space-2)",
              }}
            >
              {t("language")}
            </span>
            <SegmentedControl<AppLocale>
              value={locale()}
              onChange={setLocale}
              options={[
                { value: "en", label: "English" },
                { value: "sv", label: "Svenska" },
                { value: "de", label: "Deutsch" },
              ]}
            />
          </div>

          {/* Reduced Motion Toggle */}
          <div
            style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}
          >
            <span
              style={{
                "font-size": "var(--font-size-sm)",
                "font-weight": "var(--font-weight-medium)",
              }}
            >
              {t("reducedMotion")}
            </span>
            <Button
              variant={reducedMotion() ? "primary" : "secondary"}
              size="sm"
              onClick={() => setReducedMotion(!reducedMotion())}
            >
              {reducedMotion() ? "Enabled" : "Disabled"}
            </Button>
          </div>

          <div style={{ "margin-top": "var(--space-6)" }}>
            <A href="/profiles" onClick={() => setSettingsOpen(false)}>
              <Button variant="outline" size="md" style={{ width: "100%" }}>
                {t("switchProfile")}
              </Button>
            </A>
          </div>
        </div>
      </Sheet>
    </div>
  );
}

export default Layout;

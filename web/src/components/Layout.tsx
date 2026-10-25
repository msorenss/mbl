import { A, useLocation, useNavigate } from "@solidjs/router";
import { type JSX, Show } from "solid-js";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";
import { ErrorBoundary } from "./ErrorBoundary";

export function Layout(props: { children?: JSX.Element }) {
  const { user, logout } = useAuth();
  const { showToast } = useToast();
  const navigate = useNavigate();
  const location = useLocation();

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
      transition: "background 0.15s, color 0.15s",
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
      {/* Sidebar */}
      <aside
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
            <span>Home</span>
          </A>
          <A href="/libraries" style={navLinkStyle("/libraries")}>
            <span>📚</span>
            <span>Libraries</span>
          </A>
          <A href="/search" style={navLinkStyle("/search")}>
            <span>🔍</span>
            <span>Search</span>
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
              Administration
            </div>
            <A href="/admin/users" style={navLinkStyle("/admin/users")}>
              <span>👥</span>
              <span>Users</span>
            </A>
            <A href="/admin/ai-agents" style={navLinkStyle("/admin/ai-agents")}>
              <span>🤖</span>
              <span>AI Agents</span>
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
            <Show when={user()?.isAdmin}>
              <span
                style={{
                  display: "inline-block",
                  "font-size": "10px",
                  padding: "1px 6px",
                  "border-radius": "10px",
                  background: "var(--accent-light)",
                  color: "var(--accent)",
                  "font-weight": "600",
                }}
              >
                Admin
              </span>
            </Show>
          </div>
          <button
            type="button"
            onClick={handleLogout}
            title="Log out"
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
          <div style={{ "font-size": "14px", color: "var(--text-secondary)" }}>
            {location.pathname === "/"
              ? "Dashboard"
              : location.pathname.substring(1).replace(/\//g, " / ")}
          </div>
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                width: "8px",
                height: "8px",
                "border-radius": "50%",
                background: "var(--success)",
              }}
            />
            <span style={{ "font-size": "12px", color: "var(--text-muted)" }}>Server Online</span>
          </div>
        </header>

        {/* Scrollable View Area */}
        <main
          style={{
            flex: 1,
            overflow: "auto",
            padding: "24px",
          }}
        >
          <ErrorBoundary>{props.children}</ErrorBoundary>
        </main>
      </div>
    </div>
  );
}

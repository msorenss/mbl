import type { JSX } from "solid-js";

export type AppIconName =
  | "home"
  | "libraries"
  | "search"
  | "livetv"
  | "webtv"
  | "collections"
  | "requests"
  | "account"
  | "users"
  | "ai-agents"
  | "hardware"
  | "remote"
  | "workers"
  | "settings"
  | "logs"
  | "profiles"
  | "appearance"
  | "api"
  | "logout"
  | "install"
  | "menu"
  | "theme"
  | "accent"
  | "language";

export interface AppIconProps {
  name: AppIconName;
  size?: number;
  class?: string;
  style?: JSX.CSSProperties;
}

export function AppIcon(props: AppIconProps) {
  const size = () => props.size ?? 20;

  const renderPaths = (): JSX.Element => {
    switch (props.name) {
      case "logs":
        return (
          <>
            <rect x="4" y="3" width="16" height="18" rx="2" />
            <path d="M8 8h8M8 12h8M8 16h5" />
          </>
        );
      case "home":
        return (
          <>
            <path d="M3 10.5L12 3l9 7.5" />
            <path d="M5 9.5V20a1 1 0 0 0 1 1h4.5v-6h3V21H18a1 1 0 0 0 1-1V9.5" />
          </>
        );
      case "libraries":
        return (
          <>
            <rect x="3" y="3" width="7" height="7" rx="1.5" />
            <rect x="14" y="3" width="7" height="7" rx="1.5" />
            <rect x="3" y="14" width="7" height="7" rx="1.5" />
            <rect x="14" y="14" width="7" height="7" rx="1.5" />
          </>
        );
      case "search":
        return (
          <>
            <circle cx="11" cy="11" r="7" />
            <path d="M20 20l-4.2-4.2" />
          </>
        );
      case "livetv":
        return (
          <>
            <rect x="2.5" y="7" width="19" height="13" rx="2" />
            <path d="M8 2.5L12 7l4-4.5" />
            <path d="M10 11.5l5 2.5-5 2.5v-5z" fill="currentColor" stroke="none" />
          </>
        );
      case "webtv":
        return (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M3.5 12h17" />
            <path d="M12 3a14 14 0 0 1 0 18" />
            <path d="M12 3a14 14 0 0 0 0 18" />
          </>
        );
      case "collections":
        return (
          <>
            <rect x="3" y="6" width="15" height="14" rx="2" />
            <path d="M7 3h13a1 1 0 0 1 1 1v13" />
            <path d="M8.5 10.5l5 2.5-5 2.5v-5z" fill="currentColor" stroke="none" />
          </>
        );
      case "requests":
        return (
          <path d="M12 2.5l2.6 5.6 6.1.8-4.5 4.2 1.1 6.1L12 16.2l-5.3 3 1.1-6.1-4.5-4.2 6.1-.8L12 2.5z" />
        );
      case "account":
        return (
          <>
            <path d="M12 2.5l7.5 3.2v5.3c0 5-3.2 8.8-7.5 10.5-4.3-1.7-7.5-5.5-7.5-10.5V5.7L12 2.5z" />
            <path d="M9.5 12l1.8 1.8 3.5-3.6" />
          </>
        );
      case "users":
        return (
          <>
            <circle cx="9" cy="8" r="3.5" />
            <path d="M2.5 19.5c0-3.3 2.9-6 6.5-6s6.5 2.7 6.5 6" />
            <circle cx="17" cy="9" r="2.5" />
            <path d="M17.5 14c2.3.4 4 2.3 4 4.8" />
          </>
        );
      case "ai-agents":
        return (
          <>
            <rect x="4" y="6" width="16" height="12" rx="3" />
            <path d="M12 2.5V6" />
            <circle cx="9" cy="12" r="1.4" fill="currentColor" stroke="none" />
            <circle cx="15" cy="12" r="1.4" fill="currentColor" stroke="none" />
            <path d="M9.5 15.5h5" />
            <path d="M2 12h2M20 12h2" />
          </>
        );
      case "hardware":
        return (
          <>
            <rect x="6" y="6" width="12" height="12" rx="2" />
            <rect x="9.5" y="9.5" width="5" height="5" rx="0.8" />
            <path d="M9 2.5V6M15 2.5V6M9 18v3.5M15 18v3.5M2.5 9H6M2.5 15H6M18 9h3.5M18 15h3.5" />
          </>
        );
      case "remote":
        return (
          <>
            <circle cx="12" cy="12" r="3" />
            <path d="M6.3 17.7a8 8 0 0 1 0-11.4M17.7 6.3a8 8 0 0 1 0 11.4" />
            <path d="M3.5 20.5a12 12 0 0 1 0-17M20.5 3.5a12 12 0 0 1 0 17" />
          </>
        );
      case "workers":
        return (
          <>
            <rect x="3" y="3.5" width="18" height="6.5" rx="1.5" />
            <rect x="3" y="14" width="18" height="6.5" rx="1.5" />
            <circle cx="7" cy="6.75" r="1" fill="currentColor" stroke="none" />
            <circle cx="7" cy="17.25" r="1" fill="currentColor" stroke="none" />
            <path d="M11.5 6.75h6M11.5 17.25h6" />
          </>
        );
      case "settings":
        return (
          <>
            <circle cx="12" cy="12" r="3.2" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.08a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.08a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.08a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </>
        );
      case "profiles":
        return (
          <>
            <circle cx="12" cy="8.5" r="3.8" />
            <path d="M4.5 20c0-3.8 3.4-6.5 7.5-6.5s7.5 2.7 7.5 6.5" />
          </>
        );
      case "appearance":
      case "theme":
      case "accent":
        return (
          <>
            <path d="M12 3a9 9 0 1 0 0 18c1.3 0 2.2-.9 2.2-2.1 0-.5-.2-1-.5-1.4-.3-.3-.4-.8-.4-1.2 0-1.1.9-2 2-2H17a4 4 0 0 0 4-4c0-4.1-4-7.3-9-7.3z" />
            <circle cx="8" cy="10" r="1.1" fill="currentColor" stroke="none" />
            <circle cx="11.5" cy="7.5" r="1.1" fill="currentColor" stroke="none" />
            <circle cx="15.5" cy="9.5" r="1.1" fill="currentColor" stroke="none" />
          </>
        );
      case "api":
        return (
          <>
            <path d="M8 8l-4 4 4 4" />
            <path d="M16 8l4 4-4 4" />
            <path d="M13.5 5.5l-3 13" />
          </>
        );
      case "logout":
        return (
          <>
            <path d="M10 4H5.5A1.5 1.5 0 0 0 4 5.5v13A1.5 1.5 0 0 0 5.5 20H10" />
            <path d="M15.5 16.5L20 12l-4.5-4.5" />
            <path d="M9.5 12H20" />
          </>
        );
      case "install":
        return (
          <>
            <rect x="5" y="2.5" width="14" height="19" rx="2" />
            <path d="M12 7.5v7" />
            <path d="M9.2 11.8L12 14.6l2.8-2.8" />
            <path d="M10 18.5h4" />
          </>
        );
      case "language":
        return (
          <>
            <path d="M4 6h9M8.5 4v2M5.5 6c.4 3.5 2.5 6.8 6 8.5M10.5 6c-.5 2.8-2.2 5.7-5 7.5" />
            <path d="M13.5 19.5l3.5-9 3.5 9M14.7 16.5h4.6" />
          </>
        );
      default:
        return (
          <>
            <path d="M4 6.5h16" />
            <path d="M4 12h16" />
            <path d="M4 17.5h16" />
          </>
        );
    }
  };

  return (
    <svg
      width={size()}
      height={size()}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.85"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      class={props.class}
      style={{
        "flex-shrink": 0,
        display: "inline-block",
        "vertical-align": "middle",
        ...props.style,
      }}
    >
      {renderPaths()}
    </svg>
  );
}

export default AppIcon;

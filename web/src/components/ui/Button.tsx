import { Button as KobalteButton } from "@kobalte/core/button";
import { type JSX, splitProps } from "solid-js";

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "outline" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  loading?: boolean;
  children?: JSX.Element;
  class?: string;
  onClick?: JSX.EventHandlerUnion<HTMLButtonElement, MouseEvent>;
}

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, ["variant", "size", "loading", "children", "class"]);
  const variant = () => local.variant || "primary";
  const size = () => local.size || "md";

  const getVariantStyles = () => {
    switch (variant()) {
      case "secondary":
        return {
          "background-color": "var(--bg-surface)",
          color: "var(--text-primary)",
          border: "1px solid var(--border)",
        };
      case "outline":
        return {
          "background-color": "transparent",
          color: "var(--text-primary)",
          border: "1px solid var(--border)",
        };
      case "ghost":
        return {
          "background-color": "transparent",
          color: "var(--text-secondary)",
          border: "none",
        };
      case "danger":
        return {
          "background-color": "var(--danger)",
          color: "#ffffff",
          border: "none",
        };
      case "primary":
      default:
        return {
          "background-color": "var(--accent)",
          color: "#ffffff",
          border: "none",
        };
    }
  };

  const getSizeStyles = () => {
    switch (size()) {
      case "sm":
        return {
          padding: "6px 12px",
          "font-size": "var(--font-size-sm)",
          "border-radius": "var(--radius-sm)",
        };
      case "lg":
        return {
          padding: "12px 24px",
          "font-size": "var(--font-size-lg)",
          "border-radius": "var(--radius-md)",
        };
      case "md":
      default:
        return {
          padding: "8px 16px",
          "font-size": "var(--font-size-md)",
          "border-radius": "var(--radius-sm)",
        };
    }
  };

  return (
    <KobalteButton
      disabled={local.loading || rest.disabled}
      style={{
        display: "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        gap: "var(--space-2)",
        "font-weight": "var(--font-weight-medium)",
        cursor: local.loading || rest.disabled ? "not-allowed" : "pointer",
        opacity: rest.disabled ? "0.5" : "1",
        transition: "all var(--duration-fast) var(--ease-standard)",
        outline: "none",
        ...getVariantStyles(),
        ...getSizeStyles(),
      }}
      class={`mbl-button ${local.class || ""}`}
      {...rest}
    >
      {local.loading && (
        <span
          style={{
            width: "14px",
            height: "14px",
            border: "2px solid currentColor",
            "border-right-color": "transparent",
            "border-radius": "var(--radius-full)",
            animation: "spin 0.75s linear infinite",
          }}
        />
      )}
      {local.children}
    </KobalteButton>
  );
}

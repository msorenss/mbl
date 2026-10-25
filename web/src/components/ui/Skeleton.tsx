export interface SkeletonProps {
  width?: string;
  height?: string;
  borderRadius?: string;
  class?: string;
}

export function Skeleton(props: SkeletonProps) {
  return (
    <div
      class={`mbl-skeleton ${props.class || ""}`}
      style={{
        width: props.width || "100%",
        height: props.height || "20px",
        "border-radius": props.borderRadius || "var(--radius-sm)",
        "background-color": "var(--bg-surface)",
        opacity: "0.7",
        animation: "pulse 1.5s ease-in-out infinite",
      }}
    />
  );
}

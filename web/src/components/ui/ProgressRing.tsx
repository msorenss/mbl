export interface ProgressRingProps {
  progress: number; // 0 to 1
  size?: number;
  strokeWidth?: number;
  color?: string;
  trackColor?: string;
  label?: string;
}

export function ProgressRing(props: ProgressRingProps) {
  const size = () => props.size || 36;
  const strokeWidth = () => props.strokeWidth || 3;
  const radius = () => (size() - strokeWidth()) / 2;
  const circumference = () => 2 * Math.PI * radius();
  const clamped = () => Math.max(0, Math.min(1, props.progress));
  const strokeDashoffset = () => circumference() - clamped() * circumference();

  return (
    <div
      style={{
        position: "relative",
        width: `${size()}px`,
        height: `${size()}px`,
        display: "inline-flex",
        "align-items": "center",
        "justify-content": "center",
      }}
    >
      <svg
        role="img"
        aria-label={props.label || "Progress"}
        width={size()}
        height={size()}
        style={{ transform: "rotate(-90deg)" }}
      >
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          stroke={props.trackColor || "rgba(255, 255, 255, 0.15)"}
          stroke-width={strokeWidth()}
          fill="none"
        />
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          stroke={props.color || "var(--accent)"}
          stroke-width={strokeWidth()}
          stroke-dasharray={`${circumference()}`}
          stroke-dashoffset={`${strokeDashoffset()}`}
          stroke-linecap="round"
          fill="none"
          style={{ transition: "stroke-dashoffset var(--duration-normal) var(--ease-standard)" }}
        />
      </svg>
      {props.label && (
        <span
          style={{
            position: "absolute",
            "font-size": "10px",
            "font-weight": "var(--font-weight-bold)",
            color: "var(--text-primary)",
          }}
        >
          {props.label}
        </span>
      )}
    </div>
  );
}

/**
 * Bar.tsx — the progress bar primitive used by both Codex rows and the
 * OpenRouter row. Per spec §7.3:
 *   - `variant="filled"` (Codex): solid fill, color escalates near the cap.
 *   - `variant="differentiated"` (OpenRouter): visually distinct treatment
 *     so it doesn't read as a third Codex window — outline/ghost style.
 *
 * Color thresholds for the filled variant follow the default in §9:
 *   - < 80%  → neutral
 *   - ≥ 80%  → amber
 *   - ≥ 95%  → red
 */

import "./Bar.css";

export type BarVariant = "filled" | "differentiated";

interface BarProps {
  variant: BarVariant;
  /** Fill ratio in [0, 100]. Clamped internally. */
  fill: number;
}

function clampFill(fill: number): number {
  if (Number.isNaN(fill)) return 0;
  if (fill < 0) return 0;
  if (fill > 100) return 100;
  return fill;
}

function filledClass(fill: number): string {
  if (fill >= 95) return "bar-filled bar-warn-critical";
  if (fill >= 80) return "bar-filled bar-warn-high";
  return "bar-filled";
}

export function Bar({ variant, fill }: BarProps) {
  const pct = clampFill(fill);

  if (variant === "differentiated") {
    return (
      <div
        className="bar bar-differentiated"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
      >
        <div className="bar-differentiated-fill" style={{ width: `${pct}%` }} />
      </div>
    );
  }

  return (
    <div
      className={`bar ${filledClass(pct)}`}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct}
    >
      <div className="bar-filled-inner" style={{ width: `${pct}%` }} />
    </div>
  );
}

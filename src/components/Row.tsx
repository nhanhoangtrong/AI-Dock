/**
 * Row.tsx — one of the three rows in the popover (§1.4):
 *
 *   - Codex Primary window
 *   - Codex Secondary window
 *   - OpenRouter credits
 *
 * Each row renders:
 *   - a label on the left
 *   - a Bar (variant passed in)
 *   - secondary text on the right (e.g. "$2.50 / $10.00" or "resets 14:32")
 *   - optionally a caption beneath the bar (stale-dim or error caption)
 *
 * The row receives pre-shaped props rather than the raw `Status` payload so
 * App.tsx stays the source of truth for variant selection (Codex = filled,
 * OpenRouter = differentiated, per §7.3 + ADR-0003).
 */

import { Bar, type BarVariant } from "./Bar";
import "./Row.css";

interface RowProps {
  label: string;
  /** Right-aligned secondary text. Pass empty string to hide. */
  text?: string;
  /** Plan badge shown between label and bar (e.g. "Plus"). */
  badge?: string;
  /** Fill ratio [0, 100]. Ignored when `state` is "error". */
  fill: number;
  variant: BarVariant;
  /** Display state. Drives the caption and whether the bar dims. */
  state: "ok" | "stale" | "error";
  /** Caption shown beneath the bar when state ≠ "ok". */
  caption?: string;
}

export function Row({
  label,
  text,
  badge,
  fill,
  variant,
  state,
  caption,
}: RowProps) {
  const dim = state === "stale";
  return (
    <div className={`row ${dim ? "row-dim" : ""}`}>
      <div className="row-header">
        <div className="row-label">
          <span className="row-label-text">{label}</span>
          {badge ? <span className="row-badge">{badge}</span> : null}
        </div>
        {text ? <div className="row-text">{text}</div> : null}
      </div>
      {state === "error" ? null : <Bar variant={variant} fill={fill} />}
      {caption ? (
        <div
          className={`row-caption ${
            state === "error" ? "row-caption-error" : ""
          }`}
        >
          {caption}
        </div>
      ) : null}
    </div>
  );
}

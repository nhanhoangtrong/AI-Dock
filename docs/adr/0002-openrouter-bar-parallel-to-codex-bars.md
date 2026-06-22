# Render OpenRouter credits as a filled bar visually parallel to Codex rate-limit bars

Status: superseded by ADR-0003

## Context

The popover shows two Codex rows (Primary 5h window, Secondary 7d window) as filled progress bars of `used_percent`, and one OpenRouter row. Codex bars are time-bounded rate-limits that reset; OpenRouter's value is prepaid spend that only accrues and never resets until topped up.

## Decision

Render the OpenRouter row as a filled progress bar in the same visual treatment as the Codex bars — fill ratio `total_usage / total_credits`, with the literal `$used / $total` text alongside.

## Why

The user chose visual parallelism across all three rows over a differentiated treatment, accepting the conflation risk in exchange for a unified glanceable shape language.

## Considered options

- **Differentiated bar (p1, rejected):** OpenRouter rendered as a different bar treatment (outline/ghost, or neutral color, or `$` glyph chip) so a glance can't conflate "I'm being throttled" with "I'm spending prepaid credits." Rejected as unnecessary visual variation.
- **Bar + explicit non-reset label (p2, rejected):** OpenRouter bar with a "spent (doesn't reset)" caption making the semantic difference literal. Rejected as clutter for a glance UI.

## Consequences

- **Flagged miscue (recorded dissent):** Three visually identical bars invite the misreading "high bar = I'm about to be blocked" on the OpenRouter row, when in fact a high OpenRouter bar means "I've spent most of my prepaid credits, which is the intended use of them." Codex bars reset on a clock; the OpenRouter bar only moves when dollars move. A future reader maintaining this UI should treat the parallelism as a deliberate user choice, not an accident to "fix" by differentiating the bars — that would re-litigate a resolved decision.
- The literal `$used / $total` text is the disambiguating signal that survives the visual parallelism; do not remove it.

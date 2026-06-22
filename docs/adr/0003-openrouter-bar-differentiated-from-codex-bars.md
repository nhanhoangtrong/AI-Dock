# Render OpenRouter credits as a bar treatment distinct from the Codex rate-limit bars

Status: accepted (supersedes ADR-0002)

## Context

The popover shows two Codex rows (Primary 5h, Secondary 7d) as filled bars of `used_percent`, and one OpenRouter row. Codex bars are time-bounded rate-limits that reset on a clock; OpenRouter's value is prepaid spend that only accrues until topped up. ADR-0002 chose full visual parallelism (three identical filled bars); the user reversed that after accepting that the parallelism is a real miscue.

## Decision

Render the OpenRouter row as a progress bar in a *distinct treatment* from the Codex rate-limit bars — same shape language (a bar), but immediately distinguishable by treatment (e.g. outline/ghost instead of filled, or neutral color, or a `$` glyph chip) so a glance cannot conflate "I am about to be rate-limited" with "I am spending prepaid credits." Keep the literal `$used / $total` text alongside.

## Why

Identical bars invited the misreading "high bar = I'm about to be blocked" on the OpenRouter row, when a high OpenRouter bar actually means "I've spent most of my prepaid credits, which is the intended use of them." Codex bars reset on a clock; the OpenRouter bar only moves when dollars move. A differentiated treatment makes the kind-difference visible without abandoning the bar metaphor, so the popover stays visually coherent without lying about what the numbers mean.

## Considered options

- **Identical filled bars (ADR-0002, superseded):** maximum visual parallelism, but conflates rate-limit with spend on the most glanceable surface. Reversed by the user.
- **Bar + explicit non-reset label (p2, rejected):** an honest caption ("spent, doesn't reset") but clutters a glance UI; the differentiated treatment carries the same signal more cheaply.
- **No bar, text only (rejected):** abandons the bar metaphor entirely, making the OpenRouter row a different shape from the Codex rows. Differentiation should be *within* the bar family, not a different shape.

## Consequences

- A future reader should not "fix" the differentiated treatment by re-parallelizing the bars — that re-litigates a resolved decision and re-introduces the miscue.
- The literal `$used / $total` text remains the source of truth; the bar is a secondary visual cue. Do not remove the text.
- The exact differentiator (outline vs. neutral color vs. glyph) is an implementation detail not worth specifying here; pick whichever reads cleanest at the size the tray popover renders.

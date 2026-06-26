# Read Codex rate-limit telemetry from the CLI logs database

Status: Superseded by ADR-0005 for the primary Codex usage source.

## Context

We need to surface the user's ChatGPT-plan Codex rate limits: plan type (Plus), two rolling windows (5h primary, 7d secondary), `used_percent` per window, and absolute reset timestamps. The ChatGPT backend-api (`chatgpt.com/backend-api`) can return this on demand, but it is undocumented and requires the CLI's OAuth access token on the wire.

## Decision

Read the latest `codex.rate_limits` event from the Codex CLI's local logs database (`~/.codex/logs_2.sqlite`) instead of calling the ChatGPT backend-api.

## Why

The CLI already logs a structured JSON event on every turn containing every field we need — `plan_type`, `rate_limits.allowed`, `limit_reached`, `primary`/`secondary` with `used_percent`, `window_minutes`, `reset_after_seconds`, `reset_at`, plus slots for `credits`/`promo`/`code_review_rate_limits`. Reading it is local, needs no auth, has no 401/degraded states, and the schema is Codex's own contract — more stable than a reverse-engineered HTTP endpoint whose shape OpenAI can change any week.

## Considered options

- **ChatGPT backend-api (rejected):** live on-demand freshness, but undocumented, puts the OAuth token on the wire, introduces 401/token-expired degraded states, and forces us to either own token refresh or degrade when the CLI hasn't run. The freshness gain is negligible for a heavy Codex user whose log data is at worst minutes stale.
- **Log-DB + backend-api hybrid (rejected):** re-introduces the backend-api's fragility on the exact "refresh now" path where it hurts most. Defer to a v2 if log staleness proves a real problem.

## Consequences

- The Codex display is only as fresh as the user's last Codex turn. We surface this honestly with an "as of N min ago" timestamp rather than pretending to be live.
- If Codex changes its `codex.rate_limits` event schema, parsing breaks — but that schema is Codex's own internal contract, far more stable than a reverse-engineered HTTP endpoint.
- The app does not read or write `~/.codex/auth.json` for the Codex side; the only credential in the app is the OpenRouter API key.

# Read Codex subscription usage from ChatGPT wham API

## Context

The original Codex signal came from `~/.codex/logs_2.sqlite` (ADR-0001). That avoided credentials, but it only updated after Codex emitted a fresh local event and could become stale or be confused by unrelated log rows containing `codex.rate_limits`.

The desired behavior is live subscription usage from the same ChatGPT login used by the Codex CLI.

## Decision

Use `GET https://chatgpt.com/backend-api/wham/usage` as the primary Codex source, authenticated with the Codex CLI ChatGPT OAuth token from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`.

The first implementation covers the API-backed Session and Weekly windows, plan name, extra-usage balance when present, and reset-credit count embedded in the usage response. Spend tiles and reset-credit expiry details are deferred.

## Why

The API returns current `rate_limit.primary_window` and `rate_limit.secondary_window` fields without waiting for a local Codex turn. That fixes the practical staleness problem with the log database while preserving the existing two-window UI model.

## Consequences

- The Codex provider now reads and writes Codex auth credentials.
- ChatGPT login is required. API-key-only Codex setups cannot show subscription usage.
- A `401` or `403` triggers one OAuth refresh through `auth.openai.com` and one retry.
- The endpoint is not a stable public API. If the shape changes, Codex rows show a format error rather than falling back to stale local logs.
- ADR-0001 is superseded for the primary Codex source. Local spend from logs remains a possible future feature.

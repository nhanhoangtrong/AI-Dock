# Read Claude Code subscription usage from the OAuth usage API

## Context

We need to surface Claude Code subscription usage alongside Codex, OpenRouter, and DeepSeek. The desired display is the real utilization for the same practical windows the user cares about elsewhere in the app: a 5-hour window and a weekly window.

Claude Code local session logs under `~/.claude/projects/**/*.jsonl` include token usage per assistant message, but they do not expose the subscription limit denominator or official reset windows. Aggregating JSONL logs would show local token volume, not actual subscription utilization.

OpenUsage documents a reverse-engineered Claude Code usage endpoint:

- Source: `https://raw.githubusercontent.com/robinebers/openusage/refs/heads/main/docs/providers/claude.md`
- `GET https://api.anthropic.com/api/oauth/usage`
- Required beta header: `anthropic-beta: oauth-2025-04-20`
- Auth: Claude Code OAuth access token
- Response windows: `five_hour`, `seven_day`, optional `seven_day_opus`, optional `seven_day_omelette`, optional `extra_usage`

This endpoint is undocumented and may change without notice.

## Decision

Use the reverse-engineered Claude Code OAuth usage endpoint for the Claude Code provider, because it is the only known source that returns real subscription utilization percentages and reset timestamps.

Render the initial Claude Code provider as two rows:

1. `Claude Code · 5h window` from `five_hour.utilization` and `five_hour.resets_at`.
2. `Claude Code · weekly window` from `seven_day.utilization` and `seven_day.resets_at`.

Add Claude Code to the generic provider visibility list and default it to visible. Do not add a Settings API-key input. Settings should describe the source in-label, following the current hint-in-label style, for example `Claude Code (OAuth from Keychain)`.

## Authentication

Read Claude Code credentials using the source order documented by OpenUsage:

1. macOS Keychain service `Claude Code-credentials-<sha256(CLAUDE_CONFIG_DIR).slice(0, 8)>` when `CLAUDE_CONFIG_DIR` is set.
2. macOS Keychain service `Claude Code-credentials`.
3. Fallback file `~/.claude/.credentials.json`.

The credential JSON shape contains:

```json
{
  "claudeAiOauth": {
    "accessToken": "<jwt>",
    "refreshToken": "<token>",
    "expiresAt": 1738300000000,
    "subscriptionType": "pro",
    "rateLimitTier": "..."
  }
}
```

Access tokens are short-lived. Refresh proactively 5 minutes before expiration and reactively on `401` or `403` by calling:

```text
POST https://platform.claude.com/v1/oauth/token
```

with the Claude Code client id documented by OpenUsage:

```text
9d1c250a-e61b-44d9-88ed-5944d1962f5e
```

If token refresh succeeds, persist the refreshed token data back to the same credential store that supplied the original credential. If persistence fails, use the refreshed access token for the current poll and surface a stale/error state on the next poll if credentials are no longer usable.

## Error and stale policy

Follow the existing app policy: never render a fake zero for failed data.

- Missing credentials: `Claude Code: no credentials found — run Claude Code once.`
- Keychain/file read failure: `Claude Code: could not read credentials.`
- Token refresh failure: `Claude Code: sign in again.`
- Usage endpoint auth rejection after refresh: `Claude Code: sign in again.`
- Network/transient failure with last-known value available: stale row with last-known utilization and a caption.
- Network/transient failure without last-known value: error caption, no bar.
- Unexpected response shape: `Claude Code: usage format changed — check for app update.`

## Considered options

- **Aggregate local JSONL logs (rejected):** local and credential-free, but it cannot produce true 5-hour or weekly subscription utilization. It would show token volume, not quota usage.
- **Manual user-entered Claude key (rejected):** the usage endpoint uses Claude Code OAuth, not an Anthropic API key. Asking for a key would be misleading.
- **Hide Claude Code behind an opt-in toggle (rejected):** provider visibility already lets users hide providers. Default visible is consistent with the provider model.

## Consequences

- The Claude Code provider will be more fragile than Codex because it depends on an undocumented HTTP endpoint and OAuth credential details.
- The app will read Claude Code OAuth credentials. Unlike Codex, this cannot be implemented as a credential-free local log read while still showing real subscription utilization.
- The implementation must keep this risk visible in code comments and user-facing error states.
- If Anthropic changes the endpoint, headers, token format, or Keychain service naming, the Claude Code provider can fail independently while the other providers continue working.

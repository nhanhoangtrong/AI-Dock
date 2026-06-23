# ai-dock

A macOS menu-bar app that surfaces Codex rate-limit status and OpenRouter credit balance as a glanceable status item.

## Language

**Codex**:
The OpenAI Codex CLI as run under a ChatGPT subscription login (Plus), distinct from the OpenAI Platform API.
_Avoid_: Codex API, the old Codex model, "ChatGPT API"

**Primary window**:
The shorter of Codex's two rolling rate-limit windows (5 hours). The limit that interrupts a running session.
_Avoid_: 5h limit, short window, hourly limit

**Secondary window**:
The longer of Codex's two rolling rate-limit windows (7 days). The limit that decides whether to route work elsewhere for the day.
_Avoid_: 7d limit, long window, weekly limit

**OpenRouter credits**:
The documented GET `/api/v1/credits` response shape: `{total_credits, total_usage}`. Remaining is derived as `total_credits - total_usage`.
_Avoid_: balance, wallet, budget

**Claude Code subscription usage**:
Claude Code's OAuth-backed subscription utilization windows, distinct from Anthropic Platform API usage or spend. The app reads the reverse-engineered `/api/oauth/usage` response for `five_hour` and `seven_day` utilization.
_Avoid_: Claude API usage, Anthropic API spend, token count

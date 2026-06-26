/**
 * App.test.ts — unit tests for the pure helpers and row builders in App.tsx.
 *
 * Covers the recently-touched logic (reset-time formatting past tense,
 * error-vs-ok row shape, no-bar DeepSeek row) plus a sanity check on the
 * shape builders so they don't silently drift.
 */

import { describe, expect, test } from "vitest";

import {
  buildClaudeRows,
  buildCodexRows,
  buildDeepSeekRow,
  buildOpenRouterRow,
  formatDeepSeekBalance,
  formatMoney,
  formatResetAt,
  titleCase,
  type CodexStatus,
  type ClaudeStatus,
  type DeepSeekStatus,
  type OpenRouterStatus,
  type WindowStatus,
} from "./App";

// ---------- fixtures ----------

function windowStatus(used_percent: number, reset_at: number): WindowStatus {
  return {
    used_percent,
    window_minutes: 300,
    reset_after_seconds: 1800,
    reset_at,
  };
}

const NOW = 1_700_000_000;

const codexOk: CodexStatus = {
  kind: "ok",
  plan_type: "plus",
  primary: windowStatus(73, NOW + 1800),
  secondary: windowStatus(41, NOW + 86400),
  event_ts: NOW,
  reset_credits_available: 2,
};

const codexError: CodexStatus = {
  kind: "error",
  message: "Codex: no rate-limit data yet.",
};

// ---------- formatResetAt ----------

describe("formatResetAt", () => {
  test("returns past tense when reset is in the past", () => {
    expect(formatResetAt(NOW - 1, NOW)).toBe("reset");
    expect(formatResetAt(NOW, NOW)).toBe("reset");
  });

  test("formats minutes when under one hour", () => {
    expect(formatResetAt(NOW + 60, NOW)).toBe("resets in 1m");
    expect(formatResetAt(NOW + 59 * 60, NOW)).toBe("resets in 59m");
  });

  test("formats hours and minutes when under one day", () => {
    expect(formatResetAt(NOW + 60 * 60, NOW)).toBe("resets in 1h");
    expect(formatResetAt(NOW + 90 * 60, NOW)).toBe("resets in 1h 30m");
    expect(formatResetAt(NOW + 23 * 3600, NOW)).toBe("resets in 23h");
  });

  test("formats days and hours when over one day", () => {
    expect(formatResetAt(NOW + 86400, NOW)).toBe("resets in 1d");
    expect(formatResetAt(NOW + 86400 + 3600, NOW)).toBe("resets in 1d 1h");
    expect(formatResetAt(NOW + 7 * 86400, NOW)).toBe("resets in 7d");
  });
});

// ---------- formatters ----------

describe("formatMoney", () => {
  test("formats with two decimals and a dollar sign", () => {
    expect(formatMoney(0)).toBe("$0.00");
    expect(formatMoney(25.75)).toBe("$25.75");
    expect(formatMoney(100.5)).toBe("$100.50");
  });
});

describe("formatDeepSeekBalance", () => {
  test("USD prefix for dollars", () => {
    expect(formatDeepSeekBalance(4.12, "USD")).toBe("$4.12");
  });

  test("currency code prefix for non-USD", () => {
    expect(formatDeepSeekBalance(123.45, "EUR")).toBe("EUR 123.45");
  });
});

describe("titleCase", () => {
  test("uppercases the first letter, leaves the rest alone", () => {
    expect(titleCase("plus")).toBe("Plus");
    expect(titleCase("")).toBe("");
    expect(titleCase("PLUS")).toBe("PLUS");
  });
});

// ---------- row builders ----------

describe("buildCodexRows", () => {
  test("ok state: filled bars with reset text", () => {
    const rows = buildCodexRows(codexOk, NOW);
    // `text` only exists in the ok/stale branch of the return union.
    const { primary, secondary } = rows as {
      primary: { label: string; fill: number; variant: "filled"; state: "ok"; text: string };
      secondary: { label: string; fill: number; variant: "filled"; state: "ok"; text: string };
    };
    expect(primary.label).toBe("Codex · Session");
    expect(primary.fill).toBe(73);
    expect(primary.variant).toBe("filled");
    expect(primary.state).toBe("ok");
    expect(primary.text).toContain("73%");
    expect(primary.text).toContain("resets in 30m");

    expect(secondary.label).toBe("Codex · weekly window");
    expect(secondary.fill).toBe(41);
    expect(secondary.text).toContain("41%");
    expect(rows.resetCredits?.text).toBe("2 available");
  });

  test("fresh unused session shows not started", () => {
    const fresh: CodexStatus = {
      kind: "ok",
      plan_type: "plus",
      primary: {
        used_percent: 0,
        window_minutes: 300,
        reset_after_seconds: 17_980,
        reset_at: NOW + 17_980,
      },
      secondary: windowStatus(41, NOW + 86400),
      event_ts: NOW,
    };

    const rows = buildCodexRows(fresh, NOW);
    const primary = rows.primary as { text: string };

    expect(primary.text).toBe("0% · Not started");
  });

  test("error state: no fill, caption carries the message", () => {
    const rows = buildCodexRows(codexError, NOW);
    expect(rows.primary.state).toBe("error");
    expect(rows.primary.fill).toBe(0);
    expect(rows.primary.caption).toBe("Codex: no rate-limit data yet.");
    expect(rows.secondary.caption).toBe("Codex: no rate-limit data yet.");
  });

  test("stale state: still renders bars but caption is set", () => {
    const stale: CodexStatus = {
      kind: "stale",
      plan_type: "plus",
      primary: windowStatus(50, NOW + 1800),
      secondary: windowStatus(20, NOW + 86400),
      event_ts: NOW,
      message: "Codex: log read failed: timeout",
    };
    const rows = buildCodexRows(stale, NOW);
    expect(rows.primary.state).toBe("stale");
    expect(rows.primary.fill).toBe(50);
    expect(rows.primary.caption).toContain("timeout");
  });
});

describe("buildOpenRouterRow", () => {
  test("ok state: differentiated variant, fill from usage ratio", () => {
    const ok: OpenRouterStatus = {
      kind: "ok",
      total_credits: 100,
      total_usage: 25,
      remaining: 75,
    };
    const row = buildOpenRouterRow(ok);
    expect(row.variant).toBe("differentiated");
    expect(row.fill).toBe(25); // 25/100 * 100
    expect(row.text).toBe("$25.00 / $100.00");
    expect(row.state).toBe("ok");
  });

  test("ok state with zero credits: fill is 0, no division by zero", () => {
    const ok: OpenRouterStatus = {
      kind: "ok",
      total_credits: 0,
      total_usage: 0,
      remaining: 0,
    };
    const row = buildOpenRouterRow(ok);
    expect(row.fill).toBe(0);
  });

  test("error state: caption carries the message", () => {
    const err: OpenRouterStatus = {
      kind: "error",
      message: "OpenRouter: no key — add one in settings.",
    };
    const row = buildOpenRouterRow(err);
    expect(row.state).toBe("error");
    expect(row.caption).toContain("no key");
  });
});

describe("buildDeepSeekRow", () => {
  test("ok state: no fill (no usage ratio), just balance text", () => {
    const ok: DeepSeekStatus = {
      kind: "ok",
      total_balance: 4.12,
      currency: "USD",
    };
    const row = buildDeepSeekRow(ok);
    expect(row.variant).toBe("differentiated");
    expect(row.fill).toBeUndefined(); // no fill ratio — API returns balance only
    expect(row.text).toBe("$4.12");
    expect(row.state).toBe("ok");
  });

  test("error state: caption carries the message", () => {
    const err: DeepSeekStatus = {
      kind: "error",
      message: "DeepSeek: account unavailable.",
    };
    const row = buildDeepSeekRow(err);
    expect(row.state).toBe("error");
    expect(row.caption).toContain("account unavailable");
  });
});

// ---------- Claude (smoke test — covers the same row builder contract) ----------

describe("buildClaudeRows", () => {
  test("error state surfaces the message on both windows", () => {
    const claude: ClaudeStatus = {
      kind: "error",
      message: "Claude: no data yet.",
    };
    const rows = buildClaudeRows(claude, NOW);
    expect(rows.fiveHour.state).toBe("error");
    expect(rows.fiveHour.caption).toBe("Claude: no data yet.");
    expect(rows.weekly.state).toBe("error");
    expect(rows.weekly.caption).toBe("Claude: no data yet.");
  });
});

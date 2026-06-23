/**
 * App.tsx — popover shell.
 *
 * Subscribes to the `status-update` event emitted by the Rust poll loop
 * (§5.1, §6) and renders visible provider rows + a footer (refresh button,
 * settings). The frontend never polls, fetches, or reads files (§5).
 */

import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { Row } from "./components/Row";
import { Settings } from "./components/Settings";
import {
  defaultProviderVisibility,
  type ProviderId,
  type ProviderVisibility,
  withProviderVisibilityDefaults,
} from "./providers";
import "./App.css";

// ---------- Payload types — mirror the Rust contract (§6) ----------

export type WindowStatus = {
  used_percent: number;
  window_minutes: number;
  reset_after_seconds: number;
  reset_at: number; // unix seconds
};

export type CodexStatus =
  | {
      kind: "ok";
      plan_type: string;
      primary: WindowStatus;
      secondary: WindowStatus;
      event_ts: number;
    }
  | { kind: "error"; message: string }
  | {
      kind: "stale";
      plan_type: string;
      primary: WindowStatus;
      secondary: WindowStatus;
      event_ts: number;
      message: string;
    };

export type ClaudeStatus =
  | {
      kind: "ok";
      five_hour: WindowStatus;
      seven_day: WindowStatus;
      seven_day_opus?: WindowStatus;
      seven_day_omelette?: WindowStatus;
    }
  | { kind: "error"; message: string }
  | {
      kind: "stale";
      five_hour: WindowStatus;
      seven_day: WindowStatus;
      seven_day_opus?: WindowStatus;
      seven_day_omelette?: WindowStatus;
      message: string;
    };

export type OpenRouterStatus =
  | {
      kind: "ok";
      total_credits: number;
      total_usage: number;
      remaining: number;
    }
  | { kind: "error"; message: string }
  | {
      kind: "stale";
      total_credits: number;
      total_usage: number;
      remaining: number;
      message: string;
    };

export type DeepSeekStatus =
  | { kind: "ok"; total_balance: number; currency: string }
  | { kind: "error"; message: string }
  | {
      kind: "stale";
      total_balance: number;
      currency: string;
      message: string;
    };

export type StatusUpdate = {
  codex: CodexStatus;
  claude: ClaudeStatus;
  openrouter: OpenRouterStatus;
  deepseek: DeepSeekStatus;
  polled_at: number;
};

const EMPTY_PAYLOAD: StatusUpdate = {
  codex: { kind: "error", message: "Codex: no rate-limit data yet." },
  claude: {
    kind: "error",
    message: "Claude Code: no credentials found — run Claude Code once.",
  },
  openrouter: {
    kind: "error",
    message: "OpenRouter: no key — add one in settings.",
  },
  deepseek: {
    kind: "error",
    message: "DeepSeek: no key — add one in settings.",
  },
  polled_at: 0,
};

// ---------- Formatting helpers ----------

export function formatResetAt(resetAt: number, nowSec: number): string {
  const delta = resetAt - nowSec;
  if (delta <= 0) return "reset";
  const minutes = Math.floor(delta / 60);
  if (minutes < 60) return `resets in ${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remMin = minutes % 60;
  if (hours < 24)
    return remMin > 0
      ? `resets in ${hours}h ${remMin}m`
      : `resets in ${hours}h`;
  const days = Math.floor(hours / 24);
  const remH = hours % 24;
  return remH > 0 ? `resets in ${days}d ${remH}h` : `resets in ${days}d`;
}

export function formatMoney(n: number): string {
  return `$${n.toFixed(2)}`;
}

export function formatDeepSeekBalance(n: number, currency: string): string {
  // Ponytail: assume USD for most users; prefix non-USD with the code.
  const sym = currency.toUpperCase() === "USD" ? "$" : `${currency} `;
  return `${sym}${n.toFixed(2)}`;
}

function nowSec(): number {
  return Math.floor(Date.now() / 1000);
}

// ---------- App ----------

export default function App() {
  const [status, setStatus] = useState<StatusUpdate>(EMPTY_PAYLOAD);
  const [refreshing, setRefreshing] = useState(false);
  const [providerVisibility, setProviderVisibility] =
    useState<ProviderVisibility>(defaultProviderVisibility);

  // Subscribe to status-update events from Rust (§5.1).
  useEffect(() => {
    const unlistenP = listen<StatusUpdate>("status-update", (e) => {
      setStatus(e.payload);
      setRefreshing(false);
    });
    return () => {
      void unlistenP.then((u) => u());
    };
  }, []);

  useEffect(() => {
    let alive = true;
    void invoke<Record<string, boolean>>("get_provider_visibility")
      .then((visibility) => {
        if (alive) {
          setProviderVisibility(withProviderVisibilityDefaults(visibility));
        }
      })
      .catch((e) => console.error("get_provider_visibility failed", e));
    return () => {
      alive = false;
    };
  }, []);

  // Global Escape key (§1.3) — hides the popover. Frontend path is one of
  // several: tray-click, blur, and this Escape all converge on hide().
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      // Don't hijack Escape when the user is interacting with the settings
      // input (the input has its own Escape handling in Settings.tsx that
      // closes the panel instead of the whole popover).
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (e.key === "Escape" && tag !== "INPUT" && tag !== "TEXTAREA") {
        void invoke("hide_popover").catch(() => {});
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await invoke("refresh_now");
    } catch (e) {
      console.error("refresh_now failed", e);
      setRefreshing(false);
    }
  }, []);

  const onProviderVisibilityChange = useCallback(
    async (provider: ProviderId, visible: boolean) => {
      let previous = true;
      setProviderVisibility((current) => {
        previous = current[provider];
        return { ...current, [provider]: visible };
      });

      try {
        await invoke("set_provider_visibility", { provider, visible });
      } catch (e) {
        console.error("set_provider_visibility failed", e);
        setProviderVisibility((current) => ({
          ...current,
          [provider]: previous,
        }));
      }
    },
    [],
  );

  // ---- Derived view-model ----

  const codexRow = useMemo(
    () => buildCodexRows(status.codex, nowSec()),
    [status],
  );
  const claudeRow = useMemo(
    () => buildClaudeRows(status.claude, nowSec()),
    [status],
  );
  const openrouterRow = useMemo(
    () => buildOpenRouterRow(status.openrouter),
    [status],
  );
  const deepseekRow = useMemo(
    () => buildDeepSeekRow(status.deepseek),
    [status],
  );
  const providerGroups = [
    {
      id: "codex" as const,
      rows: (
        <>
          <Row {...codexRow.primary} />
          <Row {...codexRow.secondary} />
        </>
      ),
    },
    {
      id: "claude" as const,
      rows: (
        <>
          <Row {...claudeRow.fiveHour} />
          <Row {...claudeRow.weekly} />
        </>
      ),
    },
    {
      id: "openrouter" as const,
      rows: <Row {...openrouterRow} />,
    },
    {
      id: "deepseek" as const,
      rows: <Row {...deepseekRow} />,
    },
  ].filter((group) => providerVisibility[group.id]);

  return (
    <div className="popover">
      <div className="provider-list">
        {providerGroups.length > 0 ? (
          providerGroups.map((group, index) => (
            <Fragment key={group.id}>
              {index > 0 ? <div className="popover-divider" /> : null}
              {group.rows}
            </Fragment>
          ))
        ) : (
          <div className="popover-empty">No providers visible</div>
        )}
      </div>
      <div className="popover-footer">
        <button
          type="button"
          className="refresh-button"
          onClick={() => void onRefresh()}
          aria-label="Refresh now"
          title="Refresh now"
          disabled={refreshing}
        >
          <RefreshIcon spinning={refreshing} />
          <span>{refreshing ? "Refreshing…" : "Refresh"}</span>
        </button>
        <div className="footer-actions">
          <Settings
            providerVisibility={providerVisibility}
            onProviderVisibilityChange={onProviderVisibilityChange}
          />
          <button
            type="button"
            className="quit-button"
            onClick={() => void invoke("quit_app")}
            aria-label="Quit"
            title="Quit"
          >
            <QuitIcon />
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------- Row builders ----------

export function buildCodexRows(codex: CodexStatus, now: number) {
  if (codex.kind === "error") {
    return {
      primary: {
        label: "Codex · 5h window",
        fill: 0,
        variant: "filled" as const,
        state: "error" as const,
        caption: codex.message,
      },
      secondary: {
        label: "Codex · weekly window",
        fill: 0,
        variant: "filled" as const,
        state: "error" as const,
        caption: codex.message,
      },
    };
  }

  const badge = titleCase(codex.plan_type);
  const base = {
    variant: "filled" as const,
    badge,
    state: codex.kind === "stale" ? ("stale" as const) : ("ok" as const),
    caption: codex.kind === "stale" ? codex.message : undefined,
  };

  return {
    primary: {
      ...base,
      label: "Codex · 5h window",
      fill: codex.primary.used_percent,
      text: `${Math.round(codex.primary.used_percent)}% · ${formatResetAt(
        codex.primary.reset_at,
        now,
      )}`,
    },
    secondary: {
      ...base,
      label: "Codex · weekly window",
      fill: codex.secondary.used_percent,
      text: `${Math.round(codex.secondary.used_percent)}% · ${formatResetAt(
        codex.secondary.reset_at,
        now,
      )}`,
    },
  };
}

export function buildOpenRouterRow(or: OpenRouterStatus) {
  if (or.kind === "error") {
    return {
      label: "OpenRouter",
      fill: 0,
      variant: "differentiated" as const,
      state: "error" as const,
      caption: or.message,
    };
  }

  const pct =
    or.total_credits > 0 ? (or.total_usage / or.total_credits) * 100 : 0;
  return {
    label: "OpenRouter",
    badge: "credits",
    fill: pct,
    variant: "differentiated" as const,
    state: or.kind === "stale" ? ("stale" as const) : ("ok" as const),
    text: `${formatMoney(or.total_usage)} / ${formatMoney(or.total_credits)}`,
    caption: or.kind === "stale" ? or.message : undefined,
  };
}

export function buildClaudeRows(claude: ClaudeStatus, now: number) {
  if (claude.kind === "error") {
    return {
      fiveHour: {
        label: "Claude Code · 5h window",
        fill: 0,
        variant: "filled" as const,
        state: "error" as const,
        caption: claude.message,
      },
      weekly: {
        label: "Claude Code · weekly window",
        fill: 0,
        variant: "filled" as const,
        state: "error" as const,
        caption: claude.message,
      },
    };
  }

  const base = {
    variant: "filled" as const,
    badge: "subscription",
    state: claude.kind === "stale" ? ("stale" as const) : ("ok" as const),
    caption: claude.kind === "stale" ? claude.message : undefined,
  };

  return {
    fiveHour: {
      ...base,
      label: "Claude Code · 5h window",
      fill: claude.five_hour.used_percent,
      text: `${Math.round(claude.five_hour.used_percent)}% · ${formatResetAt(
        claude.five_hour.reset_at,
        now,
      )}`,
    },
    weekly: {
      ...base,
      label: "Claude Code · weekly window",
      fill: claude.seven_day.used_percent,
      text: `${Math.round(claude.seven_day.used_percent)}% · ${formatResetAt(
        claude.seven_day.reset_at,
        now,
      )}`,
    },
  };
}

export function buildDeepSeekRow(ds: DeepSeekStatus) {
  if (ds.kind === "error") {
    return {
      label: "DeepSeek",
      fill: undefined,
      variant: "differentiated" as const,
      state: "error" as const,
      caption: ds.message,
    };
  }

  return {
    label: "DeepSeek",
    badge: "balance",
    fill: undefined, // no fill ratio — DeepSeek only returns remaining balance
    variant: "differentiated" as const,
    state: ds.kind === "stale" ? ("stale" as const) : ("ok" as const),
    text: formatDeepSeekBalance(ds.total_balance, ds.currency),
    caption: ds.kind === "stale" ? ds.message : undefined,
  };
}

export function titleCase(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function RefreshIcon({ spinning }: { spinning: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={spinning ? "refresh-icon spinning" : "refresh-icon"}
    >
      <path d="M21 12a9 9 0 1 1-3.46-7.05" />
      <polyline points="21 4 21 10 15 10" />
    </svg>
  );
}

function QuitIcon() {
  // Lucide "circle-arrow-out-up-right" — no icon library dependency.
  return (
    <svg
      width="1em"
      height="1em"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="lucide lucide-circle-arrow-out-up-right-icon lucide-circle-arrow-out-up-right"
    >
      <path d="M22 12A10 10 0 1 1 12 2" />
      <path d="M22 2 12 12" />
      <path d="M16 2h6v6" />
    </svg>
  );
}

/**
 * App.tsx — popover shell.
 *
 * Subscribes to the `status-update` event emitted by the Rust poll loop
 * (§5.1, §6) and renders three rows + a footer (refresh button, settings).
 * The frontend never polls, fetches, or reads files (§5).
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { Row } from "./components/Row";
import { Settings } from "./components/Settings";
import "./App.css";

// ---------- Payload types — mirror the Rust contract (§6) ----------

type WindowStatus = {
  used_percent: number;
  window_minutes: number;
  reset_after_seconds: number;
  reset_at: number; // unix seconds
};

type CodexStatus =
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

type OpenRouterStatus =
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

type StatusUpdate = {
  codex: CodexStatus;
  openrouter: OpenRouterStatus;
  polled_at: number;
};

const EMPTY_PAYLOAD: StatusUpdate = {
  codex: { kind: "error", message: "Codex: no rate-limit data yet." },
  openrouter: { kind: "error", message: "OpenRouter: no key — add one in settings." },
  polled_at: 0,
};

// ---------- Formatting helpers ----------

function formatResetAt(resetAt: number, nowSec: number): string {
  const delta = resetAt - nowSec;
  if (delta <= 0) return "reset";
  const minutes = Math.floor(delta / 60);
  if (minutes < 60) return `resets in ${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remMin = minutes % 60;
  if (hours < 24) return remMin > 0 ? `resets in ${hours}h ${remMin}m` : `resets in ${hours}h`;
  const days = Math.floor(hours / 24);
  const remH = hours % 24;
  return remH > 0 ? `resets in ${days}d ${remH}h` : `resets in ${days}d`;
}

function formatMoney(n: number): string {
  return `$${n.toFixed(2)}`;
}

function nowSec(): number {
  return Math.floor(Date.now() / 1000);
}

// ---------- App ----------

export default function App() {
  const [status, setStatus] = useState<StatusUpdate>(EMPTY_PAYLOAD);
  const [refreshing, setRefreshing] = useState(false);

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

  // ---- Derived view-model ----

  const codexRow = useMemo(() => buildCodexRows(status.codex, nowSec()), [status]);
  const openrouterRow = useMemo(
    () => buildOpenRouterRow(status.openrouter),
    [status],
  );

  return (
    <div className="popover">
      <Row {...codexRow.primary} />
      <Row {...codexRow.secondary} />
      <div className="popover-divider" />
      <Row {...openrouterRow} />
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
        <Settings />
      </div>
    </div>
  );
}

// ---------- Row builders ----------

function buildCodexRows(codex: CodexStatus, now: number) {
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

function buildOpenRouterRow(or: OpenRouterStatus) {
  if (or.kind === "error") {
    return {
      label: "OpenRouter",
      fill: 0,
      variant: "differentiated" as const,
      state: "error" as const,
      caption: or.message,
    };
  }

  const pct = or.total_credits > 0 ? (or.total_usage / or.total_credits) * 100 : 0;
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

function titleCase(s: string): string {
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

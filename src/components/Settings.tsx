/**
 * Settings.tsx — the OpenRouter key entry affordance (§1.4, §3).
 *
 * The key never crosses back to the frontend after save (Rust re-reads the
 * config file on every poll). This component just sends the new value via
 * `invoke("set_openrouter_key", { key })`. The "saved" indicator is purely
 * a UI acknowledgement — no payload comes back.
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./Settings.css";

type View = "closed" | "open" | "saved";

export function Settings() {
  const [view, setView] = useState<View>("closed");
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Autofocus when the field opens.
  useEffect(() => {
    if (view === "open") {
      inputRef.current?.focus();
    }
  }, [view]);

  function open() {
    setError(null);
    setDraft("");
    setView("open");
  }

  function close() {
    setView("closed");
    setError(null);
    setDraft("");
  }

  async function save() {
    const trimmed = draft.trim();
    if (trimmed.length === 0) {
      setError("Key cannot be empty");
      return;
    }
    try {
      await invoke("set_openrouter_key", { key: trimmed });
      setView("saved");
      setDraft("");
      // Auto-collapse back to the icon after a beat.
      window.setTimeout(() => setView("closed"), 1200);
    } catch (e) {
      setError(String(e));
    }
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      void save();
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }

  if (view === "closed") {
    return (
      <button
        type="button"
        className="settings-toggle"
        aria-label="OpenRouter settings"
        onClick={open}
      >
        <SettingsIcon />
      </button>
    );
  }

  // Open: render an overlay anchored to the popover (its closest positioned
  // ancestor — see App.css .popover { position: relative }). The backdrop
  // dismisses on click; the card itself centers the input.
  return (
    <div className="settings-overlay" role="dialog" aria-modal="true" aria-label="OpenRouter settings">
      <button
        type="button"
        className="settings-overlay-backdrop"
        onClick={close}
        aria-label="Close settings"
        tabIndex={-1}
      />
      <div className="settings-card">
        <div className="settings-card-header">
          <span className="settings-card-title">OpenRouter key</span>
          <button
            type="button"
            className="settings-card-close"
            onClick={close}
            aria-label="Close"
          >
            ×
          </button>
        </div>
        <label className="settings-label" htmlFor="openrouter-key">
          Management key
        </label>
        <div className="settings-row">
          <input
            id="openrouter-key"
            ref={inputRef}
            type="password"
            className="settings-input"
            placeholder="sk-or-v1-..."
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={onKeyDown}
            autoComplete="off"
            spellCheck={false}
          />
          <button
            type="button"
            className="settings-save"
            onClick={() => void save()}
            disabled={view === "saved"}
          >
            {view === "saved" ? "Saved" : "Save"}
          </button>
        </div>
        {error ? <div className="settings-error">{error}</div> : null}
        <div className="settings-hint">
          Stored in <code>~/.config/ai-dock/config.json</code>. Use a
          management key, not a chat key.
        </div>
      </div>
    </div>
  );
}

function SettingsIcon() {
  // A small inline gear — no icon library dependency.
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.04 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.04-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.55-1.04H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.6 8.91a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.04-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.04 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.4 9c.16.39.51.66.93.74H21a2 2 0 1 1 0 4h-.09c-.42.08-.77.35-.93.74Z" />
    </svg>
  );
}

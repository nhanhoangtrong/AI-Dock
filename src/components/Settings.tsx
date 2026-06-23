/**
 * Settings.tsx — API key entry and provider visibility controls.
 *
 * Keys never cross back to the frontend after save (Rust re-reads the config
 * file on every poll). Provider toggles are generic so future providers can
 * reuse the same visibility flow.
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  PROVIDERS,
  type ProviderId,
  type ProviderVisibility,
} from "../providers";
import "./Settings.css";

type View = "closed" | "open";
type KeyTarget = "openrouter" | "deepseek" | null;

interface KeyState {
  draft: string;
  saved: boolean;
  error: string | null;
}

function emptyKey(): KeyState {
  return { draft: "", saved: false, error: null };
}

interface SettingsProps {
  providerVisibility: ProviderVisibility;
  onProviderVisibilityChange: (provider: ProviderId, visible: boolean) => void;
}

export function Settings({
  providerVisibility,
  onProviderVisibilityChange,
}: SettingsProps) {
  const [view, setView] = useState<View>("closed");
  const [target, setTarget] = useState<KeyTarget>(null);
  const [or, setOr] = useState<KeyState>(emptyKey);
  const [ds, setDs] = useState<KeyState>(emptyKey);

  const orRef = useRef<HTMLInputElement | null>(null);
  const dsRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (target === "openrouter") orRef.current?.focus();
    else if (target === "deepseek") dsRef.current?.focus();
  }, [target]);

  function open() {
    // Reset all draft/error state each time the panel opens.
    setOr(emptyKey());
    setDs(emptyKey());
    setTarget(null);
    setView("open");
  }

  function close() {
    setView("closed");
    setTarget(null);
    setOr(emptyKey());
    setDs(emptyKey());
  }

  async function saveKey(
    kind: "openrouter" | "deepseek",
    value: string,
    setter: (s: KeyState) => void,
  ) {
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      setter({ draft: value, saved: false, error: "Key cannot be empty" });
      return;
    }
    const cmd =
      kind === "openrouter" ? "set_openrouter_key" : "set_deepseek_key";
    try {
      await invoke(cmd, { key: trimmed });
      setter({ draft: value, saved: true, error: null });
      window.setTimeout(() => setter(emptyKey()), 1200);
    } catch (e: unknown) {
      setter({
        draft: value,
        saved: false,
        error: String(e),
      });
    }
  }

  function onKeyDown(
    e: React.KeyboardEvent<HTMLInputElement>,
    kind: "openrouter" | "deepseek",
    value: string,
    setter: (s: KeyState) => void,
  ) {
    if (e.key === "Enter") {
      e.preventDefault();
      void saveKey(kind, value, setter);
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
        aria-label="Settings"
        onClick={open}
      >
        <SettingsIcon />
      </button>
    );
  }

  return (
    <div
      className="settings-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
    >
      <button
        type="button"
        className="settings-overlay-backdrop"
        onClick={close}
        aria-label="Close settings"
        tabIndex={-1}
      />
      <div className="settings-card">
        <div className="settings-card-header">
          <span className="settings-card-title">Settings</span>
          <button
            type="button"
            className="settings-card-close"
            onClick={close}
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <div className="settings-group-title">API keys</div>

        {/* ---- OpenRouter ---- */}
        <div className="settings-section">
          <label className="settings-label" htmlFor="or-key">
            OpenRouter (management key)
          </label>
          <div className="settings-row">
            <input
              id="or-key"
              ref={orRef}
              type="password"
              className="settings-input"
              placeholder="sk-or-v1-..."
              value={or.draft}
              onChange={(e) =>
                setOr({ draft: e.target.value, saved: false, error: null })
              }
              onKeyDown={(e) => onKeyDown(e, "openrouter", or.draft, setOr)}
              autoComplete="off"
              spellCheck={false}
            />
            <button
              type="button"
              className="settings-save"
              onClick={() => void saveKey("openrouter", or.draft, setOr)}
              disabled={or.saved}
            >
              {or.saved ? "Saved" : "Save"}
            </button>
          </div>
          {or.error ? <div className="settings-error">{or.error}</div> : null}
        </div>

        {/* ---- DeepSeek ---- */}
        <div className="settings-section">
          <label className="settings-label" htmlFor="ds-key">
            DeepSeek (API key)
          </label>
          <div className="settings-row">
            <input
              id="ds-key"
              ref={dsRef}
              type="password"
              className="settings-input"
              placeholder="sk-..."
              value={ds.draft}
              onChange={(e) =>
                setDs({ draft: e.target.value, saved: false, error: null })
              }
              onKeyDown={(e) => onKeyDown(e, "deepseek", ds.draft, setDs)}
              autoComplete="off"
              spellCheck={false}
            />
            <button
              type="button"
              className="settings-save"
              onClick={() => void saveKey("deepseek", ds.draft, setDs)}
              disabled={ds.saved}
            >
              {ds.saved ? "Saved" : "Save"}
            </button>
          </div>
          {ds.error ? <div className="settings-error">{ds.error}</div> : null}
        </div>

        {/* ---- Providers ---- */}
        <div className="settings-section">
          <div className="settings-group-title">Providers</div>
          <div className="provider-toggle-list">
            {PROVIDERS.map((provider) => (
              <label className="provider-toggle-row" key={provider.id}>
                <span className="provider-toggle-label">{provider.label}</span>
                <span className="provider-toggle-control">
                  <input
                    type="checkbox"
                    className="provider-toggle-input"
                    checked={providerVisibility[provider.id]}
                    onChange={(e) =>
                      onProviderVisibilityChange(provider.id, e.target.checked)
                    }
                  />
                  <span className="provider-toggle-track" aria-hidden="true">
                    <span className="provider-toggle-thumb" />
                  </span>
                </span>
              </label>
            ))}
          </div>
        </div>

        <div className="settings-footer-hint">
          Stored in <code>~/.config/ai-dock/config.json</code>.
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

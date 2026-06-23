export const PROVIDERS = [
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude Code (OAuth from Keychain)" },
  { id: "openrouter", label: "OpenRouter" },
  { id: "deepseek", label: "DeepSeek" },
] as const;

export type ProviderId = (typeof PROVIDERS)[number]["id"];
export type ProviderVisibility = Record<ProviderId, boolean>;

export function defaultProviderVisibility(): ProviderVisibility {
  return Object.fromEntries(
    PROVIDERS.map((provider) => [provider.id, true]),
  ) as ProviderVisibility;
}

export function withProviderVisibilityDefaults(
  input: Partial<Record<string, boolean>>,
): ProviderVisibility {
  return Object.fromEntries(
    PROVIDERS.map((provider) => [
      provider.id,
      input[provider.id] ?? true,
    ]),
  ) as ProviderVisibility;
}

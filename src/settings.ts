import type { DisplayMode, ProviderId, Settings } from "./types";

export const MIN_POLLING_SECONDS = 30;
export const MAX_POLLING_SECONDS = 3600;
export const DEFAULT_SETTINGS: Settings = {
  codexEnabled: true,
  antigravityEnabled: true,
  claudeCodeEnabled: false,
  pollingIntervalSeconds: 300,
  alwaysOnTop: true,
  displayMode: "bar",
};

export function normalizePollingInterval(value: unknown): number {
  const numeric = typeof value === "number" && Number.isFinite(value) ? Math.round(value) : DEFAULT_SETTINGS.pollingIntervalSeconds;
  return Math.min(MAX_POLLING_SECONDS, Math.max(MIN_POLLING_SECONDS, numeric));
}

export function normalizeDisplayMode(value: unknown): DisplayMode {
  return value === "gauge" ? "gauge" : "bar";
}

export function normalizeSettings(input: Partial<Settings> | null | undefined): Settings {
  return {
    codexEnabled: input?.codexEnabled ?? DEFAULT_SETTINGS.codexEnabled,
    antigravityEnabled: input?.antigravityEnabled ?? DEFAULT_SETTINGS.antigravityEnabled,
    claudeCodeEnabled: input?.claudeCodeEnabled ?? DEFAULT_SETTINGS.claudeCodeEnabled,
    pollingIntervalSeconds: normalizePollingInterval(input?.pollingIntervalSeconds),
    alwaysOnTop: input?.alwaysOnTop ?? DEFAULT_SETTINGS.alwaysOnTop,
    displayMode: normalizeDisplayMode(input?.displayMode),
  };
}

export function enabledProviders(settings: Settings): ProviderId[] {
  const providers: ProviderId[] = [];
  if (settings.codexEnabled) providers.push("codex");
  if (settings.antigravityEnabled) providers.push("antigravity");
  if (settings.claudeCodeEnabled) providers.push("claudeCode");
  return providers;
}

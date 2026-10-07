export type ProviderId = "codex" | "antigravity" | "claudeCode";
export type ProviderStatus = "loading" | "ok" | "error" | "auth" | "unavailable";
export type DisplayMode = "bar" | "gauge";

export interface Settings {
  codexEnabled: boolean;
  antigravityEnabled: boolean;
  claudeCodeEnabled: boolean;
  pollingIntervalSeconds: number;
  alwaysOnTop: boolean;
  displayMode: DisplayMode;
}

export interface QuotaWindow {
  id: string;
  label: string;
  remainingPercent: number;
  windowDurationMins: number | null;
  resetAt: number | null;
}

export interface QuotaGroup {
  id: string;
  title: string;
  windows: QuotaWindow[];
}

export interface ProviderSnapshot {
  provider: ProviderId;
  status: ProviderStatus;
  message: string | null;
  updatedAt: number | null;
  stale: boolean;
  groups: QuotaGroup[];
}

export type SnapshotMap = Partial<Record<ProviderId, ProviderSnapshot>>;

export const PROVIDER_META: Record<ProviderId, { name: string; description: string }> = {
  codex: { name: "Codex", description: "Codex 帳號配額" },
  antigravity: { name: "Antigravity", description: "Gemini 與 Claude / GPT 模型" },
  claudeCode: { name: "Claude Code", description: "Claude 訂閱配額（5 小時 / 每週）" },
};

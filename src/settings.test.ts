import { describe, expect, it } from "vitest";
import {
  DEFAULT_SETTINGS,
  MAX_POLLING_SECONDS,
  MIN_POLLING_SECONDS,
  enabledProviders,
  normalizePollingInterval,
  normalizeSettings,
} from "./settings";

describe("polling settings", () => {
  it("uses a five-minute default", () => {
    expect(normalizeSettings(undefined)).toEqual(DEFAULT_SETTINGS);
  });

  it("bounds and rounds polling intervals", () => {
    expect(normalizePollingInterval(12)).toBe(MIN_POLLING_SECONDS);
    expect(normalizePollingInterval(3600.4)).toBe(MAX_POLLING_SECONDS);
    expect(normalizePollingInterval(121.6)).toBe(122);
    expect(normalizePollingInterval("bad")).toBe(300);
  });

  it("does not include disabled providers", () => {
    expect(enabledProviders({ ...DEFAULT_SETTINGS, antigravityEnabled: false })).toEqual(["codex"]);
    expect(enabledProviders({ ...DEFAULT_SETTINGS, codexEnabled: false, claudeCodeEnabled: true })).toEqual(["antigravity", "claudeCode"]);
  });

  it("loads legacy settings without displayMode as bar", () => {
    expect(normalizeSettings({ codexEnabled: false, pollingIntervalSeconds: 120 })).toMatchObject({
      codexEnabled: false,
      pollingIntervalSeconds: 120,
      displayMode: "bar",
    });
  });

  it("falls back to bar for an unknown display mode", () => {
    expect(normalizeSettings({ displayMode: "future-mode" } as unknown as Partial<typeof DEFAULT_SETTINGS>).displayMode).toBe("bar");
  });
});

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./settings";
import { renderApp, type ViewState } from "./ui";

describe("rendered quota gauges", () => {
  it.each([true, false])("renders separate dials and exact legends, expanded=%s", (expanded) => {
    const state: ViewState = {
      settings: { ...DEFAULT_SETTINGS, displayMode: "gauge", antigravityEnabled: false },
      snapshots: { codex: {
        provider: "codex", status: "ok", message: null, updatedAt: null, stale: false,
        groups: [1, 2].map((count) => ({ id: String(count), title: `Group ${count}`,
          windows: Array.from({ length: count }, (_, index) => ({
            id: String(index), label: `Window ${index}`, remainingPercent: 48,
            windowDurationMins: 300, resetAt: 1790942400,
          })),
        })),
      } },
      refreshing: false, expanded, settingsOpen: false, settingsDraft: null,
      settingsWasCollapsed: false, notice: null,
    };
    const html = renderApp(state);
    // Collapsed markup also retains the hidden expanded cards.
    const visibleHtml = expanded ? html : html.split('<div class="provider-list">')[0];
    expect((visibleHtml.match(/class="gauge-svg"/g) ?? [])).toHaveLength(2);
    expect((visibleHtml.match(/class="gauge-pointer pointer-/g) ?? [])).toHaveLength(3);
    expect(visibleHtml).toContain("48.0%");
    expect(visibleHtml).not.toContain("NaN");
  });
});

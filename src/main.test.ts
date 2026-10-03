import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./settings";
import { enabledProviders } from "./settings";

describe("frontend provider polling selection", () => {
  it("keeps disabled services out of the polling list", () => {
    expect(enabledProviders({ ...DEFAULT_SETTINGS, codexEnabled: false })).toEqual(["antigravity"]);
    expect(enabledProviders({ ...DEFAULT_SETTINGS, antigravityEnabled: false, claudeCodeEnabled: true })).toEqual(["codex", "claudeCode"]);
  });
});

import { describe, expect, it } from "vitest";
import type { QuotaGroup, QuotaWindow } from "../types";
import {
  createGaugeModels,
  createGaugeModelsForGroups,
  gaugeAngleForPercent,
  usableGaugeWindows,
} from "./gauge";

function quotaWindow(id: string, label: string, remainingPercent: number): QuotaWindow {
  return { id, label, remainingPercent, windowDurationMins: null, resetAt: 1_700_000_000 };
}

function quotaGroup(id: string, title: string, windows: QuotaWindow[]): QuotaGroup {
  return { id, title, windows };
}

describe("gauge model", () => {
  it("creates one pointer for a single quota window", () => {
    const [model] = createGaugeModels(quotaGroup("codex", "Codex", [quotaWindow("week", "一週", 48)]));

    expect(model.pointers).toHaveLength(1);
    expect(model.windows).toHaveLength(1);
    expect(model.pointers[0].percent).toBe(48);
  });

  it("keeps two windows on one dial with distinguishable pointers", () => {
    const [model] = createGaugeModels(quotaGroup("codex", "Codex", [
      quotaWindow("week", "一週", 48),
      quotaWindow("five-hour", "5 小時", 48),
    ]));

    expect(model.pointers).toHaveLength(2);
    expect(model.pointers[0].color).not.toBe(model.pointers[1].color);
    expect(model.pointers[0].length).not.toBe(model.pointers[1].length);
  });

  it("keeps exact 0% and 100% boundaries on the dial", () => {
    const [model] = createGaugeModels(quotaGroup("limits", "邊界", [
      quotaWindow("zero", "0%", 0),
      quotaWindow("full", "100%", 100),
    ]));

    expect(model.pointers.map((pointer) => pointer.percent)).toEqual([0, 100]);
    expect(gaugeAngleForPercent(0)).toBe(135);
    expect(gaugeAngleForPercent(100)).toBe(405);
  });

  it("splits more than two windows into extra dials without dropping them", () => {
    const windows = [1, 2, 3, 4, 5].map((value) => quotaWindow(`window-${value}`, `視窗 ${value}`, value * 10));
    const models = createGaugeModels(quotaGroup("many", "多視窗", windows));

    expect(models).toHaveLength(3);
    expect(models.map((model) => model.pointers.length)).toEqual([2, 2, 1]);
    expect(models.flatMap((model) => model.windows)).toHaveLength(5);
  });

  it("does not combine independent quota groups into one dial", () => {
    const models = createGaugeModelsForGroups([
      quotaGroup("gemini", "Gemini 模型", [quotaWindow("week", "一週", 80), quotaWindow("hour", "5 小時", 20)]),
      quotaGroup("claude-gpt", "Claude 與 GPT 模型", [quotaWindow("week", "一週", 60)]),
    ]);

    expect(models).toHaveLength(2);
    expect(models.map((model) => model.groupId)).toEqual(["gemini", "claude-gpt"]);
  });

  it("does not create a pointer for missing or invalid data", () => {
    const missing = createGaugeModels(quotaGroup("empty", "無資料", []));
    const invalid = createGaugeModels(quotaGroup("invalid", "無效", [quotaWindow("nan", "未知", Number.NaN)]));

    expect(missing).toHaveLength(0);
    expect(invalid).toHaveLength(0);
    expect(usableGaugeWindows([quotaWindow("nan", "未知", Number.NaN)])).toHaveLength(0);
  });
});

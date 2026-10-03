import type { QuotaGroup, QuotaWindow } from "../types";

export const GAUGE_POINTER_COLORS = ["#83aaff", "#70d9b4"] as const;
const MAX_POINTERS_PER_GAUGE = 2;

export interface GaugePointer {
  window: QuotaWindow;
  percent: number;
  angle: number;
  length: number;
  color: string;
  index: number;
}

export interface GaugeModel {
  groupId: string;
  groupTitle: string;
  windows: QuotaWindow[];
  pointers: GaugePointer[];
  page: number;
  pageCount: number;
}

/** Return a bounded percentage, or null when the provider did not supply a number. */
export function normalizeGaugePercent(value: number): number | null {
  if (!Number.isFinite(value)) return null;
  return Math.min(100, Math.max(0, value));
}

/** Filter invalid windows without turning missing data into a fake 0% value. */
export function usableGaugeWindows(windows: readonly QuotaWindow[]): QuotaWindow[] {
  return windows.filter((window) => normalizeGaugePercent(window.remainingPercent) !== null);
}

export function splitGaugeWindows(windows: readonly QuotaWindow[]): QuotaWindow[][] {
  const usable = usableGaugeWindows(windows);
  const chunks: QuotaWindow[][] = [];
  for (let index = 0; index < usable.length; index += MAX_POINTERS_PER_GAUGE) {
    chunks.push(usable.slice(index, index + MAX_POINTERS_PER_GAUGE));
  }
  return chunks;
}

/** 0% is lower-left, 50% is the top, and 100% is lower-right. */
export function gaugeAngleForPercent(value: number): number {
  return 135 + (normalizeGaugePercent(value) ?? 0) * 2.7;
}

export function gaugePointForPercent(value: number, radius: number, centerX = 100, centerY = 88): { x: number; y: number } {
  const radians = (gaugeAngleForPercent(value) * Math.PI) / 180;
  return {
    x: centerX + Math.cos(radians) * radius,
    y: centerY + Math.sin(radians) * radius,
  };
}

export function gaugePointerLength(pointerCount: number, pointerIndex: number): number {
  if (pointerCount < 2) return 53;
  return pointerIndex === 0 ? 55 : 43;
}

export function createGaugeModels(group: QuotaGroup): GaugeModel[] {
  const chunks = splitGaugeWindows(group.windows);
  return chunks.map((windows, pageIndex) => ({
    groupId: group.id,
    groupTitle: group.title,
    windows,
    pointers: windows.map((window, index) => {
      const percent = normalizeGaugePercent(window.remainingPercent) ?? 0;
      return {
        window,
        percent,
        angle: gaugeAngleForPercent(percent),
        length: gaugePointerLength(windows.length, index),
        color: GAUGE_POINTER_COLORS[index] ?? GAUGE_POINTER_COLORS[0],
        index,
      };
    }),
    page: pageIndex,
    pageCount: chunks.length,
  }));
}

/** Groups are deliberately flattened only after each group has its own models. */
export function createGaugeModelsForGroups(groups: readonly QuotaGroup[]): GaugeModel[] {
  return groups.flatMap((group) => createGaugeModels(group));
}

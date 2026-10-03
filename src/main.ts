import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DEFAULT_SETTINGS, normalizeSettings } from "./settings";
import { compactWindowHeight, renderApp, type ViewState } from "./ui";
import type { ProviderSnapshot, Settings, SnapshotMap } from "./types";
import "./styles.css";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("找不到應用程式根節點");
const app: HTMLDivElement = root;

let state: ViewState = {
  settings: DEFAULT_SETTINGS,
  snapshots: {},
  refreshing: false,
  expanded: true,
  settingsOpen: false,
  settingsDraft: null,
  settingsWasCollapsed: false,
  notice: null,
};
let pollTimer: number | undefined;
let noticeTimer: number | undefined;

function render(): void {
  app.innerHTML = renderApp(state);
  bindActions();
}

function bindActions(): void {
  app.querySelectorAll<HTMLElement>("[data-action]").forEach((element) => {
    element.addEventListener("click", () => void handleAction(element.dataset.action));
  });
  app.querySelectorAll<HTMLInputElement>("[data-setting]").forEach((input) => {
    const update = () => updateSettingsDraft(input);
    input.addEventListener("input", update);
    input.addEventListener("change", update);
  });
  const titlebar = app.querySelector<HTMLElement>(".titlebar");
  titlebar?.addEventListener("mousedown", (event) => {
    if ((event.target as HTMLElement).closest("button")) return;
    void getCurrentWindow().startDragging().catch((error: unknown) => {
      showNotice(readableError(error, "無法拖曳視窗"));
    });
  });
}

async function handleAction(action: string | undefined): Promise<void> {
  switch (action) {
    case "toggle-expand":
      await toggleExpanded();
      break;
    case "toggle-settings":
      if (state.settingsOpen) {
        await closeSettings();
      } else {
        await openSettings();
      }
      break;
    case "close-settings":
      await closeSettings();
      break;
    case "save-settings":
      await saveSettingsFromPanel();
      break;
    case "refresh":
      await refresh();
      break;
    case "hide":
      try {
        await invoke("hide_to_tray");
      } catch (error) {
        showNotice(readableError(error, "無法最小化到系統匣"));
      }
      break;
  }
}

async function toggleExpanded(): Promise<void> {
  const expanded = !state.expanded;
  try {
    await setNativeWindowMode(expanded, state);
    state = { ...state, expanded };
    render();
  } catch (error) {
    showNotice(readableError(error, expanded ? "無法展開視窗" : "無法收合視窗"));
  }
}

async function openSettings(): Promise<void> {
  const wasCollapsed = !state.expanded;
  if (wasCollapsed) {
    try {
      await setNativeWindowMode(true, state);
    } catch (error) {
      showNotice(readableError(error, "無法暫時展開設定"));
      return;
    }
  }
  state = {
    ...state,
    expanded: true,
    settingsOpen: true,
    settingsDraft: { ...state.settings },
    settingsWasCollapsed: wasCollapsed,
    notice: null,
  };
  render();
}

async function closeSettings(): Promise<void> {
  const restoreCollapsed = state.settingsWasCollapsed;
  if (restoreCollapsed) {
    try {
      await setNativeWindowMode(false, state);
    } catch (error) {
      showNotice(readableError(error, "無法還原迷你視窗"));
      return;
    }
  }
  state = {
    ...state,
    expanded: restoreCollapsed ? false : state.expanded,
    settingsOpen: false,
    settingsDraft: null,
    settingsWasCollapsed: false,
  };
  render();
}

async function saveSettingsFromPanel(): Promise<void> {
  const next = normalizeSettings(state.settingsDraft ?? state.settings);
  try {
    const saved = await invoke<Settings>("save_settings", { settings: next });
    const savedSettings = normalizeSettings(saved);
    const restoreCollapsed = state.settingsWasCollapsed;
    let restoreError: unknown;
    if (restoreCollapsed) {
      try {
        await setNativeWindowMode(false, { ...state, settings: savedSettings });
      } catch (error) {
        restoreError = error;
      }
    }
    state = {
      ...state,
      settings: savedSettings,
      expanded: restoreCollapsed ? false : state.expanded,
      settingsOpen: false,
      settingsDraft: null,
      settingsWasCollapsed: false,
      notice: null,
    };
    resetPollingTimer();
    render();
    if (restoreError) {
      showNotice(readableError(restoreError, "設定已儲存，但無法還原迷你視窗"));
    }
    await refresh();
  } catch (error) {
    // Keep the independent draft in state so a failed save never erases edits.
    showNotice(readableError(error, "設定儲存失敗"));
  }
}

async function refresh(): Promise<void> {
  if (state.refreshing) return;
  state = { ...state, refreshing: true, notice: null };
  render();
  try {
    const snapshots = await invoke<ProviderSnapshot[]>("refresh_quotas");
    const nextState = { ...state, snapshots: toSnapshotMap(snapshots), refreshing: false };
    let compactError: unknown;
    if (!nextState.expanded) {
      try {
        await setNativeWindowMode(false, nextState);
      } catch (error) {
        compactError = error;
      }
    }
    state = {
      ...nextState,
      notice: compactError ? readableError(compactError, "無法調整迷你視窗大小") : null,
    };
  } catch (error) {
    state = { ...state, refreshing: false };
    showNotice(readableError(error, "更新失敗，請稍後重試"));
  }
  render();
}

async function setNativeWindowMode(expanded: boolean, sourceState: ViewState): Promise<void> {
  await invoke("set_compact_mode", {
    compact: !expanded,
    compactHeight: compactWindowHeight(sourceState),
  });
}

function updateSettingsDraft(input: HTMLInputElement): void {
  const draft = { ...(state.settingsDraft ?? state.settings) };
  if (input.type === "checkbox") {
    (draft as unknown as Record<string, boolean | number | string>)[input.dataset.setting ?? ""] = input.checked;
  } else if (input.type === "radio" && input.dataset.setting === "displayMode" && (input.value === "bar" || input.value === "gauge")) {
    draft.displayMode = input.value;
  } else if (input.dataset.setting === "pollingIntervalSeconds" && input.value !== "") {
    draft.pollingIntervalSeconds = Number(input.value);
  }
  state = { ...state, settingsDraft: draft };
}

function toSnapshotMap(snapshots: ProviderSnapshot[]): SnapshotMap {
  return snapshots.reduce<SnapshotMap>((map, snapshot) => {
    map[snapshot.provider] = snapshot;
    return map;
  }, {});
}

function showNotice(message: string): void {
  if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  state = { ...state, notice: message };
  render();
  noticeTimer = window.setTimeout(() => {
    state = { ...state, notice: null };
    render();
  }, 6000);
}

function readableError(error: unknown, fallback: string): string {
  let message = "";
  if (typeof error === "string") {
    message = error;
  } else if (error instanceof Error) {
    message = error.message;
  } else if (typeof error === "object" && error !== null && "message" in error) {
    const candidate = (error as { message?: unknown }).message;
    if (typeof candidate === "string") message = candidate;
  }
  return /[㐀-鿿]/.test(message) ? message : fallback;
}

function resetPollingTimer(): void {
  if (pollTimer !== undefined) window.clearInterval(pollTimer);
  pollTimer = window.setInterval(() => void refresh(), state.settings.pollingIntervalSeconds * 1000);
}

async function registerBackendEvents(): Promise<void> {
  await listen<ProviderSnapshot[]>("quotas-updated", (event) => {
    state = {
      ...state,
      snapshots: toSnapshotMap(event.payload),
      refreshing: false,
    };
    render();
    if (!state.expanded) void resizeCompactWindow();
  });
  await listen<string>("quota-refresh-error", (event) => {
    showNotice(readableError(event.payload, "系統匣更新失敗，請稍後重試"));
  });
}

async function resizeCompactWindow(): Promise<void> {
  try {
    await setNativeWindowMode(false, state);
  } catch (error) {
    showNotice(readableError(error, "無法調整迷你視窗大小"));
  }
}

async function initialize(): Promise<void> {
  await registerBackendEvents();
  try {
    const settings = await invoke<Settings>("get_settings");
    state = { ...state, settings: normalizeSettings(settings) };
  } catch (error) {
    showNotice(readableError(error, "讀取設定失敗，將使用預設值"));
  }
  render();
  resetPollingTimer();
  await refresh();
}

void initialize();

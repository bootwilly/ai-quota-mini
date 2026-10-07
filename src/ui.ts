import type { DisplayMode, ProviderId, ProviderSnapshot, ProviderStatus, QuotaGroup, QuotaWindow, Settings, SnapshotMap } from "./types";
import { PROVIDER_META } from "./types";
import {
  createGaugeModels,
  createGaugeModelsForGroups,
  gaugePointForPercent,
  type GaugeModel,
} from "./ui/gauge";

export interface ViewState {
  settings: Settings;
  snapshots: SnapshotMap;
  refreshing: boolean;
  expanded: boolean;
  settingsOpen: boolean;
  settingsDraft: Settings | null;
  settingsWasCollapsed: boolean;
  notice: string | null;
}

const STATUS_LABELS: Record<ProviderStatus, string> = {
  loading: "更新中",
  ok: "已更新",
  error: "暫時錯誤",
  auth: "需要登入",
  unavailable: "無法提供",
};

const PROVIDERS: ProviderId[] = ["codex", "antigravity", "claudeCode"];
const GAUGE_TICK_VALUES = [0, 25, 50, 75, 100];

export function renderApp(state: ViewState): string {
  const updated = latestUpdatedAt(state.snapshots);
  return `
    <div class="app-shell ${state.expanded ? "is-expanded" : "is-collapsed"}">
      <header class="titlebar">
        <div class="brand">
          <span class="brand-mark" aria-hidden="true">◒</span>
          <div>
            <h1>AI Quota Mini</h1>
            <p>本機帳號配額總覽</p>
          </div>
        </div>
        <div class="title-actions" aria-label="視窗操作">
          <button class="icon-button ${state.expanded ? "active" : ""}" data-action="toggle-expand" title="${state.expanded ? "收合" : "展開"}" aria-label="${state.expanded ? "收合" : "展開"}">${state.expanded ? "⌃" : "⌄"}</button>
          <button class="icon-button ${state.settingsOpen ? "active" : ""}" data-action="toggle-settings" title="設定" aria-label="設定">⚙</button>
          <button class="icon-button" data-action="hide" title="最小化到系統匣" aria-label="最小化到系統匣">−</button>
        </div>
      </header>
      <section class="toolbar">
        <div class="sync-copy">
          <span class="sync-dot ${state.refreshing ? "is-pulsing" : ""}" aria-hidden="true"></span>
          <span>${state.refreshing ? "正在同步配額" : updated ? `更新於 ${formatDate(updated)}` : "尚未同步"}</span>
        </div>
        <button class="refresh-button" data-action="refresh" ${state.refreshing ? "disabled" : ""}>
          <span class="refresh-icon ${state.refreshing ? "spin" : ""}" aria-hidden="true">↻</span>
          ${state.refreshing ? "更新中" : "重新整理"}
        </button>
      </section>
      ${!state.expanded ? renderMiniSummary(state) : ""}
      ${state.notice ? `<div class="notice" role="status">${escapeHtml(state.notice)}</div>` : ""}
      <div class="provider-list">
        ${renderProviderCard("codex", state)}
        ${renderProviderCard("antigravity", state)}
        ${renderProviderCard("claudeCode", state)}
      </div>
      <footer class="footer-note">
        <span>每 ${Math.round(state.settings.pollingIntervalSeconds / 60)} 分鐘自動更新</span>
        <span class="footer-separator">·</span>
        <span>時間：台北</span>
      </footer>
      ${state.settingsOpen ? renderSettings(state.settingsDraft ?? state.settings) : ""}
    </div>
  `;
}

export function compactWindowHeight(state: ViewState): number {
  if (state.settings.displayMode !== "gauge") return 230;
  const enabled = PROVIDERS.filter((provider) => isEnabled(provider, state.settings));
  if (enabled.length === 0) return 230;
  const modelCount = enabled.reduce((count, provider) => {
    const snapshot = state.snapshots[provider];
    return count + (snapshot ? createGaugeModelsForGroups(snapshot.groups).length : 0);
  }, 0);
  const noDataProviderCount = enabled.filter((provider) => {
    const snapshot = state.snapshots[provider];
    return !snapshot || createGaugeModelsForGroups(snapshot.groups).length === 0;
  }).length;
  const contentUnits = Math.max(modelCount * 1.7, noDataProviderCount * 0.85, 1.2);
  return Math.round(285 + contentUnits * 100);
}

function renderMiniSummary(state: ViewState): string {
  const enabled = PROVIDERS.filter((provider) => isEnabled(provider, state.settings));
  if (enabled.length === 0) {
    return `<section class="mini-summary empty-state">沒有啟用中的監控服務</section>`;
  }
  if (state.settings.displayMode === "gauge") {
    return renderMiniGaugeSummary(enabled, state);
  }
  return `
    <section class="mini-summary" aria-label="配額摘要">
      ${enabled.map((provider) => {
        const snapshot = state.snapshots[provider];
        const remaining = snapshot ? lowestRemaining(snapshot) : null;
        const status = snapshot?.stale ? "stale" : snapshot?.status ?? "loading";
        return `<div class="mini-stat">
          <span class="mini-stat-name"><span class="mini-stat-icon ${provider}">${providerIcon(provider)}</span>${PROVIDER_META[provider].name}</span>
          <strong class="mini-stat-value ${status}">${remaining === null ? "—" : formatPercent(remaining)}</strong>
        </div>`;
      }).join("")}
    </section>
  `;
}

function renderMiniGaugeSummary(enabled: ProviderId[], state: ViewState): string {
  return `
    <section class="mini-gauge-summary" aria-label="指針配額摘要">
      ${enabled.map((provider) => renderMiniGaugeProvider(provider, state.snapshots[provider])).join("")}
    </section>
  `;
}

function renderMiniGaugeProvider(provider: ProviderId, snapshot: ProviderSnapshot | undefined): string {
  if (!snapshot) {
    return `<section class="mini-gauge-provider">
      ${renderMiniProviderHeading(provider, "等待更新")}
      <p class="mini-gauge-empty">尚未取得配額，不繪製指針。</p>
    </section>`;
  }

  const models = createGaugeModelsForGroups(snapshot.groups);
  const statusClass = snapshot.stale ? "stale" : snapshot.status;
  const statusLabel = snapshot.stale ? "資料過期" : STATUS_LABELS[snapshot.status];
  const content = models.length > 0
    ? snapshot.groups.map((group) => renderGaugeModels(provider, group, snapshot.stale, true, provider === "antigravity" ? antigravityGroupTitle(group.title) : group.title)).join("")
    : `<p class="mini-gauge-empty">${escapeHtml(snapshot.message ?? "目前沒有可顯示的配額，不繪製指針。")}</p>`;

  return `<section class="mini-gauge-provider ${statusClass}">
    ${renderMiniProviderHeading(provider, statusLabel)}
    ${content}
    ${snapshot.stale && models.length > 0 ? `<p class="gauge-stale-note">資料過期，以下為最後成功的配額。</p>` : ""}
  </section>`;
}

function renderMiniProviderHeading(provider: ProviderId, statusLabel: string): string {
  return `<div class="mini-gauge-heading">
    <span class="mini-gauge-name"><span class="mini-stat-icon ${provider}">${providerIcon(provider)}</span>${PROVIDER_META[provider].name}</span>
    <span class="mini-gauge-status">${escapeHtml(statusLabel)}</span>
  </div>`;
}

function lowestRemaining(snapshot: ProviderSnapshot): number | null {
  const values = snapshot.groups.flatMap((group) => group.windows.map((window) => window.remainingPercent)).filter(Number.isFinite);
  return values.length > 0 ? Math.min(...values) : null;
}

function renderProviderCard(provider: ProviderId, state: ViewState): string {
  const enabled = isEnabled(provider, state.settings);
  const snapshot = state.snapshots[provider];
  const meta = PROVIDER_META[provider];
  if (!enabled) {
    return `
      <article class="provider-card is-disabled">
        <div class="provider-heading">
          <div class="provider-icon ${provider}">${providerIcon(provider)}</div>
          <div class="provider-name"><h2>${meta.name}</h2><p>${meta.description}</p></div>
          <span class="status-pill disabled">已停用</span>
        </div>
        <p class="disabled-copy">已停用，不會執行輪詢。</p>
      </article>
    `;
  }
  if (!snapshot) {
    return renderEmptyCard(provider, meta.name, meta.description);
  }

  const statusClass = snapshot.stale ? "stale" : snapshot.status;
  const statusLabel = snapshot.stale ? "資料過期" : STATUS_LABELS[snapshot.status];
  const message = snapshot.message ? `<p class="provider-message">${escapeHtml(snapshot.message)}</p>` : "";
  const groups = snapshot.groups.map((group) => renderGroup(provider, group, state.settings.displayMode)).join("");
  const noQuota = snapshot.groups.length === 0 ? `<div class="empty-state">${escapeHtml(snapshot.message ?? "目前沒有可顯示的配額")}</div>` : "";

  return `
    <article class="provider-card ${statusClass}">
      <div class="provider-heading">
        <div class="provider-icon ${provider}">${providerIcon(provider)}</div>
        <div class="provider-name"><h2>${meta.name}</h2><p>${meta.description}</p></div>
        <span class="status-pill ${statusClass}"><span class="status-dot"></span>${statusLabel}</span>
      </div>
      ${message}
      <div class="quota-groups">${groups || noQuota}</div>
      ${snapshot.updatedAt ? `<div class="card-updated">最後成功資料 ${formatDate(snapshot.updatedAt)}</div>` : ""}
    </article>
  `;
}

function renderEmptyCard(provider: ProviderId, name: string, description: string): string {
  return `
    <article class="provider-card is-loading">
      <div class="provider-heading">
        <div class="provider-icon ${provider}">${providerIcon(provider)}</div>
        <div class="provider-name"><h2>${name}</h2><p>${description}</p></div>
        <span class="status-pill loading"><span class="status-dot"></span>等待更新</span>
      </div>
      <div class="loading-lines"><span></span><span></span></div>
    </article>
  `;
}

function renderGroup(provider: ProviderId, group: QuotaGroup, displayMode: DisplayMode): string {
  const title = provider === "antigravity" ? antigravityGroupTitle(group.title) : group.title;
  if (displayMode === "gauge") {
    return renderGaugeModels(provider, group, false, false, title);
  }
  const windows = group.windows.filter((window) => Number.isFinite(window.remainingPercent));
  return `
    <section class="quota-group">
      <h3>${escapeHtml(title)}</h3>
      ${windows.length > 0 ? windows.map((window) => renderBarWindow(title, window)).join("") : `<div class="empty-state">目前沒有可顯示的配額</div>`}
    </section>
  `;
}

function renderBarWindow(title: string, window: QuotaWindow): string {
  const value = Math.min(100, Math.max(0, window.remainingPercent));
  return `<div class="quota-row">
    <div class="quota-label"><span>${escapeHtml(window.label)}</span><strong>${formatPercent(value)}</strong></div>
    <div class="progress-track" role="progressbar" aria-valuenow="${value.toFixed(1)}" aria-valuemin="0" aria-valuemax="100" aria-label="${escapeHtml(title)} ${escapeHtml(window.label)} 剩餘比例">
      <span class="progress-fill ${progressTone(value)}" style="width:${value}%"></span>
    </div>
    <div class="reset-label">${resetLabel(window)}</div>
  </div>`;
}

function renderGaugeModels(provider: ProviderId, group: QuotaGroup, stale: boolean, mini: boolean, displayTitle = group.title): string {
  const models = createGaugeModels(group);
  if (models.length === 0) {
    return `<section class="quota-group gauge-group-empty">
      <h3>${escapeHtml(displayTitle)}</h3>
      <div class="empty-state">目前沒有可顯示的配額，不繪製指針。</div>
    </section>`;
  }
  return `<div class="gauge-panels ${mini ? "is-mini" : ""}">
    ${models.map((model) => renderGauge(model, displayTitle, provider, stale, mini)).join("")}
  </div>`;
}

function renderGauge(model: GaugeModel, title: string, provider: ProviderId, stale: boolean, mini: boolean): string {
  const pageLabel = model.pageCount > 1 ? `（錶盤 ${model.page + 1}/${model.pageCount}）` : "";
  const gaugeTitle = `${title}${pageLabel}`;
  const ticks = GAUGE_TICK_VALUES.map((value) => {
    const outer = gaugePointForPercent(value, 68);
    const inner = gaugePointForPercent(value, 59);
    const label = gaugePointForPercent(value, 80);
    return `<line class="gauge-tick" x1="${svgNumber(inner.x)}" y1="${svgNumber(inner.y)}" x2="${svgNumber(outer.x)}" y2="${svgNumber(outer.y)}"></line><text class="gauge-tick-label" x="${svgNumber(label.x)}" y="${svgNumber(label.y)}">${value}%</text>`;
  }).join("");
  const arcStart = gaugePointForPercent(0, 64);
  const arcEnd = gaugePointForPercent(100, 64);
  const pointers = model.pointers.map((pointer) => {
    const endpoint = gaugePointForPercent(pointer.percent, pointer.length);
    return `<line class="gauge-pointer pointer-${pointer.index}" stroke="${pointer.color}" x1="100" y1="88" x2="${svgNumber(endpoint.x)}" y2="${svgNumber(endpoint.y)}"></line>`;
  }).join("");
  const legend = model.pointers.map((pointer) => `<div class="gauge-legend-item">
    <span class="gauge-swatch pointer-${pointer.index}" style="background:${pointer.color}"></span>
    <span class="gauge-window-copy"><strong>${escapeHtml(pointer.window.label)}</strong><small>${resetLabel(pointer.window)}</small></span>
    <b>${formatPrecisePercent(pointer.percent)}</b>
  </div>`).join("");
  const ariaLabel = `${gaugeTitle}，${model.pointers.map((pointer) => `${pointer.window.label} ${formatPrecisePercent(pointer.percent)}`).join("、")}`;

  return `<section class="quota-gauge ${mini ? "is-mini" : ""} ${stale ? "is-stale" : ""}" data-provider="${provider}">
    <div class="gauge-heading"><h3>${escapeHtml(gaugeTitle)}</h3>${stale ? `<span class="gauge-stale">資料過期</span>` : ""}</div>
    <div class="gauge-layout">
      <svg class="gauge-svg" viewBox="0 0 200 158" role="img" aria-label="${escapeHtml(ariaLabel)}">
        <path class="gauge-arc" d="M ${svgNumber(arcStart.x)} ${svgNumber(arcStart.y)} A 64 64 0 1 1 ${svgNumber(arcEnd.x)} ${svgNumber(arcEnd.y)}"></path>
        <g class="gauge-ticks">${ticks}</g>
        <g class="gauge-pointers">${pointers}</g>
        <circle class="gauge-pivot" cx="100" cy="88" r="5"></circle>
      </svg>
      <div class="gauge-legend" aria-label="${escapeHtml(gaugeTitle)}圖例">${legend}</div>
    </div>
  </section>`;
}

function renderSettings(settings: Settings): string {
  return `
    <div class="settings-backdrop" data-action="close-settings"></div>
    <aside class="settings-panel" aria-label="設定">
      <div class="settings-header"><div><span class="eyebrow">偏好設定</span><h2>監控服務</h2></div><button class="icon-button" data-action="close-settings" aria-label="關閉設定">×</button></div>
      <p class="settings-intro">只會輪詢你開啟的服務，設定會儲存在本機。</p>
      <label class="setting-toggle"><span><strong>Codex</strong><small>讀取帳號配額限制</small></span><input type="checkbox" data-setting="codexEnabled" ${settings.codexEnabled ? "checked" : ""}><i></i></label>
      <label class="setting-toggle"><span><strong>Antigravity</strong><small>Gemini 與 Claude / GPT 模型</small></span><input type="checkbox" data-setting="antigravityEnabled" ${settings.antigravityEnabled ? "checked" : ""}><i></i></label>
      <label class="setting-toggle"><span><strong>Claude Code</strong><small>5 小時與每週配額，沿用 Claude Code 登入</small></span><input type="checkbox" data-setting="claudeCodeEnabled" ${settings.claudeCodeEnabled ? "checked" : ""}><i></i></label>
      <label class="interval-field"><span><strong>更新頻率</strong><small>最短 30 秒，最長 60 分鐘</small></span><div class="number-wrap"><input type="number" min="30" max="3600" step="30" value="${settings.pollingIntervalSeconds}" data-setting="pollingIntervalSeconds"><span>秒</span></div></label>
      <fieldset class="display-mode-field"><legend>配額顯示方式</legend>
        <label class="mode-option"><input type="radio" name="displayMode" value="bar" data-setting="displayMode" ${settings.displayMode === "bar" ? "checked" : ""}><span><strong>長條</strong><small>保留原有長條進度</small></span></label>
        <label class="mode-option"><input type="radio" name="displayMode" value="gauge" data-setting="displayMode" ${settings.displayMode === "gauge" ? "checked" : ""}><span><strong>指針</strong><small>以 0–100% 錶盤顯示每個配額視窗</small></span></label>
      </fieldset>
      <label class="setting-toggle"><span><strong>固定在最上層</strong><small>讓小工具保持可見</small></span><input type="checkbox" data-setting="alwaysOnTop" ${settings.alwaysOnTop ? "checked" : ""}><i></i></label>
      <div class="settings-actions"><button class="secondary-button" data-action="close-settings">取消</button><button class="primary-button" data-action="save-settings">儲存設定</button></div>
    </aside>
  `;
}

function isEnabled(provider: ProviderId, settings: Settings): boolean {
  return provider === "codex" ? settings.codexEnabled : provider === "antigravity" ? settings.antigravityEnabled : settings.claudeCodeEnabled;
}

function antigravityGroupTitle(title: string): string {
  const normalized = title.toLowerCase();
  if (normalized.includes("gemini")) return "Gemini 模型";
  if (normalized.includes("claude") || normalized.includes("gpt")) return "Claude 與 GPT 模型";
  return title;
}

function progressTone(value: number): string {
  if (value <= 10) return "critical";
  if (value <= 30) return "warning";
  return "healthy";
}

function providerIcon(provider: ProviderId): string {
  return provider === "codex" ? "✦" : provider === "antigravity" ? "✺" : "◌";
}

function formatPercent(value: number): string {
  return `${Math.round(value)}%`;
}

function formatPrecisePercent(value: number): string {
  return `${value.toFixed(1)}%`;
}

function resetLabel(window: QuotaWindow): string {
  return window.resetAt !== null ? `重設於 ${formatDate(window.resetAt)}` : "重設時間未提供";
}

function svgNumber(value: number): string {
  return value.toFixed(2);
}

function latestUpdatedAt(snapshots: SnapshotMap): number | null {
  return Object.values(snapshots).reduce<number | null>((latest, snapshot) => {
    if (!snapshot.updatedAt) return latest;
    return latest === null || snapshot.updatedAt > latest ? snapshot.updatedAt : latest;
  }, null);
}

function formatDate(unixSeconds: number): string {
  return new Intl.DateTimeFormat("zh-TW", {
    timeZone: "Asia/Taipei",
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(unixSeconds * 1000));
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);
}

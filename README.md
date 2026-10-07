# AI Quota Mini

Windows 桌面 AI 帳號配額小工具，以 **TypeScript + Tauri v2 + Rust** 開發。可選擇要監控的服務，用長條或指針錶盤查看剩餘百分比、限額視窗與重設時間。這是獨立桌面程式，不需要修改 Codex 或 Antigravity 主程式。

## 支援狀態

| 服務 | 配額來源 | 目前行為 |
| --- | --- | --- |
| Codex | 本機 CLI app-server 的 `account/rateLimits/read` | 顯示實際回傳的限額視窗與剩餘量 |
| Antigravity | 本機 `agy` 結構化 `/usage` 回應 | Gemini 與 Claude / GPT 模型群組分開顯示 |
| Claude Code | 讀取 Claude Code 已登入的 OAuth 憑證，查詢 Claude 帳號用量端點 | 顯示 5 小時與每週剩餘量（若有則含 Opus / Sonnet 每週） |

不同 CLI 版本或帳號類型可能沒有配額資料。沒有資料時會顯示狀態，不推估剩餘量。Antigravity 的 Claude / GPT 配額不等於 Claude 訂閱配額；單次 CLI 執行的 token 數與費用也不等於帳號剩餘量。

## 功能與操作

- 深色透明圓角浮動視窗，可拖曳標題列、調整大小與設定置頂。
- 可分別啟用 Codex、Antigravity、Claude Code；停用服務不輪詢。
- 更新間隔 30–3600 秒，預設 300 秒，可手動重新整理。
- 長條模式顯示各視窗的剩餘百分比與重設時間。
- 指針模式採用 SVG，各 quota group 分別建立錶盤。
- 支援展開與 mini 模式；指針 mini 依錶盤數調整視窗高度。
- 系統匣提供顯示、更新、最小化與退出功能。
- 更新失敗保留最後成功資料並標示過期，不把失敗當成 0%。
- 重設時間以台北時區顯示，界面使用繁體中文。

啟動後按齒輪，選擇服務、更新間隔、顯示方式及置頂，再按「儲存設定」。箭頭切換展開 / 收合；減號最小化至系統匣。結束程式請從系統匣選擇退出。

### 指針規則

| 同一群組的有效限額視窗數 | 顯示 |
| --- | --- |
| 0 | 狀態文字，不畫指針 |
| 1 | 一個錶盤、一支指針 |
| 2 | 一個錶盤、兩支指針 |
| 3 個以上 | 每兩個視窗分錶，保留全部視窗 |

雙指針使用不同顏色、長度及線條樣式；數值相同時仍可辨識。圖例顯示視窗名稱、百分比（小數一位）及重設時間。0% 與 100% 都有效，無資料或非有限數值不產生假指針。Antigravity 的 Gemini 與 Claude / GPT 各自分錶。

## 環境需求

目前交付及驗證平台為 **Windows x64**，需要 WebView2 Runtime。各 provider 使用已安裝、已登入且可從 `PATH` 啟動的本機 CLI。

| 工具 | 用途 |
| --- | --- |
| Codex CLI | 讀取 Codex 帳號配額 |
| Antigravity `agy` CLI | 讀取 Antigravity 配額 |
| Node.js 22+、npm | 前端開發及建置 |
| Rust stable MSVC toolchain | Tauri 後端編譯 |
| Visual Studio C++ Build Tools、Windows SDK | 原生編譯及連結 |

倉庫保存原始碼與鎖定檔，不包含依賴目錄、執行檔、個人設定、派工日誌或測試帳號截圖。

## 開發與建置

```powershell
git clone https://github.com/bootwilly/ai-quota-mini.git
cd ai-quota-mini
npm ci
npm run tauri:dev
```

單獨 `npm run dev` 只啟動 Vite；配額查詢及視窗控制需要 Tauri 原生環境。

```powershell
npm run test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml
npx tauri build --bundles nsis
```

`npm run tauri:build` 會嘗試設定中的 NSIS 與 MSI。若只需要已驗證的 NSIS，使用上方指定 bundle 的命令。

| 產物 | 路徑 |
| --- | --- |
| 獨立執行檔 | `src-tauri/target/release/ai-quota-mini.exe` |
| NSIS 安裝程式 | `src-tauri/target/release/bundle/nsis/` |
| MSI 安裝程式 | `src-tauri/target/release/bundle/msi/`，僅成功建置時存在 |

NSIS 已成功建置；MSI 先前因 WiX 下載逾時未完成，沒有已驗證的 MSI 成品。

## 架構與資料流程

```text
TypeScript UI / settings draft / polling
                │ Tauri invoke + events
                ▼
Rust app state / settings persistence / tray
                ├─ Codex app-server JSON-RPC
                ├─ Antigravity agy structured JSON
                └─ Claude OAuth usage endpoint (HTTPS)
```

前端處理呈現、設定草稿、更新排程與視窗模式；Rust 處理固定 CLI 指令、解析、設定讀寫與系統匣。資料階層為 provider → quota group → quota window，保留每個模型群組及限額。

### Codex

後端啟動 `codex.cmd app-server --listen stdio://`，依序進行 JSON-RPC `initialize`、`initialized`、`account/rateLimits/read`。優先解析 `rateLimitsByLimitId`，空或不存在時回退 `rateLimits`；primary / secondary 視窗可選。

剩餘量由 `100 - usedPercent` 計算，視窗長度與重設時間沿用回應。API-key-only 或沒有 quota 的回應顯示無法提供，不以 token 用量反推。

### Antigravity

固定命令：

```text
agy.exe --print /usage --output-format json --print-timeout 15s
```

只解析 `command.data.groups[].buckets[]`，將 `remaining_fraction` 轉為百分比，保留群組、視窗與重設時間；不從模型自然語言回答擷取數字。

### Claude Code

讀取 `%USERPROFILE%\.claude\.credentials.json`（或 `CLAUDE_CONFIG_DIR` 指定目錄）中 Claude Code 的登入 token，以 HTTPS 請求 `https://api.anthropic.com/api/oauth/usage`（帶 `anthropic-beta: oauth-2025-04-20`）。解析 `five_hour`、`seven_day`、`seven_day_opus`、`seven_day_sonnet` 的 `utilization`，剩餘量為 `100 - utilization`，`resets_at` 為重設時間。

token 只存在記憶體，不寫入設定、不記錄，也不自行刷新；登入過期時提示重新開啟 Claude Code。此端點為非公開介面，可能隨版本變動；回應無法辨識時顯示狀態，不以 session 用量或成本推估。

## 設定與本機資料

設定存於 Tauri app data 目錄的 `settings.json`。Windows 通常是 `%APPDATA%\com.aiquotamini.widget\settings.json`，實際位置由 Tauri 路徑 API 決定。

```json
{
  "codexEnabled": true,
  "antigravityEnabled": true,
  "claudeCodeEnabled": false,
  "pollingIntervalSeconds": 300,
  "alwaysOnTop": true,
  "displayMode": "bar"
}
```

`displayMode` 為 `bar` 或 `gauge`。舊設定缺少此欄位或未知模式時回退長條，保留其他設定。保存先寫暫存檔再替換；Windows 使用 `MoveFileExW` replacement，失敗時保留舊檔。保存失敗也不清除前端草稿。

程式沿用 CLI 登入狀態，不執行登入 / 登出、不保存 API key 或 token、不修改 gateway；Claude 查詢會讀取 Claude Code 的本機憑證並僅對 api.anthropic.com 發出請求。CLI 參數固定，表單不接受 shell 指令。查詢配額仍依賴各 CLI 與服務連線。

## 原始碼結構

```text
src/
  main.ts                 UI 事件、輪詢與 Tauri 溝通
  settings.ts             設定預設值與正規化
  types.ts                Provider / quota / settings 型別
  ui.ts                   長條、錶盤及設定畫面
  ui/gauge.ts             指針幾何與分錶模型
  styles.css              桌面與 SVG 樣式
  *.test.ts               Vitest 測試
src-tauri/
  src/lib.rs              狀態、命令、視窗與系統匣
  src/model.rs            DTO 及設定相容性
  src/settings.rs         設定讀寫與檔案替換
  src/providers/          CLI 查詢及解析
  capabilities/           Tauri 權限
  tauri.conf.json          視窗、CSP 及打包設定
```

## 驗證與限制

本機驗證：14 項 TypeScript、24 項 Rust 測試通過；TypeScript / Vite release build、Rust 格式檢查及 NSIS 打包成功。

測試涵蓋單 / 雙指針、0% / 100%、相同數值、多群組、超過兩視窗、無效數值、舊設定、配額解析與設定檔替換。原生 WebView2 另驗證真實 Codex / Antigravity 資料、指針設定保存、mini 三錶盤完整顯示及 console 無錯誤。

- CLI 格式可能隨版本改變；解析失敗會顯示錯誤或最後成功資料。
- 尚未驗證 macOS / Linux，Windows 為目前交付平台。
- 指針 mini 保留群組資訊，錶盤多時視窗會比長條模式更高。
- 未登入、連線失敗或帳號没有 quota，都可能無法取得數字。
- Claude 端點實測（2026-10-07）：以本機 Claude Code 的 OAuth 憑證請求 `/api/oauth/usage` 成功。`five_hour`、`seven_day` 含 `utilization`（已用百分比）與 `resets_at`（RFC3339，含微秒與時區偏移），與解析一致；該帳號的 `seven_day_opus` / `seven_day_sonnet` 為 `null`，因此不顯示。回應另有 `limits`、`extra_usage` 等未使用欄位，解析時忽略。已依真實格式新增測試。
- Claude 配額的原生視窗（長條與指針兩種模式）尚未在畫面上目視驗證：`tauri dev` 可啟動且無 panic，但此環境無法操作原生視窗。
- Claude 配額使用非公開端點，Anthropic 調整後可能失效；僅支援以 Claude 帳號（Pro / Max）登入的 Claude Code，API key 登入沒有此配額。

本倉庫未指定開源授權條款；公開原始碼不代表另行授予使用或散布授權。

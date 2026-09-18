# Proposal: lan-mouse CLI 加入 VeloKVM（velokvm 子命令與 onboarding）

## 緣起與動機
`lan-mouse` 已內嵌 `velokvm-proto` 實現跨主機 Noise_IK 剪貼簿同步（TCP 9022），且 VeloKVM 另有 fast-path（UDP 9021）。然而「一台新機器要加入 VeloKVM 三機網路」目前只能手動編輯 `config.toml`、手動互換公鑰、手動起停服務——沒有 CLI 流程，也沒有 `velokvm-agent`（Rust 原生 MCP Server）可以自動化的結構化介面。

本變更為 `lan-mouse-cli` 新增**獨立 `velokvm` 一級子命令**（使用者已定案），把「加入 VeloKVM」變成可重複、可驗證、可由 Code Agent 執行的 CLI 流程。

## 目標 (Goals)
1. **`lan-mouse-cli velokvm` 子命令**：
   - `velokvm init`：產生/匯入本機 Curve25519 身份（`identity.key` / `identity.pub`），寫入 daemon 設定。
   - `velokvm peer add <hostname>`：註冊對端（`--pub` 對端公鑰、`--ips`、`--fastpath-port 9021`、`--clipboard-port 9022`），建立 client 並同步 `clipboard_key`。
   - `velokvm peer list` / `peer remove`：檢視與移除 VeloKVM 對端。
   - `velokvm status`：整體狀態（本機身份指紋、各 peer 的 fast-path／clipboard 連線狀態、`--json` 機器可讀輸出）。
   - `velokvm key export` / `key fingerprint`：輸出公鑰／SHA-256 指紋，供「公鑰互換」步驟貼給對端。
2. **`lan-mouse-ipc` 擴充**：
   - `FrontendRequest`：`VelokvmInit`, `VelokvmPeerAdd`, `VelokvmPeerRemove`, `VelokvmGetStatus`。
   - `FrontendEvent`：`VelokvmStatus(VelokvmStatusPayload)`, `VelokvmPeerResult`。
   - 重用既有 `AuthorizeKey` 指紋白名單（4242 UDP 原生通道不變）。
3. **`velokvm-agent` 介接契約**：
   - 所有子命令支援 `--json`、決定性 exit code（0 成功 / 2 服務未跑 / 3 身份金鑰問題 / 4 白名單未授權），供 `velokvm-agent` 的 MCP 工具（`velokvm_onboard`, `velokvm_verify`）直接封裝呼叫。

## 與既有變更的界線（不重複）
- **`lan-mouse-cli-clipboard-control`**（剪貼簿維運）：`clipboard status/enable/disable/test-push` 維持不變；`clipboard set-key` 的**金鑰來源主導權**移交給本變更的 `velokvm peer add --pub`（`clipboard-control` 對應任務改標記為 deferred）。
- **fast-path（9021 UDP）**：本變更只做**設定註冊與狀態查詢**；實際注入轉譯屬 `velokvm-host`（VeloKVM repo）範疇，`lan-mouse` 側不實作注入。
- **4242 原生 UDP 通道**：零改動，`lan-mouse` 原有功能完全不變。

## 非目標 (Non-Goals)
- 不實作自動金鑰交換協定（配對仍為「公鑰互換 → `peer add --pub`」兩步；自動化配對屬 VeloKVM `velokvm identity` 範疇）。
- 不實作 tap0 / Solo5 Zero Trust 閘道路徑（商業版 Stage 2）。
- 不變更 Wayland `input-clipboard` OS 介面層。

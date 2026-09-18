# Proposal: lan-mouse CLI & IPC 剪貼簿控制支援 (Clipboard Control)

## 緣起與動機
目前 `lan-mouse`（`cawa0505/lan-mouse`）已成功內嵌 `velokvm-proto` 實現跨主機 0-RTT Noise_IK 剪貼簿同步。然而在管理與維護維度上存在缺口：
1. **無狀態可見度**：`lan-mouse-cli` 與 IPC 協定（`lan-mouse-ipc`）目前只支援 client 的位置、IP、指紋授權管理，完全缺乏剪貼簿連線狀態、傳輸計數、最新同步時間與錯誤診斷查詢。
2. **無即時控制能力**：無法透過 CLI 即時針對特定節點啟用/停用剪貼簿同步、強制觸發單次推送測試、或更新對端的 Noise_IK 公鑰。
3. **缺少 AI Code Agent 自動化介面**：後續 `velokvm-agent`（Rust 原生 MCP Server）需要具備明確、結構化的 CLI/IPC 接口，才能為 Claude Code、Cursor 等 Agent 提供可靠的剪貼簿健康診斷、單端測試與設定注入能力。

## 目標 (Goals)
1. **擴展 `lan-mouse-ipc`**：
   - 增加 `ClipboardStatus` 查詢請求與事件回應（連線狀態、累計傳輸次數、最後傳輸時間、錯誤記錄）。
   - 增加客戶端等級的剪貼簿開關（`SetClipboardEnabled { id, enabled }`）與金鑰配置（`SetClipboardKey { id, key }`）。
   - 增加即時手動推送測試指令（`TriggerClipboardPush { id, text }`）。
2. **擴展 `lan-mouse-cli`**：
   - 提供 `lan-mouse-cli clipboard status`（支援純文字與 `--json` 機器可讀輸出）。
   - 提供 `lan-mouse-cli clipboard enable <id>` / `disable <id>`。
   - 提供 `lan-mouse-cli clipboard test-push <id> --text "..."` 供 CI/Agent 快速驗證。
3. **為 `velokvm-agent` 提供標準自動化橋樑**：
   - 確保 CLI 指令具有決定性 exit code 與 JSON schema，供 MCP 工具直接呼叫或封裝。

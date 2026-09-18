# spec.md: lan-mouse-clipboard-cli

## ADDED Requirements

### Requirement: IPC 協定剪貼簿狀態查詢與控制
`lan-mouse-ipc` MUST 定義標準的剪貼簿狀態查詢與個別用戶端控制請求與事件。

#### Scenario: 查詢剪貼簿整體與用戶端狀態
- **GIVEN** lan-mouse daemon 正常運行且已啟用剪貼簿同步
- **WHEN** 前端或 CLI 發送 `FrontendRequest::ClipboardGetStatus`
- **THEN** daemon 必須回應 `FrontendEvent::ClipboardStatus(ClipboardStatusPayload)`
- **AND** payload 必須包含本機監聽埠（9022）、本機公鑰以及各配置用戶端的同步計數與狀態

#### Scenario: 動態開關指定用戶端剪貼簿功能
- **GIVEN** 用戶端 ID 為 1 的主機正在進行剪貼簿同步
- **WHEN** 發送 `FrontendRequest::ClipboardSetEnabled { id: 1, enabled: false }`
- **THEN** daemon 必須停用該用戶端的剪貼簿同步並在後續狀態查詢中反映 `enabled: false`

### Requirement: CLI 剪貼簿子命令
`lan-mouse-cli` MUST 提供 `clipboard` 子命令以支援狀態檢查與除錯推送。

#### Scenario: 查詢狀態並以 JSON 輸出
- **GIVEN** daemon 正在運作
- **WHEN** 執行 `lan-mouse-cli clipboard status --json`
- **THEN** 指令必須輸出合規的 JSON 物件並以 exit code 0 結束

#### Scenario: 執行手動測試推送
- **GIVEN** 目標用戶端處於連線中
- **WHEN** 執行 `lan-mouse-cli clipboard test-push 1 --text "probe test"`
- **THEN** 指令必須發起 TCP 9022 Noise_IK 連線並回傳推送成功訊息或延遲指標

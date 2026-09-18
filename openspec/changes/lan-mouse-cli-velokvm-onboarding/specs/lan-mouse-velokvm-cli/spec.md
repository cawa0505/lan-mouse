# spec.md: lan-mouse-velokvm-cli

## ADDED Requirements

### Requirement: 本機 VeloKVM 身份初始化
`lan-mouse-cli velokvm init` MUST 能產生或匯入本機 Curve25519 身份金鑰，並使 daemon 進入可加入 VeloKVM 網路的狀態。

#### Scenario: 全新產生身份
- **GIVEN** 本機尚無 VeloKVM 身份
- **WHEN** 執行 `lan-mouse-cli velokvm init`
- **THEN** daemon 必須產生 Curve25519 keypair，私鑰以 0600 權限存於 `~/.config/lan-mouse/velokvm/identity.key`
- **AND** CLI 輸出本機公鑰 SHA-256 指紋並以 exit code 0 結束

#### Scenario: 身份已存在時冪等保護
- **GIVEN** 本機已有 VeloKVM 身份
- **WHEN** 再次執行 `lan-mouse-cli velokvm init`（未帶 `--force`）
- **THEN** daemon 必須拒絕覆寫並回報既有身份指紋，exit code 非零

### Requirement: VeloKVM 對端註冊
`lan-mouse-cli velokvm peer add` MUST 能以對端公鑰與位址註冊新 peer，同時建立 lan-mouse client 並寫入剪貼簿公鑰。

#### Scenario: 註冊新對端
- **GIVEN** 本機已完成 `velokvm init` 且 daemon 運行中
- **WHEN** 執行 `lan-mouse-cli velokvm peer add megatron --pub <64hex> --ips 192.168.77.75`
- **THEN** daemon 必須建立對應 client（`clipboard_key` 寫入該公鑰、clipboard 啟用）
- **AND** `velokvm peer list` 必須列出 megatron 且 exit code 0

#### Scenario: 公鑰格式驗證 fail-closed
- **GIVEN** 使用者提供非 64 hex 或全零公鑰
- **WHEN** 執行 `velokvm peer add` 帶該公鑰
- **THEN** 指令必須以 exit code 5 拒絕，且不產生任何 client

### Requirement: VeloKVM 狀態查詢
`lan-mouse-cli velokvm status` MUST 回報本機身份指紋、通道埠與各 peer 的 fast-path／剪貼簿連線狀態。

#### Scenario: JSON 機器可讀輸出
- **GIVEN** daemon 運行中且已註冊至少一個 peer
- **WHEN** 執行 `lan-mouse-cli velokvm status --json`
- **THEN** 輸出必須為合規 JSON（含 `identity_fingerprint`、`fastpath_port`、`clipboard_port`、`peers[]`）且 exit code 0

#### Scenario: daemon 未運行
- **GIVEN** lan-mouse daemon 未啟動
- **WHEN** 執行任一 `velokvm` 子命令
- **THEN** 指令必須以 exit code 2 結束並提示服務未運行

### Requirement: 公鑰互換輔助
`lan-mouse-cli velokvm key` MUST 提供公鑰輸出與指紋顯示，支援帶外公鑰互換流程。

#### Scenario: 輸出本機公鑰指紋
- **GIVEN** 本機已完成 `velokvm init`
- **WHEN** 執行 `lan-mouse-cli velokvm key fingerprint`
- **THEN** 輸出 64 字元 SHA-256 hex 指紋且 exit code 0

### Requirement: Agent 自動化契約
所有 `velokvm` 子命令 MUST 支援 `--json` 與決定性 exit code，供 `velokvm-agent` MCP 工具直接封裝。

#### Scenario: 錯誤碼語意穩定
- **GIVEN** 任一 `velokvm` 子命令失敗
- **WHEN** 以 `--json` 模式執行
- **THEN** stdout 必須輸出 `{"error": "...", "code": N}`，且 N 符合契約（2=服務未跑、3=身份金鑰、4=未授權、5=參數錯誤）

# Design: lan-mouse CLI & IPC 剪貼簿控制與 velokvm-agent 介接架構

## 1. 系統架構定位與分層

```
+-------------------------------------------------------------------------------+
|                      AI Code Agent / Human Developer                          |
|             (Claude Code, Cursor, Windsurf, Codex, Terminal)                  |
+---------------------------------------+---------------------------------------+
                                        |
                 MCP (Model Context Protocol) via stdio
                                        v
+-------------------------------------------------------------------------------+
|                             velokvm-agent (Rust)                              |
|  - Invariants Engine (velokvm.jsonc)                                          |
|  - MCP Tool Server: verify_clipboard, check_channel, configure_peer           |
+---------------------------------------+---------------------------------------+
                                        |
              Local Unix Domain Socket (IPC) / CLI Exec
                                        v
+-------------------------------------------------------------------------------+
|                          lan-mouse-cli / lan-mouse-ipc                        |
|  - CLI Commands: clipboard status, test-push, enable, disable                 |
|  - JSON-RPC over Unix Socket (~/.local/share/lan-mouse/lan-mouse.sock)        |
+---------------------------------------+---------------------------------------+
                                        |
                 IPC Internal Channel (FrontendRequest/Event)
                                        v
+-------------------------------------------------------------------------------+
|                           lan-mouse Core Daemon                               |
|  - Watcher Thread (Local wl-clipboard / ext-data-control-v1)                  |
|  - Responder Thread (TCP 9022 Noise_IK Listener)                              |
|  - ClipboardStats & ChannelManager (Real-time telemetry)                      |
+-------------------------------------------------------------------------------+
```

## 2. IPC 訊息協議設計 (`lan-mouse-ipc`)

### 2.1 擴展 `FrontendRequest`
```rust
pub enum FrontendRequest {
    // 既有項目...
    
    /// 查詢剪貼簿整體與各 client 狀態
    ClipboardGetStatus,
    /// 針對指定 client 啟用/停用剪貼簿同步
    ClipboardSetEnabled { id: ClientHandle, enabled: bool },
    /// 更新指定 client 的 Noise_IK 剪貼簿公鑰 (hex)
    /// deferred：主導權移交 `velokvm peer add --pub`（lan-mouse-cli-velokvm-onboarding），
    /// 金鑰單一寫入路徑；本變更不實作此變體
    // ClipboardSetKey { id: ClientHandle, key: String },
    /// 觸發對特定 client 的手動測試推送 (文本內容)
    ClipboardTriggerPush { id: ClientHandle, text: String },
}
```

### 2.2 擴展 `FrontendEvent`
```rust
pub enum FrontendEvent {
    // 既有項目...

    /// 剪貼簿狀態回傳
    ClipboardStatus(ClipboardStatusPayload),
    /// 單次測試推送結果
    ClipboardPushResult {
        id: ClientHandle,
        success: bool,
        bytes: usize,
        latency_ms: Option<u64>,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardStatusPayload {
    pub enabled: bool,
    pub listen_port: u16,
    pub local_pubkey: String,
    pub clients: Vec<ClientClipboardStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientClipboardStatus {
    pub id: ClientHandle,
    pub hostname: Option<String>,
    pub enabled: bool,
    pub peer_pubkey: Option<String>,
    pub total_sent: u64,
    pub total_received: u64,
    pub last_sync_timestamp: Option<u64>,
    pub last_error: Option<String>,
}
```

## 3. CLI 子命令設計 (`lan-mouse-cli`)

CLI 子命令設計為結構化、可組合，並預設支援 `--json` 格式，方便 `velokvm-agent` 與腳本直接消費：

```bash
# 1. 查詢狀態 (人類可讀 / JSON)
lan-mouse-cli clipboard status
lan-mouse-cli clipboard status --json

# 2. 開關個別節點
lan-mouse-cli clipboard enable <client_id>
lan-mouse-cli clipboard disable <client_id>

# 3. 配置對端公鑰 — deferred：主導權移交 `velokvm peer add --pub`
#    （見 lan-mouse-cli-velokvm-onboarding；金鑰單一寫入路徑）

# 4. 手動測試推送 (驗證連線與傳輸延遲)
lan-mouse-cli clipboard test-push <client_id> --text "Agent probe ping" [--json]
```

## 4. `velokvm-agent` 介接原則 (MCP Tools)

`velokvm-agent` 啟動時，會根據 `velokvm.jsonc` 的宣告註冊下列 MCP 工具：

1. `velokvm_clipboard_status`：
   - 呼叫 `lan-mouse-cli clipboard status --json`，獲得所有配對主機的同步狀態。
2. `velokvm_clipboard_test_push`：
   - 參數：`target_host`（例如 `megatron` 或 client ID）、`payload`。
   - 呼叫 CLI 執行測試推送，回傳往返延遲（ms）與接收確認。
3. `velokvm_clipboard_toggle`：
   - 參數：`target_host`、`enabled`（bool）。
   - 動態調整特定環境的剪貼簿連線。

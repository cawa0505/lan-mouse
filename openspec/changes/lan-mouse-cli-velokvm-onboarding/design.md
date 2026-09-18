# Design: lan-mouse CLI velokvm 子命令與 onboarding 架構

## 1. 分層與職責

```
+-------------------------------------------------------------------------------+
|                  AI Code Agent / Human Developer                              |
|          (Claude Code, Cursor, velokvm-agent MCP tools)                       |
+---------------------------------------+---------------------------------------+
                                        |  CLI exec (--json, exit codes)
                                        v
+-------------------------------------------------------------------------------+
|                    lan-mouse-cli :: velokvm 子命令                             |
|  init / peer add|list|remove / status / key export|fingerprint                |
+---------------------------------------+---------------------------------------+
                                        |  FrontendRequest/Event (Unix socket)
                                        v
+-------------------------------------------------------------------------------+
|                         lan-mouse Core Daemon                                 |
|  - VelokvmManager：身份金鑰、peer 註冊表、fast-path/clipboard 通道狀態          |
|  - 既有：Clipboard Watcher/Responder (TCP 9022)、原生 4242 UDP 不變            |
+-------------------------------------------------------------------------------+
                                        |  velokvm-proto (Noise_IK)
                                        v
+-------------------------------------------------------------------------------+
|            VeloKVM Transport（fast-path UDP 9021 / clipboard TCP 9022）        |
+-------------------------------------------------------------------------------+
```

職責邊界（與 ARCHITECTURE 5.8 / HANDOFF #14 一致）：
- `lan-mouse`：OS 介面（Wayland 擷取/注入/剪貼簿）＋ **本變更新增的 onboarding 控制面**（身份、peer 註冊、狀態查詢）。
- `VeloKVM`：wire 協議與傳輸引擎（`velokvm-proto`）；`velokvm-host` 負責注入。lan-mouse 側**不**實作注入。

## 2. IPC 訊息設計（`lan-mouse-ipc`）

### 2.1 `FrontendRequest` 擴充
```rust
/// 初始化/匯入本機 VeloKVM 身份（Curve25519）
VelokvmInit {
    /// 既有私鑰檔路徑（hex 64 字元）；None = 現場產生
    import_key_path: Option<String>,
},
/// 註冊 VeloKVM 對端（同時建立 lan-mouse client 並寫入 clipboard_key）
VelokvmPeerAdd {
    hostname: String,
    pubkey_hex: String,          // 對端靜態公鑰（64 hex）
    ips: Vec<IpAddr>,
    fastpath_port: u16,          // 預設 9021
    clipboard_port: u16,         // 預設 9022
},
VelokvmPeerRemove { hostname: String },
VelokvmGetStatus,
```

### 2.2 `FrontendEvent` 擴充
```rust
VelokvmStatus(VelokvmStatusPayload),
VelokvmPeerResult { hostname: String, client_id: ClientHandle, success: bool, error: Option<String> },
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelokvmStatusPayload {
    pub identity_fingerprint: Option<String>,   // 本機公鑰 SHA-256；未 init 為 None
    pub fastpath_port: u16,                     // 9021
    pub clipboard_port: u16,                    // 9022
    pub peers: Vec<VelokvmPeerStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelokvmPeerStatus {
    pub hostname: String,
    pub client_id: ClientHandle,
    pub pubkey_fingerprint: Option<String>,
    pub fastpath_alive: bool,        // 既有 ClientState.alive
    pub clipboard_ok: bool,          // 既有 clipboard 狀態
    pub last_error: Option<String>,
}
```

設計原則：
- **重用既有結構**：peer 狀態直接映射既有 `ClientHandle`/`ClientState.alive`，不另起爐灶；`clipboard_key` 沿用 `ClientConfig.clipboard_key` 欄位（已存在）。
- **金鑰單一來源**：`velokvm peer add --pub` 是唯一寫入 `clipboard_key` 的路徑；`clipboard-control` 的 `set-key` 降級為 deferred（避免兩條寫入路徑漂移）。
- **向前相容**：所有新 enum 變體帶 `#[serde(default)]`/Option 欄位，舊 frontend 收到未知變體走既有 `Error(String)` 路徑，不 panic。

## 3. CLI 子命令設計（`lan-mouse-cli`）

```bash
# 身份
lan-mouse-cli velokvm init                          # 產生 identity，印指紋
lan-mouse-cli velokvm init --import ~/id.key        # 匯入既有私鑰
lan-mouse-cli velokvm key fingerprint               # 本機公鑰 SHA-256（貼給對端）
lan-mouse-cli velokvm key export                    # 輸出公鑰 hex

# 對端（加入 VeloKVM 網路）
lan-mouse-cli velokvm peer add megatron \
    --pub 862552a5bf61a3b1... \
    --ips 192.168.77.75 \
    [--fastpath-port 9021] [--clipboard-port 9022]
lan-mouse-cli velokvm peer list [--json]
lan-mouse-cli velokvm peer remove megatron

# 狀態
lan-mouse-cli velokvm status [--json]
```

## 4. Onboarding 流程（三機標準劇本）

```
節點 A（新機器）                        節點 B（已加入）
─────────────────                      ─────────────────
1. velokvm init
2. velokvm key fingerprint  ──(帶外傳遞)──▶  記錄 A 的公鑰
3.                                       velokvm peer add a-host --pub <A.pub> --ips <A.ip>
4. velokvm peer add b-host ──◀──(帶外傳遞)──  B 的公鑰 + IP
5. velokvm status --json                 velokvm status --json
   （雙向 fastpath_alive / clipboard_ok 全 true = 加入完成）
```

`velokvm-agent` 自動化：步驟 2→3→4→5 全部有 `--json` 與決定性 exit code，Agent 可代跑並以 `status` 驗證收斂；「帶外傳遞」可由使用者貼上或 Agent 經既有授權管道取得。

## 5. Exit Code 契約（Agent 自動化基礎）

| code | 意義 |
|------|------|
| 0 | 成功 |
| 2 | daemon 未運行（`ServiceNotRunning`） |
| 3 | 身份金鑰問題（未 init、金鑰格式錯誤、匯入檔不存在） |
| 4 | 白名單未授權／對端拒絕 |
| 5 | 參數錯誤（hex 長度、IP 格式） |

`--json` 模式下錯誤以 `{"error": "...", "code": N}` 輸出至 stdout，人類模式輸出至 stderr。

## 6. 安全與失敗模式

- 私鑰檔權限 `0600`，路徑遵循 XDG（`~/.config/lan-mouse/velokvm/identity.key`）。
- `peer add` 對 `pubkey_hex` 做嚴格驗證（64 hex、非零、非本機自身公鑰）——信任邊界輸入驗證不可省。
- daemon 端 `VelokvmInit` 在身份已存在時必須拒絕覆寫（冪等保護），除非明確 `--force`。
- 原生 4242 通道與既有授權指紋白名單零改動；本變更所有失敗皆 fail-closed。

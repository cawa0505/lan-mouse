# Clipboard Support — Design（OS 介面技術規格）

## 定位

OS 介面層：只做本機剪貼簿擷取／寫入。**不含**網路、加密、分塊、壓縮。

## Wayland 協議

| 協議 | 狀態 | 支援 |
|------|------|------|
| `ext-data-control-v1` | staging 標準（**採用**） | Sway 1.11+, Hyprland 0.52+, KWin 6.7+, Labwc, COSMIC, niri |
| `wlr-data-control-unstable-v1` | **deprecated**（不採用） | 舊 wlroots compositor |
| GNOME / Mutter | 未支援 | Phase 1 缺口 |

MIME-agnostic：source 以 `offer()` 廣告型別，receiver 以 `receive(mime_type, fd)` 取資料。selection 擁有者斷線時 compositor 送出 NULL selection。

## 介面契約（草案）

```rust
pub enum Event { Changed { mimes: Vec<String> } }   // mimes 空 = 清空

pub fn watch(cb: impl FnMut(Event));                                        // 監聽變更
pub fn read(mime: &str, max_len: usize) -> io::Result<Box<dyn io::Read>>;   // streaming；超限回錯
pub fn offer(mimes: &[&str], provider: Provider) -> Origin;                 // 寫入，回傳 origin
```

- `Origin`：後續 `Event::Changed` 若源自本次寫入，呼叫端可比對並忽略（**迴圈抑制**必要機制）。機制（序號／寬限窗）由實作決定，契約上必須存在。
- 同步 API；整合端自行包 thread。

## 範圍

- MIME：Phase 1 僅 `text/plain;charset=utf-8`（API 保留 MIME 通用性）
- 大小上限：預設 1 MiB，可組態；超限回錯，**不截斷**

## 傳輸（範圍外）

VeloKVM 負責：專用 TCP + Noise_IK bulk channel（`VeloKVM/openspec/changes/clipboard-sync/`）。本 crate 不引入任何網路依賴。
上層 `lan-mouse` daemon 以 Path B 直接依賴 `velokvm-proto`（`git@gitlab.com:saaslab/velokvm.git`），內嵌 watcher 與 responder thread 處理同步。

- Server port：**9022/tcp**（2026-09-18 決定；VeloKVM 端 spec 需同步此值）
- Wire 協議：u16 big-endian 分框 + `Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s`（0-RTT），進入 transport mode 後傳輸 `ClipboardMsg`（Offer / Chunk / Complete / Abort）。
- 迴圈抑制：跨 session 採 500ms 寫入寬限窗（grace window）避免自發 ping-pong。

## License

GPL-3.0-or-later（跟隨本 repo）。

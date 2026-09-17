# Clipboard Support — Proposal

> 來源：VeloKVM（SDCI）提出之 OS 介面層規格；**本變更由 lan-mouse 實作**。
> 對應 VeloKVM 端傳輸層：`VeloKVM/openspec/changes/clipboard-sync/`（Noise_IK bulk channel，**本變更範圍外**）。

## Why

- README Roadmap 已列 `- Clipboard support`，尚未開工。
- 跨機鍵鼠共享缺剪貼簿即半套體驗；VeloKVM 以 git dependency 消費本 repo 的 `input-event` / `input-capture` / `input-emulation`，同樣需要可重用的 OS 介面 crate。
- 剪貼簿 payload（數 bytes ～ 數 MB）與 1000Hz HID 事件性質不同，需與輸入路徑解耦——與本 repo「控制核心分離為 crate」方向一致。

## What Changes

新增 crate `input-clipboard`（建議名，與 `input-*` 命名一致；最終命名由本 repo 決定）：

- 監聽本機剪貼簿變更（Wayland `ext-data-control-v1`），事件 + MIME 宣告
- streaming 讀取（`receive(mime_type, fd)` 包裝）
- 寫入（offer + provider）並回傳 `Origin` 權杖
- 迴圈抑制：遠端寫入造成的變更不得再回報為新事件

## Decisions

| 項目 | 決定 | 理由 |
|------|------|------|
| Wayland 協議 | `ext-data-control-v1` | freedesktop staging 標準；Sway / Hyprland / KWin / Labwc / COSMIC / niri 已支援 |
| `wlr-data-control-unstable-v1` | 不採用 | 已 deprecated，官方明示改用 ext-data-control-v1 |
| crate 邊界 | 純 OS 介面；零網路／加密依賴；同步 API | 傳輸全歸 VeloKVM；同步 API 由整合端自行包 thread |
| 實作進路 | 直接依賴 `wl-clipboard-rs` 或自建 protocol 綁定（本 repo 決定） | `wl-clipboard-rs` 為 daemon-safe 前例，可最小化維護 |
| License | GPL-3.0-or-later | 跟隨本 repo |

## Non-Goals

- 網路傳輸／加密／壓縮／分塊（VeloKVM `clipboard-sync`）
- X11 / Windows / macOS 後端（後續）
- primary selection（中鍵）
- GTK / CLI 應用殼整合

## Impact

- 新增一個 crate，既有 crate 零改動
- `lan-mouse-proto` 不受影響；wire format 不變（剪貼簿不走 DTLS 通道）

## Open Questions

- GNOME / Mutter 未支援 data-control：Phase 1 接受缺口，或先做 XWayland fallback？
- crate 名稱 `input-clipboard` 是否接受？

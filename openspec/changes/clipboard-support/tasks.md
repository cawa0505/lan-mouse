# Clipboard Support — Tasks

## 1. Crate 骨架

- [x] 新增 `input-clipboard`（GPL-3.0-or-later；零網路依賴；加入 workspace）
- [x] 錯誤型別與 `Origin` 型別

## 2. Wayland 後端（ext-data-control-v1）

- [x] manager 綁定與連線生命周期（含 NULL selection 處理）
- [x] `watch()`：selection 變更事件 + MIME 廣告
- [x] `read(mime, max_len)`：`receive(mime_type, fd)` streaming 包裝 + 1 MiB 上限
- [x] `offer()` + provider + `Origin` 權杖

## 3. 迴圈抑制

- [x] origin 比對 + 寫入後寬限窗（遠端寫入不得回報為新變更）

## 4. 驗證

- [x] build success（`cargo build/test/clippy --workspace` 全綠；VeloKVM Server 未就緒，live roundtrip 延後）
- [x] live roundtrip（niri 26.04 實測通過 2026-09-18）：offer → Changed → read 內容一致；需修 event_created_child（DataOffer 建 child object）+ read 回傳 `+ Send`（讀取須與 dispatch pump 並行，單執行緒死鎖）。megatron（mango/wlroots）待測
- [x] wire 傳輸互通性初測（2026-09-18）：cybertron ↔ megatron、cybertron ↔ arhat、megatron → arhat 全綠，未授權 key fail-closed 驗證通過
- [x] daemon 整合（Step 2 / Path B）：`lan-mouse` 內嵌 `[clipboard]` 支援，依賴 `velokvm-proto`，提供雙 thread watcher/responder 與 cross-session grace 迴圈抑制
- [ ] 手動矩陣：Sway / Hyprland / KWin 6.7+
- [ ] README Roadmap 勾選 `Clipboard support`

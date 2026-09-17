# Clipboard OS Interface Specification

## ADDED Requirements

### Requirement: 剪貼簿變更通知
系統 SHALL 透過 Wayland `ext-data-control-v1` 監聽本機 selection 變更，並以事件回報可用的 MIME 型別清單。

#### Scenario: 本機複製觸發事件
- **WHEN** 使用者在本機應用程式複製文字（`text/plain`）
- **THEN** `watch` 回呼收到 `Event::Changed { mimes }`，且 `mimes` 含 `text/plain;charset=utf-8`

#### Scenario: selection 擁有者斷線
- **WHEN** 原 selection 擁有者程式結束（compositor 送出 NULL selection）
- **THEN** `watch` 回呼收到 `Event::Changed { mimes: [] }`（清空語意）

### Requirement: 串流讀取
系統 SHALL 以 streaming 介面讀取指定 MIME 的剪貼簿內容，且 SHALL 在超過大小上限時回傳錯誤而非截斷。

#### Scenario: 讀取純文字
- **WHEN** 呼叫 `read("text/plain;charset=utf-8", 1 MiB)` 且來源內容為 2 KiB
- **THEN** 回傳可讀取完整 2 KiB 的 `Read` 串流

#### Scenario: 超出大小上限
- **WHEN** 來源內容為 2 MiB 且 `max_len` 為 1 MiB
- **THEN** 回傳錯誤，且 MUST NOT 回傳截斷後的內容

### Requirement: 寫入與來源識別
系統 SHALL 支援以指定 MIME 內容寫入本機剪貼簿並成為 selection source，且 SHALL 回傳可用於迴圈抑制的來源識別。

#### Scenario: 寫入遠端內容
- **WHEN** 呼叫 `offer(&["text/plain;charset=utf-8"], provider)`
- **THEN** 本機其他應用程式可貼上該內容，且回傳 `Origin`

#### Scenario: 迴圈抑制
- **WHEN** 以 `offer` 寫入後，`watch` 因該次寫入而收到變更事件
- **THEN** 呼叫端能以 `Origin` 識別並忽略，不將其再轉發遠端（無同步迴圈）

### Requirement: 協議選擇
系統 SHALL 使用 `ext-data-control-v1`；MUST NOT 依賴已 deprecated 的 `wlr-data-control-unstable-v1`。

#### Scenario: 支援的 compositor
- **WHEN** 於 Sway 1.11+ / Hyprland 0.52+ / KWin 6.7+ 執行
- **THEN** 剪貼簿讀寫功能正常

#### Scenario: 未支援的 compositor
- **WHEN** 於 GNOME / Mutter 執行（未支援 data-control 協議）
- **THEN** 回報明確錯誤，不靜默失敗

### Requirement: 邊界與依賴
crate SHALL NOT 引入任何網路、加密、壓縮或分塊依賴；Phase 1 SHALL NOT 支援 X11 / Windows / macOS 或 primary selection。

#### Scenario: 依賴邊界
- **WHEN** 檢查 crate 依賴樹
- **THEN** 無網路／加密相關依賴

#### Scenario: Phase 1 MIME 範圍
- **WHEN** 處理非文字 MIME（如 `image/png`）
- **THEN** 明確回報不支援（介面保留 MIME 通用性，後續階段再開）

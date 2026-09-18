# Tasks: lan-mouse CLI & IPC 剪貼簿控制支援

## 1. IPC 協定層 (`lan-mouse-ipc`)
- [ ] 1.1 在 `FrontendRequest` 增加 `ClipboardGetStatus`, `ClipboardSetEnabled`, `ClipboardSetKey`, `ClipboardTriggerPush` 訊息定義。
- [ ] 1.2 在 `FrontendEvent` 增加 `ClipboardStatus(ClipboardStatusPayload)` 與 `ClipboardPushResult` 事件定義。
- [ ] 1.3 確保所有新增型別支援 `Serialize` 與 `Deserialize`，並保持向前相容。

## 2. Daemon 核心狀態管理 (`lan-mouse/src/clipboard.rs` & `server.rs`)
- [ ] 2.1 實作執行時剪貼簿狀態結構體（`ClipboardStats`），記錄各 peer 之發送次數、接收次數、最後活動時間戳記與錯誤字串。
- [ ] 2.2 在 IPC 處理迴圈中響應 `ClipboardGetStatus` 並回傳當前即時統計數據。
- [ ] 2.3 實作 `ClipboardSetEnabled`，允許在不重新啟動 daemon 的情況下動態開關特定客戶端的剪貼簿同步。
- [ ] 2.4 實作 `ClipboardTriggerPush`，主動對目標 client 的 TCP 9022 發起單次 Noise_IK 握手並回報延遲指標。

## 3. CLI 命令列工具 (`lan-mouse-cli`)
- [ ] 3.1 增加 `Clipboard` 子命令列（`status`, `enable`, `disable`, `set-key`, `test-push`）。
- [ ] 3.2 實作格式化表格輸出與 `--json` 結構化 JSON 輸出（供 Agent 與自動化腳本使用）。
- [ ] 3.3 實作適當的 Exit Code（連線成功為 0，對端離線或金鑰不匹配回傳特定錯誤碼）。

## 4. 驗證與自動化測試
- [ ] 4.1 單元測試：驗證 IPC 訊息序列化與反序列化。
- [ ] 4.2 整合測試：透過 `lan-mouse-cli clipboard status --json` 檢驗 arhat ↔ cybertron ↔ megatron 連線狀態。
- [ ] 4.3 端到端測試：透過 `test-push` 驗證單次傳輸並記錄往返延遲。

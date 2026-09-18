# Tasks: lan-mouse-cli-velokvm-onboarding

## 1. IPC（`lan-mouse-ipc`）

- [ ] 1.1 `FrontendRequest` 新增 `VelokvmInit`, `VelokvmPeerAdd`, `VelokvmPeerRemove`, `VelokvmGetStatus`
- [ ] 1.2 `FrontendEvent` 新增 `VelokvmStatus(VelokvmStatusPayload)`, `VelokvmPeerResult`
- [ ] 1.3 新增 `VelokvmStatusPayload` / `VelokvmPeerStatus` 結構體（Serialize + Deserialize，向前相容）

## 2. Daemon（`src/`）

- [ ] 2.1 `VelokvmManager`：身份金鑰產生/匯入（XDG 路徑、0600、已存在拒絕覆寫）
- [ ] 2.2 `peer add` 處理：建立 client＋寫入 `clipboard_key`＋公鑰嚴格驗證（64 hex、非零、非本機）
- [ ] 2.3 `peer remove` / `peer list` / `VelokvmGetStatus` 處理（映射既有 ClientState）
- [ ] 2.4 daemon 端 IPC 請求分派接線

## 3. CLI（`lan-mouse-cli`）

- [ ] 3.1 `velokvm` 一級子命令骨架：`init` / `key fingerprint|export` / `peer add|list|remove` / `status`
- [ ] 3.2 `--json` 輸出與 exit code 契約（0/2/3/4/5）
- [ ] 3.3 錯誤訊息文案（人類模式 stderr、JSON 模式 stdout）

## 4. 驗證

- [ ] 4.1 單元測試：金鑰產生/匯入/拒絕覆寫、公鑰驗證 fail-closed
- [ ] 4.2 整合測試：IPC 請求→事件往返（init → peer add → status）
- [ ] 4.3 `cargo test --workspace` / `cargo clippy` / `openspec validate` 全綠
- [ ] 4.4 實地雙機 onboarding 劇本：init → 互換公鑰 → peer add → status（fastpath_alive/clipboard_ok 收斂為 true）

## 5. 文件

- [ ] 5.1 README／DOC.md 補 `velokvm` 子命令章節
- [ ] 5.2 `velokvm.jsonc`／`velokvm-agent` 對接段更新（VeloKVM repo `docs/VELOKVM_AGENT.md`）

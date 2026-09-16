# Upstream Issue Backlog — Linux / 效能 / 傳輸 / 穩定性

> 掃描日期：2026-09-16。來源：feschber/lan-mouse open issues。
> 篩選準則：效能、傳輸、穩定性（不走 UI 軸線）。
> 原始 issue 內容備份在同目錄 `<num>.md`。

## 排程順序

### P1 — #488 DTLS handshake 'Bad Certificate' on CertificateVerify
- **主題**：傳輸層 mutual-TLS 握手被伺服端拒絕（兩台 Arch/Hyprland，0.11.0 與 main `392af44` 都中）
- **為何關注**：我們三台全都跑同一套 webrtc-rs DTLS——這病隨時落到自己頭上；封包已確認是協議層拒絕，非網路問題
- **優勢**：0 comments 沒人認領；雙機環境（arhat/megatron）可重現
- **切入點**：重現 → 縮小 webrtc-dtls CertificateVerify 觸發條件 → 修 / 繞
- 狀態：未開工

### P2 — Registry priority 仲裁（fork 原生，openspec/changes/registry/）
- **主題**：同邊界多機 priority 仲裁 + config 熱重載（ponytail 版：不立 NodeRegistry trait、不加第二套心跳、overlay 延後）
- **為何關注**：使用者主動需求；純邏輯無 UI，落在效能/穩定性熱區；DnsResolver 路徑降級後 #443/#413 自然歸還上游
- **切入點**：config 補 priority 欄位 → 同邊界多 client 時按 priority 排序接管 → notify 熱重載
- 狀態：未開工

### P3 — #478 Hyprland crash: splice os error 109（wlroots emulation）
- **主題**：client 端 wlroots emulation 報 `Too many references: cannot splice (os error 109)`，反覆後拖垮 compositor
- **為何關注**：megatron 同屬 wlroots 系（mango），直接受害候選
- **切入點**：wlroots.rs 的 io error 處理 / fd 與 splice 路徑；先確認錯誤後的 lifecycle 是否乾淨
- 狀態：未開工

### P4 — #472 libei capture 每次裝置/zone 變動重建 portal session（fd 洩漏）
- **主題**：barrier 更新、SeatRemoved/DeviceRemoved 觸發整個 InputCapture session 重建，30 次跨越 = 34 次 ConnectToEIS，fd 洩漏到 session 崩潰
- **為何關注**：上游目前最痛的 Linux issue，影響所有 GNOME 用戶；純上游貢獻（arhat=niri、megatron=mango 不走 libei capture）
- **切入點**：`libei.rs:49`（mutter barrier workaround）、`:558`、`:460` 三個重建點改為 session 重用
- 狀態：未開工

### P4 — #386 libei.rs:601 suspend/resume panic → gnome-shell 崩潰- **主題**：`LibeiInputCapture dropped without being terminated!` assertion 拉崩 gnome-shell
- **為何關注**：與 #472 同區（libei 生命週期），可與 P3 一起研究一起修
- 狀態：未開工

## 擱置（記錄在案，非本輪目標）

- #443 GUI hostname 多拼 `.local`（dns.rs 牛刀小用，等順手）
- #413 proto label 字元驗證誤判（同上）
- overlay 選單 UI（多裝置同邊界手動選擇器）——先做 registry priority 仲裁（openspec/changes/registry/），UI 之後再說

## 工作方式

每個 issue 配一份 openspec/changes/<slug>/ 文件（proposal.md / tasks.md），順序：#488 → Registry priority 仲裁 → #478 →（#472 + #386 一起）。

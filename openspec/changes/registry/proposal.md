# Registry — 靜態拓撲與節點狀態管理（提議）

> 狀態：草稿。本文件整合使用者提案（2026-09-16）與 ponytail 修正意見。
> 前置：batched-binary-protocol（編碼層已完成，兩者互相獨立）。

## 動機

- 同一螢幕邊界可能有多台機器（如 Bottom 同時有 cybertron 與 megatron），
  現行 config 無優先級概念，selection 行為不可控。
- mDNS 發現對固定 IP homelab 環境是不必要的複雜度；希望靜態配置為主。
- 拓撲變更需重啟 daemon；希望能熱重載。

## 提案原文的重點（保留）

1. 同方向多節點 + priority 仲裁
2. StaticFileRegistry：TOML 拓撲檔 + notify 熱重載
3. 心跳存活狀態（is_online）
4. 同邊界多節點在線時的 layer-shell overlay 選擇器

## ponytail 修正意見（本文件的核心價值）

### 1. 不做 NodeRegistry trait 抽象（原提案 Phase 2）

ClientManager + DnsResolver + config 已經就是 registry：
- `ClientManager` 已維護 handle→(ips, port, position, alive)
- `DnsResolver`（src/dns.rs）已做主機名→IP 與週期性重解析
- 上游 Ping/Pong + watchdog 已是心跳；`alive()` 就是 is_online

再立一個平行的 `NodeRegistry` trait + `InputRouter<R>` 泛型是重新發明
現有管線。**修正：擴充 config 與 ClientManager，不新增 trait。**

### 2. 心跳不做第二套（原提案 3.1）

原提案的 2 秒 UDP/TCP ping 與上游 Ping/Pong 重複。**修正：is_online
直接讀 ClientManager 的既有 alive/watchdog 狀態，零新協定流量。**

### 3. MdnsRegistry（legacy 相容層）刪除（原提案 3 表格）

YAGNI。mDNS 是現有行為，改動時「留著不動」即是相容；不需要為它
建實作類別。若日後要拔掉，刪 module 即可。

### 4. Overlay 選擇器延後（原提案 Phase 4）

有真實價值但屬 UI feature，依賴 GTK layer-shell，工作量大。
先交付 priority 自動仲裁（highest priority wins），overlay 之後
有需要再開 change。

## 採納後的最小範圍（new scope）

1. `config.toml` 的 `[[client]]` 加 `priority` 欄位（Option<u32>，預設 0）
2. 同 position 多 client 時，capture 端 `position_map` 的選取依
   priority（高者先）；現行行為保留為 fallback
3. `notify` 監看 config.toml，變更時 reload clients（沿既有的
   CaptureRequest::Create/Destroy 與 service 訊息流，不新增通道）
4. is_online 由既有 watchdog 狀態呈現（GUI 顯示，無新邏輯）

## 非目標

- 不新增 NodeRegistry trait / InputRouter 泛型
- 不新增第二套心跳協定
- 不做 overlay 選擇器（延後）
- 不在本次拔除 mDNS

## 風險

- 熱重載牽動 capture create/destroy 與 emulation handle 生命週期，
  需小心與 active client 狀態機（WaitingForAck/Sending）互動。
- mango（megatron）非 wlroots 標準層，layer-shell 行為需實測。

## 與原提案的對照表

| 原提案 | 修正後 |
|---|---|
| NodeRegistry trait + 3 實作 | 擴充 config + ClientManager，無新 trait |
| 2s UDP 心跳狀態機 | 沿用 Ping/Pong watchdog |
| MdnsRegistry 相容層 | 刪除（mDNS 本來就沒動） |
| overlay 選擇器 Phase 4 | 延後，priority 自動仲裁先行 |
| 熱重載 + priority | 保留（核心價值） |

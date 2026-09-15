# Batched Binary Protocol（Linux-only wire format 重構）

## 動機

上游 wire format（`lan-mouse-proto`）：每個 UDP datagram 裝**一個** event，固定 21 bytes
（`EventType u8` + `time u32` + payload，motion 為 `f64 dx/dy`）。

對 1000Hz+ 滑鼠：

- 每秒 1000+ 個 datagram，每個 datagram 都要負擔 DTLS 記錄開銷（~40-50B）。
  現況每秒 wire 上 ≈ 21B + overhead；batching 後 32 events/datagram 可將
  overhead 攤提 32 倍 — 這是主要收益，payload 縮小是次要收益。
- `time: u32` 在 wire 上無用：所有模擬端 backend 皆可自行取時
  （wlroots 已在 `consume_event` 計算 `now`；libei/x11/portal 忽略 time）。
- `f64` 對 evdev 相對位移過剩：真實硬體單 tick 位移幾乎都在 i8 範圍。

上游調查結論（2026-09）：feschber/lan-mouse 對協定重構**沒有任何表態**
（無 roadmap、無 issue/PR/discussion 提及）。上游先例：

- PR #178：`lan-mouse-proto` 獨立成 crate → 上游預期 wire format 穩定
- commit 72c86c0：`Hello { commit: [u8;8] }` soft-warn，舊 peer 收到未知
  event type 走 `InvalidEventId` 靜默略過 → 本 fork 的新格式與舊 peer 雙向不衝突

本變更為 **fork-only**，不考慮 Windows/macOS。

## 變更內容

新增 batched datagram 格式，僅承載熱路徑輸入事件（motion / button / key / wheel）。
控制面事件（Enter / Leave / Ack / Ping / Pong / Hello / Modifiers）維持上游
legacy 單事件 datagram 不動。

- Header 4B：`magic 0x4C` + `event_count u8` + `seq u16`（LE）
- Event 首字節：tag（bit 7..6）+ subtype（bit 5..0）
- `magic 0x4C = 76` 與 legacy `EventType`（0..=11）不重疊，接收端按首字節分流
- wire 上不再有 `time`；接收端自行蓋時間戳
- 接收端每個 datagram 只呼叫一次 `frame()`（現況為每 event 一次）

## 影響範圍

| 檔案 | 變更 |
|---|---|
| `lan-mouse-proto/src/lib.rs`（或新增 `batch.rs`） | 編碼/解碼 |
| `src/capture.rs` | 事件聚合、flush 觸發 |
| `src/connect.rs` | 送出路徑改 batch |
| `src/listen.rs` / `src/emulation.rs` | 解 batch、dispatch |
| `input-emulation/src/wlroots.rs` | frame() 每 datagram 一次 |

不動：`input-event` 抽象、DTLS 層、GTK/CLI、控制面協定。

## 風險

- batch 內按鍵事件隨 datagram 遺失 → 短暫卡鍵/卡鍵帽；由既有
  disconnect-release 機制兜底（`terminate()` 釋放 pressed keys）
- 與上游未來變更脫鉤：新格式有 magic 即版本辨識，v2 換 magic 值

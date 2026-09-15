# Batched Binary Protocol — Wire Format Spec

## Datagram 佈局（batched datagram）

```
+-----------------+-----------------+----------------------+
| magic (u8)      | event_count (u8)| seq (u16, LE)        |  Header 4B
| 0x4C            | 1..=64          | 遞增                  |
+-----------------+-----------------+----------------------+
| event 1 (3B or 5B)                                     |
| ...                                                    |
+--------------------------------------------------------+
```

- 最大 datagram：4 + 64×5 = 324B，遠低於 MTU 1500
- 解碼以 datagram 實際長度為邊界；truncated → 丟棄整個 datagram，log warn

## Event 編碼

Byte 0 定義：`[tag: 2 bits | subtype: 6 bits]`

| Byte 0 | 類型 | 長度 | 格式 |
|---|---|---|---|
| `0b00_000000` | Compact Motion | 3B | `dx: i8`, `dy: i8` |
| `0b01_000000` | Extended Motion | 5B | `dx: i16 LE`, `dy: i16 LE` |
| `0b10_000000` | Button | 3B | `code: u16 LE`, `state: u8`（0=Release, 1=Press） |
| `0b10_000001` | Key | 3B | `code: u16 LE`, `state: u8` |
| `0b11_000000` | Wheel | 3B | `axis: u8`（0=Vertical, 1=Horizontal）, `ticks: i8` |

- Motion 編碼規則：`dx, dy` 皆在 i8 範圍 → Compact；否則 Extended。解碼端
  對 `0b00`/`0b01` 的 subtype 位元不為 0 視為格式錯誤 → 丟棄 datagram
- Wheel `ticks` 以 1/120 刻度為單位（對齊 evdev `REL_WHEEL_HI_RES` 的
  120 基數與現行 `AxisDiscrete120` 語義），接收端映射 `axis_discrete120`
- **wire 上無 time**：接收端自行取時（wlroots `consume_event` 已有 `now`；
  libei/x11/portal 本來就忽略 time）

## 序列號（seq）

- 發送端每送出一個 batched datagram `seq = seq.wrapping_add(1)`
- 接收端 staleness 判斷（wrap-safe）：
  `seq.wrapping_sub(last_seq) as u16` ∈ `1..=0x7FFF` → 接受並更新；
  否則（=0 重複或 >0x7FFF 落後）丟棄。首個 datagram 無條件接受
- 僅用於亂序/重播丟棄，**無重傳**；batch 內事件遺失即遺失

## 發送端 flush 政策（src/capture.rs）

- 批次緩衝：預配置 `[u8; 324]`，零 allocation
- 觸發 flush：
  1. 佇列達 `MAX_BATCH_EVENTS = 64`
  2. 任何非 Motion 事件進入時：先 flush 累積的 motion，再追加該事件
     （保證 click/wheel 前的位移先送達，順序不反轉）
  3. `release_capture()` 前 flush（key-up/Leave 走 legacy 路徑）
  4. 週期 timer（預設 4ms，可調）— 兜底，避免位移事件滯留
- Motion coalescing（待討論，預設開啟）：相鄰 Motion 事件以
  `i32` 累加器合併，flush 時一次編碼（放不下 i8/i16 → Extended）

## 接收端

- 首字節分流：`0x4C` → batch 解碼；`0..=11` → legacy 單事件；
  其他 → 丟棄。`0x4C` = 76 與 legacy `EventType`（0..=11）不重疊
- 每個 batched datagram 解碼後依序 dispatch，**只呼叫一次 `frame()`**
  （wlroots.rs 現況為每 event 一次）
- 按鍵/鍵盤事件遺失由既有 disconnect-release 機制兜底

## Config 開關

- `batched_protocol`（toml，`Option<bool>`，預設 `true`）：仿
  `emulation_backend` 模式（config.rs:463 getter）
- 同一 flag 控制雙向：關閉時發送端全走 legacy 單事件 datagram，
  接收端跳過 batch 分流（首字節分流邏輯保留，僅不會收到 `0x4C`）
- 用途：與上游原版 peer 互連時的逃生門；不做 Hello commit 自動偵測

## 相容性

- 雙方都須跑本 fork：舊 peer 收到 batched datagram 走 `InvalidEventId`
  靜默略過（上游 72c86c0 先例）→ **舊 peer 的游標會不動**（輸入被丟），
  由既有的 `Hello { commit }` soft-warn 介面提示版本不符
- 控制面事件（Enter/Leave/Ack/Ping/Pong/Hello/Modifiers）維持 legacy
  單事件 datagram，不進 batch
- 未來 v2 格式：換 magic 值，同樣走首字節分流

# Batched Binary Protocol — Tasks

## 1. 協定編碼層（lan-mouse-proto）

- [x] 新增 `batch.rs`：`encode_event(buf, event)` / `decode_batch(datagram, last_seq)`，
      零 allocation、純 bit-shift；含 magic/count/seq header 與四種 event tag
- [x] `Motion coalescing` 累加器（i32）與 Compact/Extended 選碼
- [x] 單元測試：round-trip（encode → decode 恆等）、truncated datagram、
      錯誤 subtype、seq wrap/重播丟棄、coalescing 邊界（i8/i16 溢位）

## 2. Config 開關（src/config.rs）

- [ ] `ConfigToml` 加 `batched_protocol: Option<bool>`（預設 `true`）+ getter，
      仿 `emulation_backend`（config.rs:463）；DOC.md / config.toml 註記

## 3. 發送端（src/capture.rs + src/connect.rs）

- [x] 批次緩衝（`[u8; 324]`）與 flush 觸發：滿 64 / 非Motion 事件 / 週期 4ms / release 前
- [x] `State::Sending` 路徑改送 batched datagram；Enter/Leave/Ack/Ping/Pong/Hello 維持 legacy
- [ ] flush 政策單元測試（用既有 dummy backend 模式）

## 4. 接收端（src/listen.rs + src/emulation.rs + input-emulation）

- [x] 首字節分流：`0x4C` → batch 解碼 dispatch；`0..=11` → legacy 單事件
- [x] `Emulation` trait / 各 backend：per-datagram 一次 `frame()`
      （wlroots 為主要目標；libei/x11/portal 忽略 time 本來就不受影響）
- [x] `AxisDiscrete120` 對接 wheel `ticks`（1/120 刻度）

## 5. 驗證

- [ ] `cargo test --workspace`、`cargo fmt`、`cargo clippy --workspace --all-targets --all-features`
- [ ] 實機雙機測試：1000Hz 滑鼠移動平順度、按鍵/滾輪不卡、斷線後 pressed keys 釋放
- [ ] 版本不符情境：fork 對上游（確認 soft-warn 出現、無 panic）
- [ ] DOC.md 更新（wire format 一節）

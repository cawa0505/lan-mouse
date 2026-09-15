# Lan Mouse (cawa0505 fork)

Linux-only fork of feschber/lan-mouse（Software KVM）。

## 目的

上游協定（21-byte 固定 datagram、每 UDP 封包一個 event、`time: u32` + `f64` 欄位）對 1000Hz+ 滑鼠而言頻寬與封包數都過剩。本 fork 只服務 Linux 生態，重新設計 Linux 對 Linux 的輸入事件 wire format，不需跨平台（Windows/macOS）封包適配。

## 技術重點

- Pipeline: `input-capture` → `lan-mouse-ipc` → `input-emulation`，事件經 UDP（DTLS 加密）
- `lan-mouse-proto` 為協定編碼層；`input-event` 為事件抽象（evdev scancode）
- Wayland 模擬端（wlroots backend）使用 `wl_pointer`，flush 週期決定游標平滑度
- 上游版本策略：`Hello { commit }` soft-warn，不拒絕連線；fork 版本間相容性靠此機制提示

## 約定

- 零 allocation：編碼進預先配置的 stack buffer
- 只用 bit-shift 與位元遮罩，不做壓縮
- 變更 wire format 之前先更新 openspec，討論後再動工

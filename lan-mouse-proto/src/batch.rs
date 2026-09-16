use input_event::{Event as InputEvent, KeyboardEvent, PointerEvent};
use std::fmt;

pub const MAX_BATCH_EVENTS: usize = 64;
pub const MAX_DATAGRAM_SIZE: usize = 4 + 64 * 5; // 324

/// magic byte — doubles as format version (v2: pick a new value)
pub const MAGIC: u8 = 0x4C;

/// Tag bits in byte 0
const TAG_COMPACT_MOTION: u8 = 0b00;
const TAG_EXTENDED_MOTION: u8 = 0b01;
const TAG_BUTTON: u8 = 0b10;
const TAG_KEY: u8 = 0b11;
const TAG_WHEEL: u8 = 0b11;

/// Subtype bits (lower 6 bits of byte 0)
const SUBTYPE_MASK: u8 = 0b0011_1111;

/// Error types for batch encoding/decoding
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BatchError {
    /// Buffer full — cannot fit another event
    BufferFull,
    /// Event cannot be batched (e.g. KeyboardModifiers)
    NonBatchable,
    /// Datagram truncated
    TruncatedDatagram,
    /// Bad magic byte
    BadMagic,
    /// Bad subtype for the given tag
    BadSubtype,
    /// Seq is stale (duplicate or far behind)
    StaleSeq,
    /// Wheel value overflows i8 ticks
    WheelOverflow,
}

impl fmt::Display for BatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatchError::BufferFull => write!(f, "batch buffer full"),
            BatchError::NonBatchable => write!(f, "event cannot be batched"),
            BatchError::TruncatedDatagram => write!(f, "truncated datagram"),
            BatchError::BadMagic => write!(f, "bad magic byte"),
            BatchError::BadSubtype => write!(f, "bad subtype"),
            BatchError::StaleSeq => write!(f, "stale sequence number"),
            BatchError::WheelOverflow => write!(f, "wheel ticks overflow i8"),
        }
    }
}

impl std::error::Error for BatchError {}

/// Accumulates events into a zero-allocation batched datagram.
///
/// Motion events are coalesced into an i32 accumulator and emitted on
/// flush (non-motion event or `finish`).  If the merged sum fits i8,
/// Compact is emitted; if it fits i16, Extended; if even i16 overflows,
/// the value is clamped to i16::MAX/MIN as Extended and the remainder
/// carries into a new accumulator (documented in `flush_motion`).
pub struct BatchEncoder {
    buffer: [u8; MAX_DATAGRAM_SIZE],
    pos: usize, // write position (starts at 4, after header)
    event_count: u8,
    acc_dx: i32,
    acc_dy: i32,
    has_motion: bool,
}

impl BatchEncoder {
    pub fn new() -> Self {
        Self {
            buffer: [0u8; MAX_DATAGRAM_SIZE],
            pos: 4, // reserve header space
            event_count: 0,
            acc_dx: 0,
            acc_dy: 0,
            has_motion: false,
        }
    }

    /// Push an input event into the batch.
    ///
    /// - Motion events are accumulated (coalesced).
    /// - Non-motion events flush accumulated motion first, then are appended.
    /// - `KeyboardEvent::Modifiers` returns `NonBatchable` so the caller
    ///   falls back to the legacy single-event path.
    pub fn push_event(&mut self, event: InputEvent) -> Result<(), BatchError> {
        match event {
            InputEvent::Pointer(PointerEvent::Motion { dx, dy, .. }) => {
                self.acc_dx += dx.round() as i32;
                self.acc_dy += dy.round() as i32;
                self.has_motion = true;
                Ok(())
            }
            InputEvent::Pointer(PointerEvent::Button { button, state, .. }) => {
                self.flush_motion()?;
                // state is u32; spec says state: u8 (0=Release, 1=Press)
                let state_u8 = state as u8;
                let mut payload = [0u8; 3];
                payload[0..2].copy_from_slice(&(button as u16).to_le_bytes());
                payload[2] = state_u8;
                self.write_event(TAG_BUTTON, 0, &payload)
            }
            InputEvent::Pointer(PointerEvent::Axis { axis, value, .. }) => {
                self.flush_motion()?;
                // Convert f64 axis value to nearest 1/120 ticks
                let ticks_f = value * 120.0;
                let ticks = ticks_f.round() as i32;
                if ticks < i8::MIN as i32 || ticks > i8::MAX as i32 {
                    return Err(BatchError::WheelOverflow);
                }
                self.write_event(TAG_WHEEL, 0, &[axis, ticks as i8 as u8])
            }
            InputEvent::Pointer(PointerEvent::AxisDiscrete120 { axis, value, .. }) => {
                self.flush_motion()?;
                // value is in 120ths; ticks = value / 120, error on remainder
                let ticks = value / 120;
                if value % 120 != 0 {
                    return Err(BatchError::WheelOverflow);
                }
                if ticks < i8::MIN as i32 || ticks > i8::MAX as i32 {
                    return Err(BatchError::WheelOverflow);
                }
                self.write_event(TAG_WHEEL, 0, &[axis, ticks as i8 as u8])
            }
            InputEvent::Keyboard(KeyboardEvent::Key { key, state, .. }) => {
                self.flush_motion()?;
                let mut payload = [0u8; 3];
                payload[0..2].copy_from_slice(&(key as u16).to_le_bytes());
                payload[2] = state;
                self.write_event(TAG_KEY, 1, &payload)
            }
            InputEvent::Keyboard(KeyboardEvent::Modifiers { .. }) => Err(BatchError::NonBatchable),
        }
    }

    /// Finalize the batch: flush remaining motion, write header, return the datagram slice.
    pub fn finish(&mut self, seq: u16) -> Result<&[u8], BatchError> {
        self.flush_motion()?;
        if self.event_count == 0 {
            return Err(BatchError::BufferFull); // nothing to encode
        }
        // Write header: magic, event_count, seq (LE)
        self.buffer[0] = MAGIC;
        self.buffer[1] = self.event_count;
        self.buffer[2..4].copy_from_slice(&seq.to_le_bytes());
        Ok(&self.buffer[..self.pos])
    }

    /// number of events currently held in the batch
    pub fn event_count(&self) -> usize {
        self.event_count as usize
    }

    /// true if anything is pending: encoded events OR coalesced motion
    /// still sitting in the accumulator (flushed only at `finish()`).
    pub fn has_pending(&self) -> bool {
        self.event_count > 0 || self.has_motion
    }

    /// drop all buffered events (used after a failed send)
    pub fn reset(&mut self) {
        self.pos = 4;
        self.event_count = 0;
        self.acc_dx = 0;
        self.acc_dy = 0;
        self.has_motion = false;
    }

    /// Flush accumulated motion into the buffer.
    fn flush_motion(&mut self) -> Result<(), BatchError> {
        if !self.has_motion {
            return Ok(());
        }

        let (dx, dy) = (self.acc_dx, self.acc_dy);

        if dx >= i8::MIN as i32
            && dx <= i8::MAX as i32
            && dy >= i8::MIN as i32
            && dy <= i8::MAX as i32
        {
            // Compact: fits i8
            self.write_event(TAG_COMPACT_MOTION, 0, &[dx as i8 as u8, dy as i8 as u8])?;
        } else if dx >= i16::MIN as i32
            && dx <= i16::MAX as i32
            && dy >= i16::MIN as i32
            && dy <= i16::MAX as i32
        {
            // Extended: fits i16 — narrow to i16 BEFORE encoding the payload
            // (dx/dy are i32 accumulators; to_le_bytes on i32 would emit 8 bytes)
            let mut payload = [0u8; 4];
            payload[0..2].copy_from_slice(&(dx as i16).to_le_bytes());
            payload[2..4].copy_from_slice(&(dy as i16).to_le_bytes());
            self.write_event(TAG_EXTENDED_MOTION, 0, &payload)?;
        } else {
            // Doesn't fit i16: clamp to i16::MAX/MIN, emit Extended, carry remainder
            let clamped_dx = dx.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let clamped_dy = dy.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let mut payload = [0u8; 4];
            payload[0..2].copy_from_slice(&clamped_dx.to_le_bytes());
            payload[2..4].copy_from_slice(&clamped_dy.to_le_bytes());
            self.write_event(TAG_EXTENDED_MOTION, 0, &payload)?;
            // Remainder carries into new accumulator
            self.acc_dx = dx - clamped_dx as i32;
            self.acc_dy = dy - clamped_dy as i32;
            // has_motion stays true — the remainder will be flushed on next call
            return Ok(());
        }

        self.acc_dx = 0;
        self.acc_dy = 0;
        self.has_motion = false;
        Ok(())
    }

    fn write_event(&mut self, tag: u8, subtype: u8, payload: &[u8]) -> Result<(), BatchError> {
        if self.event_count >= MAX_BATCH_EVENTS as u8 {
            return Err(BatchError::BufferFull);
        }
        let total_len = 1 + payload.len(); // tag byte + payload
        if self.pos + total_len > MAX_DATAGRAM_SIZE {
            return Err(BatchError::BufferFull);
        }
        self.buffer[self.pos] = (tag << 6) | (subtype & SUBTYPE_MASK);
        self.pos += 1;
        self.buffer[self.pos..self.pos + payload.len()].copy_from_slice(payload);
        self.pos += payload.len();
        self.event_count += 1;
        Ok(())
    }
}

impl Default for BatchEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode a batched datagram into events and the new sequence number.
///
/// # Seq staleness
/// Accepts iff `seq.wrapping_sub(last_seq) as u16` is in `1..=0x7FFF`.
/// For the first datagram, pass `last_seq = u16::MAX` to accept unconditionally
/// (since `seq.wrapping_sub(u16::MAX) == seq.wrapping_add(1)` which is in
/// range for seq in `0..=0x7FFE`).
pub fn decode_batch(datagram: &[u8], last_seq: u16) -> Result<(Vec<InputEvent>, u16), BatchError> {
    if datagram.len() < 4 {
        return Err(BatchError::TruncatedDatagram);
    }

    if datagram[0] != MAGIC {
        return Err(BatchError::BadMagic);
    }

    let event_count = datagram[1];
    if event_count == 0 || event_count > MAX_BATCH_EVENTS as u8 {
        return Err(BatchError::BadSubtype); // reuse for invalid count
    }

    let seq = u16::from_le_bytes([datagram[2], datagram[3]]);

    // Seq staleness check
    if last_seq != u16::MAX {
        let diff = seq.wrapping_sub(last_seq);
        if diff == 0 || diff > 0x7FFF {
            return Err(BatchError::StaleSeq);
        }
    }

    let mut events = Vec::new();
    let mut offset = 4;

    for _ in 0..event_count {
        if offset + 1 > datagram.len() {
            return Err(BatchError::TruncatedDatagram);
        }

        let byte0 = datagram[offset];
        offset += 1;

        let tag = byte0 >> 6;
        let subtype = byte0 & SUBTYPE_MASK;

        match (tag, subtype) {
            (TAG_COMPACT_MOTION, 0) => {
                if offset + 2 > datagram.len() {
                    return Err(BatchError::TruncatedDatagram);
                }
                let dx = datagram[offset] as i8;
                let dy = datagram[offset + 1] as i8;
                offset += 2;
                events.push(InputEvent::Pointer(PointerEvent::Motion {
                    time: 0,
                    dx: dx as f64,
                    dy: dy as f64,
                }));
            }
            (TAG_EXTENDED_MOTION, 0) => {
                if offset + 4 > datagram.len() {
                    return Err(BatchError::TruncatedDatagram);
                }
                let dx = i16::from_le_bytes([datagram[offset], datagram[offset + 1]]);
                let dy = i16::from_le_bytes([datagram[offset + 2], datagram[offset + 3]]);
                offset += 4;
                events.push(InputEvent::Pointer(PointerEvent::Motion {
                    time: 0,
                    dx: dx as f64,
                    dy: dy as f64,
                }));
            }
            (TAG_BUTTON, 0) => {
                if offset + 3 > datagram.len() {
                    return Err(BatchError::TruncatedDatagram);
                }
                let code = u16::from_le_bytes([datagram[offset], datagram[offset + 1]]);
                let state = datagram[offset + 2];
                offset += 3;
                events.push(InputEvent::Pointer(PointerEvent::Button {
                    time: 0,
                    button: code as u32,
                    state: state as u32,
                }));
            }
            (TAG_KEY, 1) => {
                if offset + 3 > datagram.len() {
                    return Err(BatchError::TruncatedDatagram);
                }
                let code = u16::from_le_bytes([datagram[offset], datagram[offset + 1]]);
                let state = datagram[offset + 2];
                offset += 3;
                events.push(InputEvent::Keyboard(KeyboardEvent::Key {
                    time: 0,
                    key: code as u32,
                    state,
                }));
            }
            (TAG_WHEEL, 0) => {
                if offset + 2 > datagram.len() {
                    return Err(BatchError::TruncatedDatagram);
                }
                let axis = datagram[offset];
                let ticks = datagram[offset + 1] as i8;
                offset += 2;
                events.push(InputEvent::Pointer(PointerEvent::AxisDiscrete120 {
                    axis,
                    // wire carries ticks (1/120 of a scroll tick); restore 120ths
                    value: ticks as i32 * 120,
                }));
            }
            _ => return Err(BatchError::BadSubtype),
        }
    }

    Ok((events, seq))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motion(dx: f64, dy: f64) -> InputEvent {
        InputEvent::Pointer(PointerEvent::Motion { time: 0, dx, dy })
    }

    fn button(code: u32, state: u32) -> InputEvent {
        InputEvent::Pointer(PointerEvent::Button {
            time: 0,
            button: code,
            state,
        })
    }

    fn key(key: u32, state: u8) -> InputEvent {
        InputEvent::Keyboard(KeyboardEvent::Key {
            time: 0,
            key,
            state,
        })
    }

    fn axis(axis: u8, value: f64) -> InputEvent {
        InputEvent::Pointer(PointerEvent::Axis {
            time: 0,
            axis,
            value,
        })
    }

    fn axis_discrete120(axis: u8, value: i32) -> InputEvent {
        InputEvent::Pointer(PointerEvent::AxisDiscrete120 { axis, value })
    }

    fn modifiers() -> InputEvent {
        InputEvent::Keyboard(KeyboardEvent::Modifiers {
            depressed: 0,
            latched: 0,
            locked: 0,
            group: 0,
        })
    }

    #[test]
    fn pure_motion_batch_is_pending_before_finish() {
        // Regression: coalesced motion lives in the accumulator with
        // event_count()==0 until finish() encodes it. A flush path that
        // gates on event_count alone would reset() and drop the movement.
        let mut enc = BatchEncoder::new();
        assert!(!enc.has_pending());
        enc.push_event(motion(3.0, -7.0)).unwrap();
        assert!(enc.has_pending());
        assert_eq!(enc.event_count(), 0); // still in accumulator
        let buf = enc.finish(1).unwrap();
        assert_eq!(buf[0], MAGIC);
        assert_eq!(buf[1], 1); // one motion event encoded
    }

    #[test]
    fn round_trip_compact_motion() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(5.0, -3.0)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, seq) = decode_batch(buf, u16::MAX).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], motion(5.0, -3.0));
    }

    #[test]
    fn round_trip_extended_motion() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(500.0, -300.0)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        assert_eq!(events[0], motion(500.0, -300.0));
    }

    #[test]
    fn round_trip_button() {
        let mut enc = BatchEncoder::new();
        enc.push_event(button(0x110, 1)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        assert_eq!(events[0], button(0x110, 1));
    }

    #[test]
    fn round_trip_key() {
        let mut enc = BatchEncoder::new();
        enc.push_event(key(30, 1)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        assert_eq!(events[0], key(30, 1));
    }

    #[test]
    fn round_trip_wheel() {
        let mut enc = BatchEncoder::new();
        enc.push_event(axis_discrete120(0, 120)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        assert_eq!(events[0], axis_discrete120(0, 120));
    }

    #[test]
    fn truncation_rejected() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(5.0, -3.0)).unwrap();
        let buf = enc.finish(1).unwrap();
        // Truncate to 3 bytes (less than header)
        assert_eq!(
            decode_batch(&buf[..3], u16::MAX),
            Err(BatchError::TruncatedDatagram)
        );
    }

    #[test]
    fn bad_subtype_rejected() {
        // Build a datagram with tag=0b00, subtype=1 (invalid for motion)
        let mut bad = [0u8; 7];
        bad[0] = MAGIC;
        bad[1] = 1; // 1 event
        bad[2..4].copy_from_slice(&1u16.to_le_bytes());
        bad[4] = 0b00_000001; // Compact Motion with subtype=1 → bad
        bad[5] = 5;
        bad[6] = -3i8 as u8;
        assert_eq!(decode_batch(&bad, u16::MAX), Err(BatchError::BadSubtype));
    }

    #[test]
    fn wrong_magic_rejected() {
        let mut bad = [0u8; 7];
        bad[0] = 0x00; // wrong magic
        bad[1] = 1;
        bad[2..4].copy_from_slice(&1u16.to_le_bytes());
        bad[4] = 0x00;
        bad[5] = 5;
        bad[6] = 0;
        assert_eq!(decode_batch(&bad, u16::MAX), Err(BatchError::BadMagic));
    }

    #[test]
    fn seq_staleness() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(1.0, 0.0)).unwrap();
        let buf = enc.finish(10).unwrap();

        // Accept: seq=10, last_seq=9 → diff=1
        assert!(decode_batch(buf, 9).is_ok());

        // Duplicate: seq=10, last_seq=10 → diff=0
        assert_eq!(decode_batch(buf, 10), Err(BatchError::StaleSeq));

        // Far behind: seq=10, last_seq=100 → diff wrapping = 10-100 = 65536-90 = 65446 > 0x7FFF
        assert_eq!(decode_batch(buf, 100), Err(BatchError::StaleSeq));

        // Accept wrap-around: seq=10, last_seq=0xFFF0 → diff = 10 - 0xFFF0 = 26 (in range)
        assert!(decode_batch(buf, 0xFFF0).is_ok());
    }

    #[test]
    fn first_datagram_unconditional() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(1.0, 0.0)).unwrap();
        let buf = enc.finish(0).unwrap();
        // last_seq = u16::MAX → unconditional accept
        assert!(decode_batch(buf, u16::MAX).is_ok());
    }

    #[test]
    fn coalescing_compact() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(3.0, 2.0)).unwrap();
        enc.push_event(motion(4.0, 5.0)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        // Coalesced: dx=7, dy=7 → fits i8 → Compact
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], motion(7.0, 7.0));
    }

    #[test]
    fn coalescing_extended() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(200.0, 300.0)).unwrap();
        enc.push_event(motion(100.0, 200.0)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        // Coalesced: dx=300, dy=500 → fits i16 → Extended
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], motion(300.0, 500.0));
    }

    #[test]
    fn coalescing_i16_overflow_clamped() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(40000.0, 0.0)).unwrap();
        enc.push_event(motion(40000.0, 0.0)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        // Coalesced: dx=80000 → doesn't fit i16 → clamped to i16::MAX=32767
        // Remainder 80000-32767=47233 carried → second Extended event
        assert!(!events.is_empty());
        // First event should be clamped Extended
        match &events[0] {
            InputEvent::Pointer(PointerEvent::Motion { dx, dy, .. }) => {
                assert_eq!(*dx, i16::MAX as f64);
                assert_eq!(*dy, 0.0);
            }
            _ => panic!("expected motion"),
        }
    }

    #[test]
    fn non_batchable_modifiers() {
        let mut enc = BatchEncoder::new();
        assert_eq!(enc.push_event(modifiers()), Err(BatchError::NonBatchable));
    }

    #[test]
    fn max_batch_full() {
        let mut enc = BatchEncoder::new();
        // Buttons (3B each) — motions would coalesce and never fill the buffer
        for _ in 0..64 {
            enc.push_event(button(0x110, 1)).unwrap();
        }
        // 65th event should fail (buffer full)
        assert_eq!(
            enc.push_event(button(0x110, 1)),
            Err(BatchError::BufferFull)
        );
    }

    #[test]
    fn non_motion_flushes_motion() {
        let mut enc = BatchEncoder::new();
        enc.push_event(motion(3.0, 2.0)).unwrap();
        enc.push_event(button(0x110, 1)).unwrap();
        let buf = enc.finish(1).unwrap();
        let (events, _) = decode_batch(buf, u16::MAX).unwrap();
        // Motion flushed first, then button
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], motion(3.0, 2.0));
        assert_eq!(events[1], button(0x110, 1));
    }

    #[test]
    fn wheel_axis_overflow() {
        let mut enc = BatchEncoder::new();
        // Axis value * 120 doesn't fit i8
        assert_eq!(
            enc.push_event(axis(0, 1000.0)),
            Err(BatchError::WheelOverflow)
        );
    }

    #[test]
    fn wheel_axis_discrete120_remainder() {
        let mut enc = BatchEncoder::new();
        // value % 120 != 0 → error
        assert_eq!(
            enc.push_event(axis_discrete120(0, 130)),
            Err(BatchError::WheelOverflow)
        );
    }

    #[test]
    fn wheel_axis_discrete120_overflow() {
        let mut enc = BatchEncoder::new();
        // value / 120 doesn't fit i8
        assert_eq!(
            enc.push_event(axis_discrete120(0, 20000)),
            Err(BatchError::WheelOverflow)
        );
    }

    /// Wire-size math used in the README benchmarks table.
    #[test]
    fn wire_sizes() {
        // legacy motion datagram: event_id(1) + time(u32) + dx(f64) + dy(f64)
        let (buf, len) = crate::ProtoEvent::Input(motion(1.0, 1.0)).into();
        assert_eq!(len, 1 + 4 + 8 + 8);
        let _ = buf;

        // batched: many small motions coalesce into one Compact event
        // (100 events of (1,0) sum to dx=100, still fits i8)
        let mut enc = BatchEncoder::new();
        for _ in 0..100 {
            enc.push_event(motion(1.0, 0.0)).unwrap();
        }
        let datagram = enc.finish(0).unwrap();
        assert_eq!(datagram.len(), 4 + 3); // header + one Compact Motion
        assert_eq!(datagram.len(), 7);

        // worst-case per-event: Extended Motion = tag(1) + i16 + i16 = 5 bytes
        assert_eq!(4 + 64 * 5, MAX_DATAGRAM_SIZE);
    }

    /// Micro-benchmark for the README. Run explicitly:
    ///   cargo test -p lan-mouse-proto --release -- --ignored --nocapture bench
    #[test]
    #[ignore]
    fn bench_round_trip() {
        let n = 10_000;
        let events: Vec<InputEvent> = (0..n)
            .map(|i| motion(((i % 7) - 3) as f64, ((i % 5) - 2) as f64))
            .collect();

        let t = std::time::Instant::now();
        let mut datagrams = 0usize;
        let mut bytes = 0usize;
        for chunk in events.chunks(MAX_BATCH_EVENTS) {
            let mut enc = BatchEncoder::new();
            for e in chunk {
                enc.push_event(*e).unwrap();
            }
            let d = enc.finish(datagrams as u16).unwrap();
            bytes += d.len();
            datagrams += 1;
        }
        let enc_t = t.elapsed();

        let t = std::time::Instant::now();
        let mut decoded = 0usize;
        for seq in 0..datagrams {
            // re-encode to have a datagram to decode
            let start = seq * MAX_BATCH_EVENTS;
            let end = (start + MAX_BATCH_EVENTS).min(events.len());
            let mut enc = BatchEncoder::new();
            for e in &events[start..end] {
                enc.push_event(*e).unwrap();
            }
            let d = enc.finish(seq as u16).unwrap().to_vec();
            let last = if seq == 0 { u16::MAX } else { (seq - 1) as u16 };
            let (evts, _) = decode_batch(&d, last).unwrap();
            decoded += evts.len();
        }
        let dec_t = t.elapsed();

        println!(
            "encode: {n} events -> {datagrams} datagrams ({bytes} B) in {enc_t:?} | \
             decode: {decoded} events in {dec_t:?}"
        );
        // coalescing collapses each datagram's motions into one event
        assert_eq!(decoded, datagrams);
    }
}

use std::{
    io::{self, Write},
    os::fd::AsFd,
};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat},
};

use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};

use crate::{error::ClipboardError, origin::Origin};

/// clipboard content cap: `read` errors above this instead of truncating
pub const MAX_CLIPBOARD_SIZE: usize = 1_048_576; // 1 MiB

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// selection changed; empty `mimes` = selection cleared (NULL selection)
    Changed { mimes: Vec<String> },
}

struct State {
    /// mimes advertised by the current selection offer (empty = no selection)
    selection_mimes: Vec<String>,
    /// offer object of the current selection, kept for `read()`
    selection_offer: Option<ExtDataControlOfferV1>,
    /// offer introduced by data_offer, resolved by the following selection event
    pending_offer: Option<ExtDataControlOfferV1>,
    /// source object we created via `offer()`; used for loop suppression
    own_source: Option<ExtDataControlSourceV1>,
    /// (mimes, payload) the peer may request from our source
    own_payload: Option<(Vec<String>, Vec<u8>)>,
    /// true while the next selection event was caused by our own set_selection
    suppress_next_selection: bool,
    cb: Option<Box<dyn FnMut(Event)>>,
}

impl State {
    fn emit(&mut self, event: Event) {
        if let Some(cb) = &mut self.cb {
            cb(event);
        }
    }
}

/// A live clipboard session: owns the wayland connection, the
/// ext_data_control device, and the selection state.
pub struct ClipboardSession {
    _conn: Connection,
    event_queue: EventQueue<State>,
    state: State,
    _seat: wl_seat::WlSeat,
    manager: ExtDataControlManagerV1,
    device: ExtDataControlDeviceV1,
    next_origin_id: u32,
}

impl ClipboardSession {
    /// Connect, bind globals, create the data-control device. Fails with
    /// [`ClipboardError::ProtocolUnavailable`] when the compositor does not
    /// advertise ext_data_control_manager_v1 (e.g. GNOME/Mutter).
    pub fn new() -> Result<Self, ClipboardError> {
        Self::with_callback(Box::new(|_| {}))
    }

    /// Like [`ClipboardSession::new`], selection changes invoke `cb`.
    pub fn with_callback(cb: Box<dyn FnMut(Event)>) -> Result<Self, ClipboardError> {
        let conn = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init::<State>(&conn)?;
        let qh = event_queue.handle();

        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ())?;
        let manager: ExtDataControlManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|_| ClipboardError::ProtocolUnavailable)?;

        let state = State {
            selection_mimes: Vec::new(),
            selection_offer: None,
            pending_offer: None,
            own_source: None,
            own_payload: None,
            suppress_next_selection: false,
            cb: Some(cb),
        };

        let device = manager.get_data_device(&seat, &qh, ());
        let mut session = Self {
            _conn: conn,
            event_queue,
            state,
            _seat: seat,
            manager,
            device,
            next_origin_id: 1,
        };
        // first roundtrip delivers the initial selection event (if any)
        session.event_queue.roundtrip(&mut session.state)?;
        Ok(session)
    }

    /// MIME types of the current selection (empty = cleared)
    pub fn selection_mimes(&self) -> &[String] {
        &self.state.selection_mimes
    }

    /// Stream the current selection's `mime` content. Errors with
    /// [`ClipboardError::TooLarge`] if the content exceeds `max_len` —
    /// never truncates.
    pub fn read(
        &mut self,
        mime: &str,
        max_len: usize,
    ) -> Result<Box<dyn io::Read + Send>, ClipboardError> {
        if !mime.starts_with("text/plain") {
            return Err(ClipboardError::UnsupportedMime(mime.to_string()));
        }
        let Some(offer) = self.state.selection_offer.as_ref() else {
            return Err(ClipboardError::NoSelection);
        };
        if !self.state.selection_mimes.iter().any(|m| m == mime) {
            return Err(ClipboardError::UnsupportedMime(mime.to_string()));
        }

        // receive(mime, fd): the compositor hands the write end to the source
        // client; we read from the read end until EOF.
        let (reader, writer) = io::pipe()?;
        offer.receive(mime.to_string(), writer.as_fd());
        // flush the request before reading, else the compositor never sees it
        self.event_queue.flush()?;
        // drop our write end so the reader sees EOF when the source closes its copy
        drop(writer);

        Ok(Box::new(LimitedReader {
            inner: reader,
            max_len,
            total: 0,
            errored: false,
        }))
    }

    /// Become the selection source for `mimes`, serving `payload` to peers
    /// that receive it. Returns an [`Origin`] token for loop suppression:
    /// the selection change caused by this write updates internal state
    /// but is NOT reported to the callback.
    pub fn offer(&mut self, mimes: &[&str], payload: Vec<u8>) -> Result<Origin, ClipboardError> {
        if payload.len() > MAX_CLIPBOARD_SIZE {
            return Err(ClipboardError::TooLarge(MAX_CLIPBOARD_SIZE));
        }
        let source = self
            .manager
            .create_data_source(&self.event_queue.handle(), ());
        for m in mimes {
            source.offer(m.to_string());
        }
        self.device.set_selection(Some(&source));
        self.state.own_source = Some(source);
        self.state.own_payload = Some((mimes.iter().map(|s| s.to_string()).collect(), payload));
        self.state.suppress_next_selection = true;
        self.event_queue.flush()?;
        let origin = Origin::new(self.next_origin_id);
        self.next_origin_id += 1;
        Ok(origin)
    }

    /// Dispatch pending events without blocking.
    pub fn dispatch_pending(&mut self) -> Result<usize, ClipboardError> {
        Ok(self.event_queue.dispatch_pending(&mut self.state)?)
    }

    /// Block forever dispatching events (the `watch` primitive).
    pub fn run(&mut self) -> Result<(), ClipboardError> {
        loop {
            self.event_queue.blocking_dispatch(&mut self.state)?;
        }
    }
}

/// Reader that errors once `max_len` is exceeded — never truncates.
struct LimitedReader {
    inner: io::PipeReader,
    max_len: usize,
    total: usize,
    errored: bool,
}

impl io::Read for LimitedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.errored {
            return Err(io::Error::other("clipboard too large"));
        }
        let n = self.inner.read(buf)?;
        self.total += n;
        if self.total > self.max_len {
            self.errored = true;
            return Err(io::Error::other(
                ClipboardError::TooLarge(self.max_len).to_string(),
            ));
        }
        Ok(n)
    }
}

/// Watch for selection changes, invoking `cb` on every change. Blocks the
/// calling thread until the connection errors. Changes caused by our own
/// writes (see [`ClipboardSession::offer`]) are suppressed.
pub fn watch(cb: impl FnMut(Event) + 'static) -> Result<(), ClipboardError> {
    let mut session = ClipboardSession::with_callback(Box::new(cb))?;
    session.run()
}

// --- Dispatch impls ---

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore ExtDataControlManagerV1);

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    // DataOffer events create a new offer object (opcode 0)
    event_created_child!(State, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ())
    ]);
    fn event(
        state: &mut Self,
        _: &ExtDataControlDeviceV1,
        event: <ExtDataControlDeviceV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                // new offer cycle: reset accumulated mimes, stash the object
                state.selection_mimes.clear();
                state.pending_offer = Some(id);
            }
            ext_data_control_device_v1::Event::Selection { id: selection } => {
                state.selection_offer = state.pending_offer.take();
                if selection.is_none() {
                    state.selection_mimes.clear();
                }
                // loop suppression: our own set_selection caused this event;
                // update state but don't report it as a remote change
                if state.suppress_next_selection {
                    state.suppress_next_selection = false;
                    return;
                }
                let mimes = state.selection_mimes.clone();
                state.emit(Event::Changed { mimes });
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtDataControlOfferV1,
        event: <ExtDataControlOfferV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.selection_mimes.push(mime_type);
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtDataControlSourceV1,
        event: <ExtDataControlSourceV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { mime_type, fd } => {
                if let Some((mimes, payload)) = &state.own_payload {
                    if mimes.iter().any(|m| m == &mime_type) {
                        // write payload to fd; File::from(fd) closes on drop
                        let mut file = io::BufWriter::new(std::fs::File::from(fd));
                        let _ = file.write_all(payload);
                    }
                }
            }
            ext_data_control_source_v1::Event::Cancelled => {
                state.own_source = None;
                state.own_payload = None;
            }
            _ => {}
        }
    }
}

/// Live roundtrip against a real compositor (needs WAYLAND_DISPLAY).
/// Run manually: `cargo test -p input-clipboard -- --ignored`
#[cfg(test)]
#[test]
#[ignore = "requires a live wayland compositor with ext-data-control-v1"]
fn live_roundtrip_offer_then_read() {
    use std::sync::mpsc;
    let mut session = ClipboardSession::new().expect("session");
    let payload = b"lan-mouse clipboard roundtrip".to_vec();
    session
        .offer(&["text/plain;charset=utf-8"], payload.clone())
        .expect("offer");
    // let the compositor process set_selection and deliver the selection event
    session.event_queue.roundtrip(&mut session.state).unwrap();
    session.event_queue.roundtrip(&mut session.state).unwrap();
    let mut reader = session
        .read("text/plain;charset=utf-8", MAX_CLIPBOARD_SIZE)
        .expect("read");
    // the reader only sees data once OUR source's Send event is dispatched,
    // so pumping must happen concurrently with reading
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut got = Vec::new();
        let res = reader.read_to_end(&mut got).map(|_| got);
        let _ = tx.send(res);
    });
    let mut got = None;
    for _ in 0..50 {
        if let Ok(Ok(v)) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            got = Some(v);
            break;
        }
        session.event_queue.roundtrip(&mut session.state).unwrap();
    }
    assert_eq!(got.expect("read within timeout"), payload);
}

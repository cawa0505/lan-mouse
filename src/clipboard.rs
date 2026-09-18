//! Opt-in clipboard sync: watches the local Wayland clipboard and pushes
//! changes to configured peers over TCP + Noise_IK; receives pushes from
//! peers and writes them to the local clipboard.
//!
//! All threads are `std::thread` — the daemon's tokio runtime and select
//! loop are untouched. Disabled by default: `spawn()` returns `None` and
//! starts nothing unless `[clipboard] enabled = true`.

use crate::config::{ClipboardConfig, ConfigClient};
use input_clipboard::{ClipboardSession, Event, MAX_CLIPBOARD_SIZE};
use std::{
    collections::HashSet,
    error::Error,
    fs,
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use velokvm_proto::{
    ClipboardAssembler, ClipboardMsg, MAX_CHUNK_LEN, NOISE_PARAMS, NoiseIkTcpChannel,
    SUPPORTED_MIME,
};

const KEY_LEN: usize = 32;
const MAX_HANDSHAKE_MSG: usize = 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// cross-session loop suppression window: our own `offer()` (from the
/// responder thread's session) surfaces as a `Changed` event in the watcher
/// session; events within this window are ignored.
// ponytail: 500ms grace window, replace with per-origin token if missed copies are reported
const GRACE_WINDOW: Duration = Duration::from_millis(500);

/// A peer we push clipboard changes to.
pub struct ClipboardPeer {
    pub ip: IpAddr,
    pub port: u16,
    pub public_key: [u8; 32],
}

/// Parse a 64-char hex X25519 public key.
pub fn parse_public_key(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != KEY_LEN * 2 {
        return None;
    }
    let mut out = [0u8; KEY_LEN];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Build the peer list from configured clients: only clients with
/// `clipboard = true` and a valid `clipboard_key` are included.
pub fn peers(clients: &[ConfigClient], port: u16) -> Vec<ClipboardPeer> {
    clients
        .iter()
        .filter(|c| c.clipboard)
        .filter_map(|c| {
            let Some(key) = c.clipboard_key.as_deref() else {
                log::warn!(
                    "clipboard: client {} has clipboard = true but no clipboard_key",
                    c.hostname.as_deref().unwrap_or("?")
                );
                return None;
            };
            let Some(public_key) = parse_public_key(key) else {
                log::warn!(
                    "clipboard: client {} has invalid clipboard_key",
                    c.hostname.as_deref().unwrap_or("?")
                );
                return None;
            };
            Some(c.ips.iter().map(move |ip| ClipboardPeer {
                ip: *ip,
                port,
                public_key,
            }))
        })
        .flatten()
        .collect()
}

/// Start clipboard sync. Returns `None` (and logs) when disabled or when
/// startup fails (missing key, bind failure) — the daemon continues normally.
pub fn spawn(config: ClipboardConfig, peers: Vec<ClipboardPeer>) -> Option<JoinHandle<()>> {
    if !config.enabled {
        return None;
    }
    let private_key = load_private_key(config.private_key_path.as_deref())?;
    let listener = match TcpListener::bind(("0.0.0.0", config.port)) {
        Ok(l) => l,
        Err(e) => {
            log::error!("clipboard: failed to bind port {}: {e}", config.port);
            return None;
        }
    };
    let allow: HashSet<String> = config.allow_keys.into_iter().collect();
    let grace: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let responder = {
        let grace = grace.clone();
        thread::spawn(move || responder_loop(listener, private_key, allow, grace))
    };
    thread::spawn(move || watcher_loop(private_key, peers, grace));
    Some(responder)
}

fn load_private_key(path: Option<&str>) -> Option<[u8; KEY_LEN]> {
    let Some(path) = path else {
        log::error!("clipboard: no private_key_path configured");
        return None;
    };
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            log::error!("clipboard: failed to read private key {path:?}: {e}");
            return None;
        }
    };
    let mut key = [0u8; KEY_LEN];
    if bytes.len() != KEY_LEN {
        log::error!("clipboard: private key {path:?} must be exactly {KEY_LEN} bytes");
        return None;
    }
    key.copy_from_slice(&bytes);
    Some(key)
}

/// Accept loop: one thread per connection, each running a Noise_IK responder
/// handshake with a strict whitelist check.
fn responder_loop(
    listener: TcpListener,
    private_key: [u8; 32],
    allow: HashSet<String>,
    grace: Arc<Mutex<Option<Instant>>>,
) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let grace = grace.clone();
                let allow = allow.clone();
                thread::spawn(move || {
                    if let Err(e) = serve_conn(stream, &private_key, &allow, &grace) {
                        log::debug!("clipboard: connection ended: {e}");
                    }
                });
            }
            Err(e) => log::error!("clipboard: accept failed: {e}"),
        }
    }
}

/// Single connection: Noise_IK responder handshake → whitelist → receive and
/// reassemble transfers, offering completed payloads to the local clipboard.
fn serve_conn(
    mut stream: TcpStream,
    private_key: &[u8],
    allow: &HashSet<String>,
    grace: &Arc<Mutex<Option<Instant>>>,
) -> Result<(), Box<dyn Error>> {
    let mut handshake = snow::Builder::new(NOISE_PARAMS.parse()?)
        .local_private_key(private_key)
        .build_responder()?;

    // msg1: initiator → responder (IK 0-RTT, carries the initiator's static key)
    let msg1 = read_frame(&mut stream)?;
    let mut payload = [0u8; MAX_HANDSHAKE_MSG];
    handshake.read_message(&msg1, &mut payload)?;

    // the remote static key is only known after msg1 — the whitelist check
    // happens here; on a miss we silently drop (no msg2)
    let remote = handshake
        .get_remote_static()
        .map(to_hex)
        .ok_or("handshake provided no remote static key")?;
    if !allow.contains(&remote) {
        log::warn!("clipboard: rejecting unauthorized peer {remote}");
        return Ok(());
    }

    // msg2: responder → initiator
    let mut msg2 = [0u8; MAX_HANDSHAKE_MSG];
    let n = handshake.write_message(&[], &mut msg2)?;
    write_frame(&mut stream, &msg2[..n])?;

    let transport = handshake.into_transport_mode()?;
    log::info!("clipboard: Noise_IK handshake complete with {remote}");

    let mut channel = NoiseIkTcpChannel::new(stream, transport);
    let mut assembler = ClipboardAssembler::new();
    loop {
        match channel.recv_msg() {
            Ok(ClipboardMsg::Offer {
                transfer_id,
                mime,
                total_len,
            }) => {
                if let Err(e) = assembler.offer(transfer_id, &mime, total_len) {
                    log::warn!("clipboard: rejecting offer: {e}");
                    assembler.abort();
                }
            }
            Ok(ClipboardMsg::Chunk {
                transfer_id,
                seq,
                payload,
            }) => {
                if let Err(e) = assembler.push_chunk(transfer_id, seq, &payload) {
                    log::warn!("clipboard: rejecting chunk: {e}");
                    assembler.abort();
                }
            }
            Ok(ClipboardMsg::Complete { transfer_id }) => match assembler.complete() {
                Ok(data) => {
                    let mut session = match ClipboardSession::new() {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("clipboard: failed to create session: {e}");
                            return Ok(());
                        }
                    };
                    match session.offer(&[SUPPORTED_MIME], data) {
                        Ok(_) => {
                            if let Ok(mut g) = grace.lock() {
                                *g = Some(Instant::now());
                            }
                            log::info!("clipboard: received transfer #{transfer_id} from {remote}");
                        }
                        Err(e) => log::error!("clipboard: failed to offer to clipboard: {e}"),
                    }
                }
                Err(e) => log::warn!("clipboard: incomplete transfer: {e}"),
            },
            Ok(ClipboardMsg::Abort {
                transfer_id,
                reason,
            }) => {
                assembler.abort();
                log::info!("clipboard: transfer #{transfer_id} aborted (reason {reason})");
            }
            Err(e) => {
                log::debug!("clipboard: connection closed: {e}");
                return Ok(());
            }
        }
    }
}

/// Watch the local clipboard; on text selection changes, push the payload to
/// every configured peer.
fn watcher_loop(
    private_key: [u8; 32],
    peers: Vec<ClipboardPeer>,
    grace: Arc<Mutex<Option<Instant>>>,
) {
    let result = input_clipboard::watch(move |ev| {
        let Event::Changed { mimes } = ev;
        if !mimes.iter().any(|m| m.starts_with("text/plain")) {
            return;
        }
        // cross-session loop suppression: the responder thread's offer()
        // surfaces here as a Changed event in this watcher session too
        let within_grace = grace
            .lock()
            .map(|g| g.is_some_and(|t| t.elapsed() < GRACE_WINDOW))
            .unwrap_or(false);
        if within_grace {
            return;
        }
        // the watch callback cannot borrow the session it is dispatched
        // from, so read the current selection via a fresh session
        let mut session = match ClipboardSession::new() {
            Ok(s) => s,
            Err(e) => {
                log::error!("clipboard: failed to create session: {e}");
                return;
            }
        };
        let mut reader = match session.read("text/plain;charset=utf-8", MAX_CLIPBOARD_SIZE) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("clipboard: failed to read selection: {e}");
                return;
            }
        };
        let mut payload = Vec::new();
        if let Err(e) = reader.read_to_end(&mut payload) {
            log::warn!("clipboard: failed to read payload: {e}");
            return;
        }
        for peer in &peers {
            match push_to_peer(peer, &private_key, &payload) {
                Ok(()) => log::info!("clipboard: pushed {} bytes to {}", payload.len(), peer.ip),
                Err(e) => log::warn!("clipboard: failed to push to {}: {e}", peer.ip),
            }
        }
    });
    if let Err(e) = result {
        log::error!("clipboard: watcher failed: {e}");
    }
}

/// Initiator push: Noise_IK handshake with the peer's public key, then
/// Offer → Chunk(s) → Complete over the encrypted channel.
fn push_to_peer(
    peer: &ClipboardPeer,
    private_key: &[u8],
    payload: &[u8],
) -> Result<(), Box<dyn Error>> {
    let addr = SocketAddr::new(peer.ip, peer.port);
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    let mut handshake = snow::Builder::new(NOISE_PARAMS.parse()?)
        .local_private_key(private_key)
        .remote_public_key(&peer.public_key)
        .build_initiator()?;

    let mut buf = [0u8; MAX_HANDSHAKE_MSG];
    let n = handshake.write_message(&[], &mut buf)?;
    write_frame(&mut stream, &buf[..n])?;
    let msg2 = read_frame(&mut stream)?;
    let mut payload_buf = [0u8; MAX_HANDSHAKE_MSG];
    handshake.read_message(&msg2, &mut payload_buf)?;

    let mut channel = NoiseIkTcpChannel::new(stream, handshake.into_transport_mode()?);
    let transfer_id = rand::random::<u32>();
    channel.send_msg(&ClipboardMsg::Offer {
        transfer_id,
        mime: SUPPORTED_MIME.to_string(),
        total_len: payload.len() as u64,
    })?;
    for (seq, chunk) in payload.chunks(MAX_CHUNK_LEN).enumerate() {
        channel.send_msg(&ClipboardMsg::Chunk {
            transfer_id,
            seq: seq as u32,
            payload: chunk.to_vec(),
        })?;
    }
    channel.send_msg(&ClipboardMsg::Complete { transfer_id })?;
    Ok(())
}

/// Handshake message framing: `u16 BE` length prefix (same as
/// `NoiseIkTcpChannel` data frames).
fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf)?;
    let len = u16::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_HANDSHAKE_MSG {
        return Err(format!("invalid handshake frame length: {len}").into());
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

fn write_frame(stream: &mut TcpStream, data: &[u8]) -> Result<(), Box<dyn Error>> {
    if data.is_empty() || data.len() > MAX_HANDSHAKE_MSG {
        return Err("invalid handshake message length".into());
    }
    stream.write_all(&(data.len() as u16).to_be_bytes())?;
    stream.write_all(data)?;
    stream.flush()?;
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

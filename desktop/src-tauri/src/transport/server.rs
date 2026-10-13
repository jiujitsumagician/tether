//! TLS 1.3 + WebSocket listener.
//!
//! The PC is the server: the Android client dials in by mDNS-discovered
//! port, UDP-broadcasted port, /24-probed port, or USB-tunnelled
//! loopback. This module accepts those connections, performs the
//! WebSocket upgrade, and hands each upgraded channel back to the
//! pairing state machine via a channel.

use super::tls::{server_config, ServerTlsStream};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{timeout, Duration};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::WebSocketStream;

/// Upper bound on how long a connection may take to finish the TLS
/// handshake, and (separately) the WebSocket upgrade. A real phone does
/// both in milliseconds; this only drops stalled or hostile peers.
const ACCEPT_STEP_TIMEOUT: Duration = Duration::from_secs(15);

/// Max size of a single WebSocket message / frame. Handshake envelopes
/// are well under 1 KiB, so 64 KiB leaves ample headroom while keeping
/// an unauthenticated peer from making us buffer tungstenite's default
/// 64 MiB messages.
const MAX_WS_MESSAGE_BYTES: usize = 64 * 1024;

/// Max number of connections that may be between TCP accept and
/// finished WebSocket upgrade at the same time. Further connections are
/// dropped immediately.
const MAX_PENDING_CONNECTIONS: usize = 32;

pub struct AcceptedClient {
    pub peer_addr: SocketAddr,
    pub ws: WebSocketStream<ServerTlsStream>,
    /// The PIN supplied by the client in the WS upgrade query string,
    /// if any. The manual-entry path uses this; mDNS/UDP/USB paths
    /// supply no PIN.
    pub pin: Option<String>,
}

/// Run the listener forever. Returns Err if the bind itself failed;
/// individual connection errors are logged and skipped.
pub async fn serve(
    bind_addr: SocketAddr,
    incoming_tx: mpsc::Sender<AcceptedClient>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    let cfg = server_config().await?;
    let acceptor = TlsAcceptor::from(cfg);
    tracing::info!("tether TLS listener bound on {bind_addr}");
    let pending = Arc::new(Semaphore::new(MAX_PENDING_CONNECTIONS));

    loop {
        let (tcp, peer_addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        tcp.set_nodelay(true).ok();
        tracing::info!("incoming TCP from {peer_addr}");
        let permit = match Arc::clone(&pending).try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("too many pending connections; dropping {peer_addr}");
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let tx = incoming_tx.clone();
        tokio::spawn(async move {
            let _permit = permit;
            match handle_one(tcp, peer_addr, acceptor, tx).await {
                Ok(()) => tracing::info!("WS upgrade complete for {peer_addr}; passed to handshake driver"),
                Err(e) => tracing::warn!("connection from {peer_addr} dropped before pairing: {e:#}"),
            }
        });
    }
}

async fn handle_one(
    tcp: tokio::net::TcpStream,
    peer_addr: SocketAddr,
    acceptor: TlsAcceptor,
    tx: mpsc::Sender<AcceptedClient>,
) -> anyhow::Result<()> {
    let tls_inner = timeout(ACCEPT_STEP_TIMEOUT, acceptor.accept(tcp))
        .await
        .map_err(|_| anyhow::anyhow!("TLS accept from {peer_addr} timed out"))?
        .map_err(|e| anyhow::anyhow!("TLS accept from {peer_addr} failed: {e}"))?;
    tracing::info!("TLS 1.3 handshake complete with {peer_addr}");
    let tls = ServerTlsStream::Server(tls_inner);

    // We sniff the WS upgrade URI inside the callback so we can pull
    // out the optional ?pin= query before tungstenite hands us the
    // upgraded stream. The Arc<Mutex> is the simplest way to smuggle
    // a value out of the synchronous callback.
    let captured_pin: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));
    let pin_slot = Arc::clone(&captured_pin);

    let callback = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        if let Some(pin) = parse_pin(req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("")) {
            *pin_slot.lock().unwrap() = Some(pin);
        }
        Ok(resp)
    };

    let ws_config = WebSocketConfig {
        max_message_size: Some(MAX_WS_MESSAGE_BYTES),
        max_frame_size: Some(MAX_WS_MESSAGE_BYTES),
        ..Default::default()
    };
    let ws = timeout(
        ACCEPT_STEP_TIMEOUT,
        tokio_tungstenite::accept_hdr_async_with_config(tls, callback, Some(ws_config)),
    )
    .await
    .map_err(|_| anyhow::anyhow!("WebSocket upgrade from {peer_addr} timed out"))??;
    let pin = captured_pin.lock().unwrap().clone();
    tx.send(AcceptedClient { peer_addr, ws, pin }).await.ok();
    Ok(())
}

fn parse_pin(path_and_query: &str) -> Option<String> {
    let (_, query) = path_and_query.split_once('?')?;
    for kv in query.split('&') {
        if let Some(v) = kv.strip_prefix("pin=") {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                hex_nibble(bytes[i + 1]),
                hex_nibble(bytes[i + 2]),
            ) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

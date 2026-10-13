//! Pairing state machine — PC side.
//!
//! The desktop's role is always the SERVER. We advertise our
//! presence on the LAN (mDNS + UDP broadcast) and accept incoming
//! TLS+WebSocket connections from the phone. When a phone dials in,
//! we drive the handshake to confirmation and persist the pair.
//!
//! The phone is always the CLIENT and lives in the Android module —
//! see `android/.../PairingViewModel.kt`.

use super::{
    emoji_code,
    handshake::{
        ConfirmBody, Envelope, Handshake, HelloBody, MismatchBody, MismatchReason, VerifyBody,
    },
};
use crate::{
    discovery::{
        advertise::{advertise_mdns, advertise_udp, MdnsHandle},
        CascadeOptions,
    },
    store::{PairedDevice, Store},
    transport::{
        server::{serve, AcceptedClient},
        tls::{cert_short_fp, own_cert_sha256},
        ws::WsChannel,
    },
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time::{timeout, Duration};

/// How long to wait for each peer message that arrives before the
/// pairing card is shown (hello, verify). The confirm wait that follows
/// has its own, longer, 60 s timeout.
const PRE_CARD_RECV_TIMEOUT: Duration = Duration::from_secs(30);

/// Events emitted to the frontend on the `pairing` event channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PairingUiEvent {
    /// Cascade phase change. `key` is one of the cascade.* string IDs.
    StatusKey { key: String },

    /// Server bound and advertising. Surface the IP+port on the
    /// idle screen so the user can verify the listener is up — and
    /// so they have something to type into the phone's manual-entry
    /// form if they need to.
    Listening {
        ip: String,
        port: u16,
        /// True if a freshly-spawned localhost probe to (ip, port)
        /// succeeded right after binding. False usually means the
        /// listener bound but the OS firewall is dropping inbound
        /// traffic — the only realistic culprit on Windows.
        firewall_ok: bool,
        /// Absolute path to run.log so the user can paste it back
        /// when pairing fails. Filled by main.rs at startup.
        log_path: String,
    },

    /// Pairing card ready. UI shows three emojis + Confirm button.
    Card {
        peer_device_name: String,
        emojis: [String; 3],
    },

    /// Manual-entry escape hatch opened on the PC. Frontend renders
    /// the PC's IP + 6-digit PIN so the user can type both on their
    /// phone's "Pair another way" form.
    ManualEntryPin { ip: String, pin: String },

    /// Successful pair.
    Paired { peer_device_name: String },

    /// Peer reported a mismatch (or our cross-check did).
    Mismatch { reason: String },
}

pub struct PairingState {
    inner: Arc<Mutex<Inner>>,
    options: CascadeOptions,
    store: Arc<Store>,
    /// Per-message timeout for the pre-card reads (hello, verify).
    /// Always `PRE_CARD_RECV_TIMEOUT` outside of tests.
    pre_card_recv_timeout: Duration,
}

struct Inner {
    ui_tx: Option<mpsc::Sender<PairingUiEvent>>,
    /// Signals the active handshake task that the user tapped Confirm.
    user_confirmed: Arc<Notify>,
    /// Signals the active handshake task that the user reported a mismatch.
    user_mismatch: Arc<Notify>,
    /// True only while a pairing card is on screen and a handshake is
    /// waiting for the user's decision. Confirm/Mismatch taps outside
    /// that window are ignored so they cannot leave a stored `Notify`
    /// permit that auto-answers a later pairing.
    card_active: bool,
    /// Holds the mDNS daemon alive for the app's lifetime. Drop ⇒
    /// unregister + shutdown.
    _mdns: Option<MdnsHandle>,
    /// 6-digit PIN currently displayed (or None if manual entry is
    /// not currently open). The server-side WS upgrade requires this
    /// PIN on `?pin=` whenever a client has come in via the manual
    /// path.
    manual_pin: Option<String>,
}

impl PairingState {
    pub fn new(store: Arc<Store>, options: CascadeOptions) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                ui_tx: None,
                user_confirmed: Arc::new(Notify::new()),
                user_mismatch: Arc::new(Notify::new()),
                card_active: false,
                _mdns: None,
                manual_pin: None,
            })),
            options,
            store,
            pre_card_recv_timeout: PRE_CARD_RECV_TIMEOUT,
        }
    }

    /// Start advertising + listening. Idempotent — calling twice in
    /// a row just replaces the UI channel.
    pub async fn start(
        self: &Arc<Self>,
        ui_tx: mpsc::Sender<PairingUiEvent>,
    ) -> anyhow::Result<()> {
        {
            let mut g = self.inner.lock().await;
            g.ui_tx = Some(ui_tx);
            // If we're being restarted, don't restart the listener
            // and advertise tasks — they're already running. Just
            // updating the UI sink is enough.
            if g._mdns.is_some() {
                return Ok(());
            }
        }

        // Generate (or load) the TLS cert and stash its short
        // fingerprint into the cascade options so mDNS / UDP
        // announces include it.
        let mut options = self.options.clone();
        let local_cert = own_cert_sha256().await?;
        options.local_cert_fp_short = cert_short_fp(&local_cert);

        // mDNS service register — kept alive for the lifetime of
        // the app via the MdnsHandle stored on Inner.
        match advertise_mdns(&options) {
            Ok(handle) => {
                let mut g = self.inner.lock().await;
                g._mdns = Some(handle);
            }
            Err(e) => {
                tracing::warn!(
                    "mDNS advertise failed: {e} — phone discovery falls back to UDP/probe/USB"
                );
            }
        }

        // UDP broadcast — keeps running until the process exits.
        let udp_opts = options.clone();
        tokio::spawn(async move {
            if let Err(e) = advertise_udp(udp_opts).await {
                tracing::warn!("UDP advertise loop exited: {e}");
            }
        });

        // TLS+WS listener. Every accepted client gets fed back here
        // on the `incoming` channel; we run the server-side handshake
        // per connection.
        let (incoming_tx, mut incoming_rx) = mpsc::channel::<AcceptedClient>(8);
        let bind_addr: SocketAddr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            options.local_tls_port,
        );
        tokio::spawn(async move {
            if let Err(e) = serve(bind_addr, incoming_tx).await {
                tracing::error!("TLS listener exited: {e}");
            }
        });

        // Self-diagnose: wait a beat for the listener to actually
        // bind, then probe localhost + LAN-IP. Surface the result so
        // the home screen can show "Listening on 192.168.x.y:31415"
        // (and tell the user when the firewall is silently dropping
        // inbound traffic).
        let me_diag = Arc::clone(self);
        let port = options.local_tls_port;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let ip = local_v4_address()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "<your-pc>".into());
            let firewall_ok = probe_listener(port).await;
            let log_path = dirs::data_local_dir()
                .map(|d| d.join("tether").join("run.log").to_string_lossy().into_owned())
                .unwrap_or_else(|| "run.log".into());
            tracing::info!(
                "diagnostic: listening on {ip}:{port}, firewall_ok={firewall_ok}, log_path={log_path}"
            );
            let _ = me_diag
                .emit(PairingUiEvent::Listening {
                    ip,
                    port,
                    firewall_ok,
                    log_path,
                })
                .await;
        });

        // Per-connection handler loop. One handshake at a time —
        // pairing is a focused, user-driven act, not a server you
        // multiplex.
        let me = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(client) = incoming_rx.recv().await {
                if let Err(e) = me.handle_client(client).await {
                    tracing::warn!("pairing handshake aborted: {e}");
                    let _ = me
                        .emit(PairingUiEvent::Mismatch {
                            reason: "protocol".into(),
                        })
                        .await;
                }
            }
        });

        Ok(())
    }

    /// Drive the handshake with one accepted client end-to-end.
    async fn handle_client(&self, client: AcceptedClient) -> anyhow::Result<()> {
        let peer_addr = client.peer_addr;
        tracing::info!("starting handshake with {peer_addr}");
        // PIN gate (only when the manual-entry path is currently
        // open). Cleartext compare is fine — the PIN is a throw-away
        // 6-digit value displayed on screen.
        {
            let g = self.inner.lock().await;
            if let Some(expected) = g.manual_pin.as_ref() {
                match client.pin.as_ref() {
                    Some(got) if got == expected => {
                        tracing::info!("{peer_addr} supplied correct PIN");
                    }
                    _ => {
                        tracing::warn!("{peer_addr} missing/wrong PIN; rejecting");
                        anyhow::bail!("incoming client missing or wrong PIN");
                    }
                }
            }
        }

        let mut ws = WsChannel::from_accepted(client.ws);
        let local_cert_fp = own_cert_sha256().await?;
        let result = self.drive_handshake(&mut ws, local_cert_fp).await;
        {
            // Whatever happened, no card is waiting on the user any more.
            let mut g = self.inner.lock().await;
            g.card_active = false;
        }
        match &result {
            Ok(()) => tracing::info!("handshake with {peer_addr} reached paired state"),
            Err(e) => tracing::warn!("handshake with {peer_addr} failed: {e:#}"),
        }
        result
    }

    /// Generic handshake driver shared by server and (future) client
    /// paths. Sends hello, receives hello, derives verifier, sends
    /// verify, receives verify, surfaces card, waits for both
    /// confirms, persists.
    async fn drive_handshake<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        &self,
        ws: &mut WsChannel<S>,
        local_cert_fp: Vec<u8>,
    ) -> anyhow::Result<()> {
        // 1. hello (send ours)
        let handshake = Handshake::new();
        let local_pub = handshake.local_pub.as_bytes().to_vec();
        let hello = Envelope {
            v: crate::TETHER_PROTOCOL_VERSION,
            kind: "hello".into(),
            id: 1,
            in_reply_to: None,
            body: HelloBody {
                device_type: self.options.local_device_type.clone(),
                device_name: self.options.local_device_name.clone(),
                protocol_version: 1,
                ecdh_pubkey: local_pub.clone(),
                tls_cert_sha256: local_cert_fp.clone(),
            },
        };
        ws.send_cbor(&hello).await?;
        tracing::debug!("sent hello");

        // 2. hello (receive theirs). A peer that finishes the upgrade
        // and then goes silent must not stall the (serial) handshake
        // loop, so every pre-card read is time-boxed.
        let peer_hello: Envelope<HelloBody> = timeout(self.pre_card_recv_timeout, ws.recv_cbor())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for peer hello"))??
            .ok_or_else(|| anyhow::anyhow!("connection closed before peer hello"))?;
        tracing::info!(
            "received hello from {} ({})",
            peer_hello.body.device_name,
            peer_hello.body.device_type
        );
        if peer_hello.body.protocol_version != 1 || peer_hello.v != 1 {
            return Err(anyhow::anyhow!(
                "protocol version mismatch (peer v={}, body protocol_version={})",
                peer_hello.v,
                peer_hello.body.protocol_version
            ));
        }

        // 3. derive verifier
        let verifier = handshake.derive(&peer_hello.body.ecdh_pubkey)?;
        let indices = emoji_code::indices_from_verifier(&verifier);
        let emojis_static = emoji_code::from_verifier(&verifier);
        let emojis = [
            emojis_static[0].to_string(),
            emojis_static[1].to_string(),
            emojis_static[2].to_string(),
        ];

        // 4. verify (send ours)
        let verify_msg = Envelope {
            v: 1,
            kind: "verify".into(),
            id: 2,
            in_reply_to: Some(peer_hello.id),
            body: VerifyBody {
                fingerprint: verifier.to_vec(),
                emoji_indices: indices,
                device_name: self.options.local_device_name.clone(),
            },
        };
        ws.send_cbor(&verify_msg).await?;
        tracing::debug!("sent verify (emoji indices: {:?})", indices);

        // 5. verify (receive theirs) + cross-check
        let peer_verify: Envelope<VerifyBody> = timeout(self.pre_card_recv_timeout, ws.recv_cbor())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for peer verify"))??
            .ok_or_else(|| anyhow::anyhow!("connection closed before peer verify"))?;
        if peer_verify.body.fingerprint != verifier.to_vec() {
            tracing::warn!("peer verifier disagrees — aborting (MITM?)");
            self.emit(PairingUiEvent::Mismatch {
                reason: "protocol".into(),
            })
            .await
            .ok();
            return Err(anyhow::anyhow!("peer verifier cross-check failed"));
        }
        tracing::info!("verifier agreement: emoji indices match peer's");

        // 6. Card → wait for both confirms (60 s)
        //
        // Take fresh Notify objects for THIS handshake before the card
        // is shown: any permit left over from an earlier tap (peer
        // dropped just before the click, double tap) is discarded, and
        // taps are only accepted from here on (card_active).
        let (user_confirmed, user_mismatch) = {
            let mut g = self.inner.lock().await;
            g.user_confirmed = Arc::new(Notify::new());
            g.user_mismatch = Arc::new(Notify::new());
            g.card_active = true;
            (g.user_confirmed.clone(), g.user_mismatch.clone())
        };

        self.emit(PairingUiEvent::Card {
            peer_device_name: peer_verify.body.device_name.clone(),
            emojis,
        })
        .await?;
        tracing::info!("pairing card shown — waiting for both confirms");

        let confirm_result = timeout(Duration::from_secs(60), async {
            let mut peer_confirm_received = false;
            let mut local_confirm_sent = false;
            loop {
                tokio::select! {
                    _ = user_confirmed.notified(), if !local_confirm_sent => {
                        let confirm = Envelope {
                            v: 1, kind: "confirm".into(), id: 3,
                            in_reply_to: Some(peer_verify.id),
                            body: ConfirmBody { confirmed: true },
                        };
                        ws.send_cbor(&confirm).await?;
                        local_confirm_sent = true;
                        if peer_confirm_received { break; }
                    }
                    _ = user_mismatch.notified() => {
                        let m = Envelope {
                            v: 1, kind: "mismatch".into(), id: 4,
                            in_reply_to: Some(peer_verify.id),
                            body: MismatchBody {
                                reason: MismatchReason::UserMismatch.wire().into(),
                            },
                        };
                        let _ = ws.send_cbor(&m).await;
                        return Err(anyhow::anyhow!("user reported mismatch"));
                    }
                    msg = ws.recv_envelope() => {
                        match msg? {
                            Some(env) if env.kind == "confirm" => {
                                peer_confirm_received = true;
                                if local_confirm_sent { break; }
                            }
                            Some(env) if env.kind == "mismatch" => {
                                let mb: MismatchBody = ciborium::from_reader(&env.body[..])
                                    .unwrap_or(MismatchBody { reason: "protocol".into() });
                                return Err(anyhow::anyhow!("peer mismatch: {}", mb.reason));
                            }
                            Some(env) => {
                                tracing::warn!("unexpected message during confirm: {}", env.kind);
                            }
                            None => return Err(anyhow::anyhow!("peer closed during confirm")),
                        }
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await;

        match confirm_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                self.emit(PairingUiEvent::Mismatch {
                    reason: "user_mismatch".into(),
                })
                .await
                .ok();
                return Err(e);
            }
            Err(_) => {
                let m = Envelope {
                    v: 1, kind: "mismatch".into(), id: 5,
                    in_reply_to: None,
                    body: MismatchBody {
                        reason: MismatchReason::Timeout.wire().into(),
                    },
                };
                let _ = ws.send_cbor(&m).await;
                self.emit(PairingUiEvent::Mismatch {
                    reason: "timeout".into(),
                })
                .await
                .ok();
                return Err(anyhow::anyhow!("confirm timed out"));
            }
        }

        // 7. Persist + close out manual entry if it was open.
        let device = PairedDevice {
            peer_device_type: peer_hello.body.device_type.clone(),
            peer_device_name: peer_verify.body.device_name.clone(),
            peer_x25519_pubkey: peer_hello.body.ecdh_pubkey.clone(),
            // Server side: we never received the peer's TLS cert
            // because we never asked for client certs during initial
            // pair. We persist what the peer told us in hello;
            // mutual-TLS reconnect will require the phone to present
            // a cert whose SHA-256 matches.
            peer_tls_cert_sha256: peer_hello.body.tls_cert_sha256.clone(),
            paired_at: now_secs(),
        };
        self.store.add_paired(device).await?;

        {
            let mut g = self.inner.lock().await;
            g.manual_pin = None;
        }

        self.emit(PairingUiEvent::Paired {
            peer_device_name: peer_verify.body.device_name,
        })
        .await
        .ok();
        Ok(())
    }

    pub async fn user_confirm(&self) -> anyhow::Result<()> {
        let g = self.inner.lock().await;
        if g.card_active {
            g.user_confirmed.notify_one();
        } else {
            tracing::debug!("confirm ignored: no pairing card is active");
        }
        Ok(())
    }

    pub async fn user_mismatch(&self) -> anyhow::Result<()> {
        let g = self.inner.lock().await;
        if g.card_active {
            g.user_mismatch.notify_one();
        } else {
            tracing::debug!("mismatch ignored: no pairing card is active");
        }
        Ok(())
    }

    /// "Pair another way" on the PC: generate a 6-digit PIN, store
    /// it, and tell the UI to display IP + PIN so the user can type
    /// both on their phone.
    pub async fn open_manual_entry(&self) -> anyhow::Result<()> {
        let pin = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000u32));
        {
            let mut g = self.inner.lock().await;
            g.manual_pin = Some(pin.clone());
        }
        let ip = local_v4_address()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "<your-pc>".into());
        self.emit(PairingUiEvent::ManualEntryPin { ip, pin }).await
    }

    /// PC has no manual-entry "submit" — the phone submits and dials
    /// the listener. Kept as a no-op so the Tauri command surface
    /// stays stable for older frontends.
    pub async fn submit_manual(&self, _address: String, _pin: String) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn reset(&self) -> anyhow::Result<()> {
        let mut g = self.inner.lock().await;
        g.manual_pin = None;
        Ok(())
    }

    async fn emit(&self, evt: PairingUiEvent) -> anyhow::Result<()> {
        let g = self.inner.lock().await;
        if let Some(tx) = g.ui_tx.as_ref() {
            let _ = tx.send(evt).await;
        }
        Ok(())
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Best-effort first non-loopback IPv4 address on this host. Used
/// by the manual-entry display. Cheap trick: open a UDP "connection"
/// to a public address — the OS picks the egress interface, then we
/// read the bound local addr. No packets actually leave the box.
fn local_v4_address() -> Option<std::net::Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let addr = sock.local_addr().ok()?;
    if let std::net::IpAddr::V4(v4) = addr.ip() {
        if !v4.is_loopback() {
            return Some(v4);
        }
    }
    None
}

/// Probe whether the TLS listener is actually reachable from
/// outside-the-process. We TCP-connect to our own LAN IP (not
/// 127.0.0.1, which always works regardless of firewall) and look
/// for either "connected" or "refused" — anything other than "we
/// got there" means a firewall is dropping our inbound port.
async fn probe_listener(port: u16) -> bool {
    let ip = match local_v4_address() {
        Some(v) => v,
        None => return false,
    };
    let addr = SocketAddr::new(std::net::IpAddr::V4(ip), port);
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_millis(800),
            tokio::net::TcpStream::connect(addr),
        )
        .await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::handshake::HelloBody;
    use std::path::PathBuf;
    use tokio::io::{duplex, DuplexStream};
    use tokio_tungstenite::{accept_async, client_async, WebSocketStream};

    async fn ws_pair() -> (WebSocketStream<DuplexStream>, WebSocketStream<DuplexStream>) {
        let (a, b) = duplex(64 * 1024);
        let (srv, cli) = tokio::join!(
            accept_async(a),
            client_async("ws://tether.local/tether/v1", b)
        );
        (srv.unwrap(), cli.unwrap().0)
    }

    fn test_state(tag: &str, pre_card: Duration) -> (Arc<PairingState>, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "tether-state-test-{}-{tag}.json",
            std::process::id()
        ));
        let store = Arc::new(Store::open_at(path.clone()));
        let mut st = PairingState::new(store, CascadeOptions::default());
        st.pre_card_recv_timeout = pre_card;
        (Arc::new(st), path)
    }

    fn peer_hello(pubkey: Vec<u8>) -> Envelope<HelloBody> {
        Envelope {
            v: 1,
            kind: "hello".into(),
            id: 1,
            in_reply_to: None,
            body: HelloBody {
                device_type: "phone".into(),
                device_name: "test-phone".into(),
                protocol_version: 1,
                ecdh_pubkey: pubkey,
                tls_cert_sha256: vec![9u8; 32],
            },
        }
    }

    /// A Confirm/Mismatch tap with no card on screen (or a permit left
    /// over from an earlier card) must not answer a later pairing.
    #[tokio::test]
    async fn stale_confirm_does_not_auto_confirm_next_handshake() {
        let (state, path) = test_state("stale", Duration::from_secs(5));
        let (ui_tx, mut ui_rx) = mpsc::channel::<PairingUiEvent>(32);
        state.inner.lock().await.ui_tx = Some(ui_tx);

        // Taps while no handshake is waiting: must be ignored...
        state.user_confirm().await.unwrap();
        state.user_mismatch().await.unwrap();
        // ...and even a permit that somehow got stored must be discarded
        // when the next handshake starts.
        {
            let g = state.inner.lock().await;
            g.user_confirmed.notify_one();
            g.user_mismatch.notify_one();
        }

        let (srv, cli) = ws_pair().await;
        let st = Arc::clone(&state);
        let driver = tokio::spawn(async move {
            let mut ws = WsChannel::from_accepted(srv);
            st.drive_handshake(&mut ws, vec![7u8; 32]).await
        });

        let mut peer = WsChannel::from_accepted(cli);
        let hs = Handshake::new();
        let peer_pub = hs.local_pub.as_bytes().to_vec();
        let pc_hello: Envelope<HelloBody> = peer.recv_cbor().await.unwrap().unwrap();
        peer.send_cbor(&peer_hello(peer_pub)).await.unwrap();
        let verifier = hs.derive(&pc_hello.body.ecdh_pubkey).unwrap();
        let pc_verify: Envelope<VerifyBody> = peer.recv_cbor().await.unwrap().unwrap();
        let verify = Envelope {
            v: 1,
            kind: "verify".into(),
            id: 2,
            in_reply_to: Some(pc_hello.id),
            body: VerifyBody {
                fingerprint: verifier.to_vec(),
                emoji_indices: emoji_code::indices_from_verifier(&verifier),
                device_name: "test-phone".into(),
            },
        };
        peer.send_cbor(&verify).await.unwrap();

        // Wait for the card to be shown.
        loop {
            match ui_rx.recv().await.expect("ui channel closed") {
                PairingUiEvent::Card { .. } => break,
                PairingUiEvent::Mismatch { reason } => {
                    panic!("handshake auto-aborted before the card: {reason}")
                }
                _ => continue,
            }
        }

        // No confirm may be sent until the user actually taps Confirm.
        let early = timeout(Duration::from_millis(400), peer.recv_cbor::<ConfirmBody>()).await;
        assert!(early.is_err(), "handshake answered without a user tap");

        // A real tap now confirms normally.
        state.user_confirm().await.unwrap();
        let pc_confirm: Envelope<ConfirmBody> = timeout(Duration::from_secs(2), peer.recv_cbor())
            .await
            .expect("no confirm after a real tap")
            .unwrap()
            .unwrap();
        assert_eq!(pc_confirm.kind, "confirm");
        peer.send_cbor(&Envelope {
            v: 1,
            kind: "confirm".into(),
            id: 3,
            in_reply_to: Some(pc_verify.id),
            body: ConfirmBody { confirmed: true },
        })
        .await
        .unwrap();

        driver.await.unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
    }

    /// A peer that finishes the WebSocket upgrade and then stays
    /// silent must be dropped by the pre-card timeout, not hang forever.
    #[tokio::test]
    async fn silent_peer_is_dropped_by_pre_card_timeout() {
        let (state, path) = test_state("silent", Duration::from_millis(200));
        let (srv, _cli_kept_open_but_silent) = ws_pair().await;
        let st = Arc::clone(&state);
        let driver = tokio::spawn(async move {
            let mut ws = WsChannel::from_accepted(srv);
            st.drive_handshake(&mut ws, vec![7u8; 32]).await
        });
        let res = timeout(Duration::from_secs(3), driver)
            .await
            .expect("drive_handshake hung on a silent peer")
            .unwrap();
        let err = res.expect_err("silent peer must not pair");
        assert!(format!("{err:#}").contains("timed out"), "got: {err:#}");
        let _ = std::fs::remove_file(&path);
    }
}

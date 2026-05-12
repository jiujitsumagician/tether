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
}

struct Inner {
    ui_tx: Option<mpsc::Sender<PairingUiEvent>>,
    /// Signals the active handshake task that the user tapped Confirm.
    user_confirmed: Arc<Notify>,
    /// Signals the active handshake task that the user reported a mismatch.
    user_mismatch: Arc<Notify>,
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
                _mdns: None,
                manual_pin: None,
            })),
            options,
            store,
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
            let _ = me_diag
                .emit(PairingUiEvent::Listening {
                    ip,
                    port,
                    firewall_ok,
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
        // PIN gate (only when the manual-entry path is currently
        // open). Cleartext compare is fine — the PIN is a throw-away
        // 6-digit value displayed on screen.
        {
            let g = self.inner.lock().await;
            if let Some(expected) = g.manual_pin.as_ref() {
                match client.pin.as_ref() {
                    Some(got) if got == expected => {}
                    _ => {
                        anyhow::bail!("incoming client missing or wrong PIN");
                    }
                }
            }
        }

        let mut ws = WsChannel::from_accepted(client.ws);
        let local_cert_fp = own_cert_sha256().await?;
        self.drive_handshake(&mut ws, local_cert_fp).await
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

        // 2. hello (receive theirs)
        let peer_hello: Envelope<HelloBody> = ws
            .recv_cbor()
            .await?
            .ok_or_else(|| anyhow::anyhow!("connection closed before peer hello"))?;
        if peer_hello.body.protocol_version != 1 || peer_hello.v != 1 {
            return Err(anyhow::anyhow!("protocol version mismatch"));
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

        // 5. verify (receive theirs) + cross-check
        let peer_verify: Envelope<VerifyBody> = ws
            .recv_cbor()
            .await?
            .ok_or_else(|| anyhow::anyhow!("connection closed before peer verify"))?;
        if peer_verify.body.fingerprint != verifier.to_vec() {
            self.emit(PairingUiEvent::Mismatch {
                reason: "protocol".into(),
            })
            .await
            .ok();
            return Err(anyhow::anyhow!("peer verifier cross-check failed"));
        }

        // 6. Card → wait for both confirms (60 s)
        self.emit(PairingUiEvent::Card {
            peer_device_name: peer_verify.body.device_name.clone(),
            emojis,
        })
        .await?;

        let (user_confirmed, user_mismatch) = {
            let g = self.inner.lock().await;
            (g.user_confirmed.clone(), g.user_mismatch.clone())
        };

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
        g.user_confirmed.notify_one();
        Ok(())
    }

    pub async fn user_mismatch(&self) -> anyhow::Result<()> {
        let g = self.inner.lock().await;
        g.user_mismatch.notify_one();
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

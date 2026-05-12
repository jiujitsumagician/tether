//! Pairing state machine — orchestrates the cascade, the TLS dial,
//! the handshake, the verify/confirm dance, and persistence.
//!
//! Frontend talks to this through a handful of Tauri commands; events
//! flow back through a single `pairing` event stream.

use super::{
    emoji_code,
    handshake::{ConfirmBody, Envelope, HelloBody, MismatchBody, MismatchReason, VerifyBody, Handshake},
};
use crate::{
    discovery::{run_cascade, CascadeEvent, CascadeOptions, DiscoveredPeer},
    store::{PairedDevice, Store},
    transport::tls::TlsClient,
    transport::ws::WsChannel,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time::{timeout, Duration};

/// Everything the UI listens for on the `pairing` event channel.
/// Keep this enum stable — the frontend switches on `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PairingUiEvent {
    /// Cascade phase change. `key` is one of the cascade.* string IDs.
    StatusKey { key: String },

    /// Pairing card ready to display. UI now shows the three emojis
    /// and the Confirm button (disabled until peer confirms — but
    /// the user can already tap; we record the local Confirm intent
    /// and emit `Paired` once the peer's confirm arrives).
    Card {
        peer_device_name: String,
        emojis: [&'static str; 3],
    },

    /// Manual-entry escape hatch opened. Frontend renders the IP +
    /// PIN form.
    ManualEntryOpen,

    /// Successful pair.
    Paired { peer_device_name: String },

    /// Peer reported a mismatch (or our cross-check did).
    Mismatch { reason: String },

    /// Cascade exhausted without finding a peer at all.
    Exhausted,
}

pub struct PairingState {
    inner: Arc<Mutex<Inner>>,
    options: CascadeOptions,
    store: Arc<Store>,
    _app: AppHandle,
}

struct Inner {
    ui_tx: Option<mpsc::Sender<PairingUiEvent>>,
    local_confirmed: bool,
    peer_confirmed: bool,
    peer_name: Option<String>,
    /// Signals the handshake task that the user tapped Confirm.
    user_confirmed: Arc<Notify>,
    /// Signals the handshake task that the user reported a mismatch.
    user_mismatch: Arc<Notify>,
}

impl PairingState {
    pub fn new(app: AppHandle, store: Arc<Store>, options: CascadeOptions) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                ui_tx: None,
                local_confirmed: false,
                peer_confirmed: false,
                peer_name: None,
                user_confirmed: Arc::new(Notify::new()),
                user_mismatch: Arc::new(Notify::new()),
            })),
            options,
            store,
            _app: app,
        }
    }

    pub async fn start(
        self: &Arc<Self>,
        ui_tx: mpsc::Sender<PairingUiEvent>,
    ) -> anyhow::Result<()> {
        {
            let mut g = self.inner.lock().await;
            g.ui_tx = Some(ui_tx);
            g.local_confirmed = false;
            g.peer_confirmed = false;
            g.peer_name = None;
        }

        // Spawn the cascade + handshake driver.
        let me = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = me.run().await {
                tracing::warn!("pairing run failed: {e}");
                let _ = me
                    .emit(PairingUiEvent::Mismatch {
                        reason: "protocol".into(),
                    })
                    .await;
            }
        });
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

    pub async fn open_manual_entry(&self) -> anyhow::Result<()> {
        self.emit(PairingUiEvent::ManualEntryOpen).await
    }

    pub async fn submit_manual(&self, address: String, pin: String) -> anyhow::Result<()> {
        // The manual-entry path is intentionally minimal — produce a
        // `DiscoveredPeer` from typed input and route through the
        // same TLS+handshake code the cascade uses. The 6-digit PIN
        // gates the transport via a server-side check; this stub
        // forwards it as a TLS SNI hint so the server can validate
        // against the PIN it's currently displaying.
        let socket: std::net::SocketAddr = address.parse()?;
        let peer = DiscoveredPeer {
            device_type: "phone".into(),
            device_name: format!("{}", socket),
            socket,
            cert_fp_short: String::new(),
            via: crate::discovery::DiscoveryMethod::Manual,
        };
        // PIN is forwarded as part of the WS upgrade query string.
        // Server validates and either accepts the upgrade or returns
        // 401, which the WS client surfaces as an error here.
        self.run_with_peer(peer, Some(pin)).await
    }

    pub async fn reset(&self) -> anyhow::Result<()> {
        let mut g = self.inner.lock().await;
        g.local_confirmed = false;
        g.peer_confirmed = false;
        g.peer_name = None;
        Ok(())
    }

    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        // Stream cascade events → UI status keys.
        let (cascade_tx, mut cascade_rx) = mpsc::channel::<CascadeEvent>(32);
        let me_for_events = Arc::clone(&self);
        let event_pump = tokio::spawn(async move {
            while let Some(evt) = cascade_rx.recv().await {
                match evt {
                    CascadeEvent::Phase { key } => {
                        let _ = me_for_events
                            .emit(PairingUiEvent::StatusKey { key })
                            .await;
                    }
                    CascadeEvent::Exhausted => {
                        let _ = me_for_events.emit(PairingUiEvent::Exhausted).await;
                    }
                    CascadeEvent::Found { .. } => {
                        // The cascade also returns the peer via its
                        // direct return path; here we only forward
                        // the status updates.
                    }
                }
            }
        });

        let peer = run_cascade(self.options.clone(), cascade_tx)
            .await?
            .ok_or_else(|| anyhow::anyhow!("cascade exhausted"))?;
        event_pump.abort();

        self.run_with_peer(peer, None).await
    }

    /// Bring up TLS + WebSocket + handshake against a known peer. The
    /// optional `pin` parameter is sent as a query-string token on
    /// the WebSocket upgrade for the manual-entry path.
    async fn run_with_peer(
        &self,
        peer: DiscoveredPeer,
        pin: Option<String>,
    ) -> anyhow::Result<()> {
        // 1. TLS-dial.
        let tls = TlsClient::dial(&peer.socket, &peer.cert_fp_short).await?;
        let peer_cert_fp = tls.peer_cert_sha256().to_vec();

        // 2. WebSocket upgrade. The pin (if any) goes as ?pin=...
        let mut ws = WsChannel::upgrade(tls, pin.as_deref()).await?;

        // 3. Send hello.
        let handshake = Handshake::new();
        let local_pub = handshake.local_pub.as_bytes().to_vec();
        let local_cert_fp = crate::transport::tls::own_cert_sha256().await?;

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

        // 4. Receive peer hello.
        let peer_hello: Envelope<HelloBody> = ws
            .recv_cbor()
            .await?
            .ok_or_else(|| anyhow::anyhow!("connection closed before peer hello"))?;
        if peer_hello.body.protocol_version != 1 || peer_hello.v != 1 {
            return Err(anyhow::anyhow!("protocol version mismatch"));
        }

        // Cross-check the cert fingerprint TLS gave us against what
        // the peer reports.
        if peer_hello.body.tls_cert_sha256 != peer_cert_fp {
            self.emit(PairingUiEvent::Mismatch {
                reason: "protocol".into(),
            })
            .await
            .ok();
            return Err(anyhow::anyhow!("peer cert fp mismatch (MITM?)"));
        }

        // 5. Derive verifier, send + receive verify.
        let verifier = handshake.derive(&peer_hello.body.ecdh_pubkey)?;
        let indices = emoji_code::indices_from_verifier(&verifier);
        let emojis = emoji_code::from_verifier(&verifier);

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

        // 6. Surface the pairing card to the user. From here we wait
        //    for BOTH the local user's confirm tap AND the peer's
        //    confirm message before persisting.
        self.emit(PairingUiEvent::Card {
            peer_device_name: peer_verify.body.device_name.clone(),
            emojis,
        })
        .await?;

        let user_confirmed = {
            let g = self.inner.lock().await;
            g.user_confirmed.clone()
        };
        let user_mismatch = {
            let g = self.inner.lock().await;
            g.user_mismatch.clone()
        };

        // Wait up to 60s for both sides to confirm (or for either
        // side to report a mismatch).
        let result = timeout(Duration::from_secs(60), async {
            // Spawn a peer-reader because we need to listen on the WS
            // while simultaneously waiting on user input.
            let mut peer_confirm_received = false;
            let mut local_confirm_sent = false;

            loop {
                tokio::select! {
                    _ = user_confirmed.notified(), if !local_confirm_sent => {
                        let confirm = Envelope {
                            v: 1,
                            kind: "confirm".into(),
                            id: 3,
                            in_reply_to: Some(peer_verify.id),
                            body: ConfirmBody { confirmed: true },
                        };
                        ws.send_cbor(&confirm).await?;
                        local_confirm_sent = true;
                        if peer_confirm_received { break; }
                    }
                    _ = user_mismatch.notified() => {
                        let m = Envelope {
                            v: 1,
                            kind: "mismatch".into(),
                            id: 4,
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
                                tracing::warn!("unexpected message: {}", env.kind);
                            }
                            None => return Err(anyhow::anyhow!("peer closed during confirm")),
                        }
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await;

        match result {
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
                    v: 1,
                    kind: "mismatch".into(),
                    id: 5,
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

        // 7. Persist.
        let device = PairedDevice {
            peer_device_type: peer_hello.body.device_type.clone(),
            peer_device_name: peer_verify.body.device_name.clone(),
            peer_x25519_pubkey: peer_hello.body.ecdh_pubkey.clone(),
            peer_tls_cert_sha256: peer_cert_fp.clone(),
            paired_at: chrono_secs_since_epoch(),
        };
        self.store.add_paired(device).await?;
        self.emit(PairingUiEvent::Paired {
            peer_device_name: peer_verify.body.device_name,
        })
        .await
        .ok();
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

fn chrono_secs_since_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

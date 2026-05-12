//! Auto-cascading discovery chain.
//!
//! Five phases:
//!   1. mDNS  — 5s timeout
//!   2. UDP broadcast — 3s timeout
//!   3. Subnet probe  — 2s timeout
//!   4. USB / ADB     — until cable plugged or user gives up
//!   5. Hotspot prompt — manual opt-in
//!
//! Each phase emits `CascadeEvent`s on a channel; the pairing state
//! machine picks the first `Found` and tears the cascade down.

pub mod advertise;
pub mod mdns;
pub mod subnet_probe;
pub mod udp_broadcast;
pub mod usb;

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Endpoint discovered by any phase. The pairing state machine treats
/// every variant the same — once we have one of these we tear the
/// cascade down and TLS-dial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub device_type: String,
    pub device_name: String,
    pub socket: SocketAddr,
    pub cert_fp_short: String,
    pub via: DiscoveryMethod,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    Mdns,
    UdpBroadcast,
    SubnetProbe,
    Usb,
    Manual,
}

/// Events emitted by the cascade. The UI binds these directly to the
/// status-line string in `desktop/src/ui/strings.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CascadeEvent {
    Phase {
        /// Status-line key. Matches `cascade.mdns`, `cascade.fallback`,
        /// `cascade.usb.prompt`, `cascade.usb.detected`,
        /// `cascade.usb.debug`, `cascade.hotspot`.
        key: String,
    },
    Found {
        peer: DiscoveredPeer,
    },
    Exhausted,
}

#[derive(Debug, Clone)]
pub struct CascadeOptions {
    pub mdns_timeout: Duration,
    pub udp_timeout: Duration,
    pub subnet_timeout: Duration,
    pub udp_port: u16,
    pub local_device_name: String,
    pub local_device_type: String,
    pub local_tls_port: u16,
    pub local_cert_fp_short: String,
}

impl Default for CascadeOptions {
    fn default() -> Self {
        Self {
            mdns_timeout: Duration::from_secs(5),
            udp_timeout: Duration::from_secs(3),
            subnet_timeout: Duration::from_secs(2),
            udp_port: crate::TETHER_UDP_PORT,
            local_device_name: default_device_name(),
            local_device_type: "pc".into(),
            local_tls_port: crate::TETHER_DEFAULT_TLS_PORT,
            local_cert_fp_short: String::new(),
        }
    }
}

fn default_device_name() -> String {
    if let Ok(host) = hostname_default() {
        return host;
    }
    "This PC".into()
}

#[cfg(target_os = "windows")]
fn hostname_default() -> std::io::Result<String> {
    std::env::var("COMPUTERNAME")
        .map_err(|_| std::io::Error::other("no COMPUTERNAME"))
}

#[cfg(not(target_os = "windows"))]
fn hostname_default() -> std::io::Result<String> {
    std::process::Command::new("hostname")
        .output()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                Err(std::io::Error::other("empty hostname"))
            } else {
                Ok(s)
            }
        })
}

/// Drive the cascade end-to-end on a single task. Events flow back on
/// `tx`. Returning `Ok(Some(_))` means we discovered a peer; `Ok(None)`
/// means every wireless+USB phase exhausted with nothing found.
pub async fn run_cascade(
    options: CascadeOptions,
    tx: mpsc::Sender<CascadeEvent>,
) -> anyhow::Result<Option<DiscoveredPeer>> {
    macro_rules! emit {
        ($evt:expr) => {
            let _ = tx.send($evt).await;
        };
    }

    // Phase 1 — mDNS
    emit!(CascadeEvent::Phase {
        key: "cascade.mdns".into(),
    });
    if let Some(peer) = timeout(options.mdns_timeout, mdns::run(&options))
        .await
        .ok()
        .and_then(|r| r.ok().flatten())
    {
        emit!(CascadeEvent::Found { peer: peer.clone() });
        return Ok(Some(peer));
    }

    // Phase 2 — UDP broadcast
    emit!(CascadeEvent::Phase {
        key: "cascade.fallback".into(),
    });
    if let Some(peer) = timeout(options.udp_timeout, udp_broadcast::run(&options))
        .await
        .ok()
        .and_then(|r| r.ok().flatten())
    {
        emit!(CascadeEvent::Found { peer: peer.clone() });
        return Ok(Some(peer));
    }

    // Phase 3 — subnet probe (same status line; the user doesn't need
    // a breakdown of which fallback they're on).
    if let Some(peer) = timeout(options.subnet_timeout, subnet_probe::run(&options))
        .await
        .ok()
        .and_then(|r| r.ok().flatten())
    {
        emit!(CascadeEvent::Found { peer: peer.clone() });
        return Ok(Some(peer));
    }

    // Phase 4 — USB / ADB. Runs indefinitely until cable plugged or
    // the user opts out via the manual-entry escape hatch (which
    // cancels the cascade by dropping the receiver).
    emit!(CascadeEvent::Phase {
        key: "cascade.usb.prompt".into(),
    });
    match usb::run(&options, tx.clone()).await {
        Ok(Some(peer)) => {
            emit!(CascadeEvent::Found { peer: peer.clone() });
            Ok(Some(peer))
        }
        Ok(None) => {
            emit!(CascadeEvent::Exhausted);
            Ok(None)
        }
        Err(e) => {
            tracing::warn!("USB phase errored: {e}");
            emit!(CascadeEvent::Exhausted);
            Ok(None)
        }
    }
}

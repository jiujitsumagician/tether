//! Tether desktop core.
//!
//! Exposed as both a Tauri-driven binary and a plain library so the
//! unit + integration tests can drive the discovery cascade and the
//! handshake without spinning up a webview.

pub mod discovery;
pub mod pairing;
pub mod store;
pub mod transport;

/// Fixed UDP port for broadcast + subnet-probe announcements. Same
/// constant compiled into the Android counterpart.
pub const TETHER_UDP_PORT: u16 = 31413;

/// Default TCP port for TLS-wrapped WebSocket transport. Real port is
/// announced via mDNS / broadcast — apps don't assume.
pub const TETHER_DEFAULT_TLS_PORT: u16 = 31415;

/// Magic header on every UDP discovery packet. Any packet not
/// starting with this exact 8-byte sequence is dropped without
/// parsing — keeps the listener cheap even on a noisy LAN.
pub const TETHER_MAGIC: &[u8] = b"TETHER1\n";

/// Protocol version. Hard-bumped on any wire change; receivers refuse
/// `v != 1` with `mismatch { reason: "protocol" }`.
pub const TETHER_PROTOCOL_VERSION: u32 = 1;

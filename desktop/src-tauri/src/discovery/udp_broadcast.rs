//! UDP broadcast — phase 2 of the cascade.
//!
//! Sends an announce packet to 255.255.255.255:<udp_port> every 800ms
//! and listens on the same port. First non-self announce wins.

use super::{CascadeOptions, DiscoveredPeer, DiscoveryMethod};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

pub async fn run(options: &CascadeOptions) -> anyhow::Result<Option<DiscoveredPeer>> {
    // Bind 0.0.0.0:<udp_port>, enable broadcast + reuse-addr. We bind
    // to the fixed port (not :0) so the peer's announce reaches us
    // via the same socket we send from.
    let sock = bind_listener(options.udp_port)?;
    sock.set_broadcast(true)?;

    let send_payload = build_announce(options);
    let broadcast_addr: SocketAddr = format!("255.255.255.255:{}", options.udp_port).parse()?;

    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<DiscoveredPeer>(1);

    // Sender loop — broadcast every 800ms.
    let send_sock = sock.clone();
    let send_addr = broadcast_addr;
    let send_payload_clone = send_payload.clone();
    let send_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(800));
        loop {
            interval.tick().await;
            if let Err(e) = send_sock.send_to(&send_payload_clone, send_addr).await {
                tracing::debug!("UDP broadcast send failed: {e}");
            }
        }
    });

    // Receiver loop — keep until first valid peer.
    let our_type = options.local_device_type.clone();
    let our_name = options.local_device_name.clone();
    let recv_sock = sock.clone();
    let recv_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        loop {
            let (n, from) = match recv_sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!("UDP recv failed: {e}");
                    continue;
                }
            };
            if let Some(peer) = parse_announce(&buf[..n], from, &our_type, &our_name) {
                let _ = peer_tx.send(peer).await;
                break;
            }
        }
    });

    let result = peer_rx.recv().await;
    send_task.abort();
    recv_task.abort();
    Ok(result)
}

fn bind_listener(port: u16) -> std::io::Result<std::sync::Arc<UdpSocket>> {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr as Std};
    let domain = socket2::Domain::IPV4;
    let socket = socket2::Socket::new(domain, socket2::Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    let addr: Std = Std::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    let std_sock: std::net::UdpSocket = socket.into();
    let tokio_sock = UdpSocket::from_std(std_sock)?;
    Ok(std::sync::Arc::new(tokio_sock))
}

fn build_announce(options: &CascadeOptions) -> Vec<u8> {
    use rand::Rng;
    let nonce: u32 = rand::thread_rng().gen();
    let payload = format!(
        "TETHER1\ntype=announce\ndevice_type={}\ndevice_name={}\nport={}\ncert_fp_short={}\nnonce={:08x}\n",
        options.local_device_type,
        options.local_device_name.replace('\n', " "),
        options.local_tls_port,
        options.local_cert_fp_short,
        nonce,
    );
    payload.into_bytes()
}

fn parse_announce(
    bytes: &[u8],
    from: SocketAddr,
    our_device_type: &str,
    our_device_name: &str,
) -> Option<DiscoveredPeer> {
    if !bytes.starts_with(b"TETHER1\n") {
        return None;
    }
    let s = std::str::from_utf8(bytes).ok()?;
    let mut map = std::collections::HashMap::<&str, &str>::new();
    for line in s.lines().skip(1) {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k, v);
        }
    }
    if map.get("type") != Some(&"announce") {
        return None;
    }
    let device_type = map.get("device_type")?.to_string();
    if device_type == our_device_type {
        return None;
    }
    let device_name = map.get("device_name").unwrap_or(&"unknown").to_string();
    if device_name == our_device_name && device_type == our_device_type {
        // Defense in depth — we should never reach here.
        return None;
    }
    let port: u16 = map.get("port")?.parse().ok()?;
    let cert_fp_short = map
        .get("cert_fp_short")
        .map(|s| s.to_string())
        .unwrap_or_default();
    Some(DiscoveredPeer {
        device_type,
        device_name,
        socket: SocketAddr::new(from.ip(), port),
        cert_fp_short,
        via: DiscoveryMethod::UdpBroadcast,
    })
}

// We need socket2 for SO_REUSEADDR + SO_REUSEPORT on the broadcast
// listener (tokio's high-level API doesn't expose those before bind).
// Tokio re-exports it but only on some feature flags, so we declare
// the dep here. (Cargo.toml entry added if/when this module ships.)
use socket2;

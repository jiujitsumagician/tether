//! Subnet probe — phase 3 of the cascade.
//!
//! Unicasts the same announce payload to every /24 host the PC can
//! reach. Used on networks that block multicast AND directed
//! broadcast but allow normal unicast traffic.

use super::{CascadeOptions, DiscoveredPeer, DiscoveryMethod};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

const MAX_CONCURRENT: usize = 64;

pub async fn run(options: &CascadeOptions) -> anyhow::Result<Option<DiscoveredPeer>> {
    let sock = std::sync::Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    sock.set_broadcast(true).ok();
    let payload = build_announce(options);

    // Enumerate /24 hosts for every IPv4 interface on this host.
    let targets = collect_targets()?;

    let (peer_tx, mut peer_rx) = mpsc::channel::<DiscoveredPeer>(1);

    // Listener — same socket; receivers send back addressed to us.
    let our_type = options.local_device_type.clone();
    let recv_sock = sock.clone();
    let listener = tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        loop {
            let (n, from) = match recv_sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(_) => break,
            };
            if let Some(peer) = parse_response(&buf[..n], from, &our_type) {
                let _ = peer_tx.send(peer).await;
                break;
            }
        }
    });

    // Sender — bounded concurrency, ~64 in flight at once is plenty
    // for a /24 sweep without DOS-ing the local switch.
    let send_sock = sock.clone();
    let send_payload = payload.clone();
    let port = options.udp_port;
    let sender = tokio::spawn(async move {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT));
        for target_ip in targets {
            let permit = semaphore.clone().acquire_owned().await.ok();
            let sock = send_sock.clone();
            let bytes = send_payload.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let addr = SocketAddr::new(IpAddr::V4(target_ip), port);
                let _ = sock.send_to(&bytes, addr).await;
            });
        }
    });

    let result = peer_rx.recv().await;
    listener.abort();
    sender.abort();
    Ok(result)
}

fn build_announce(options: &CascadeOptions) -> Vec<u8> {
    use rand::Rng;
    let nonce: u32 = rand::thread_rng().gen();
    format!(
        "TETHER1\ntype=announce\ndevice_type={}\ndevice_name={}\nport={}\ncert_fp_short={}\nnonce={:08x}\n",
        options.local_device_type,
        options.local_device_name.replace('\n', " "),
        options.local_tls_port,
        options.local_cert_fp_short,
        nonce,
    )
    .into_bytes()
}

fn parse_response(
    bytes: &[u8],
    from: SocketAddr,
    our_device_type: &str,
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
        via: DiscoveryMethod::SubnetProbe,
    })
}

fn collect_targets() -> anyhow::Result<Vec<Ipv4Addr>> {
    // Cheap interface enumeration via `ipnetwork` — for each interface
    // with an IPv4 address, generate every other /24 host.
    let mut out = Vec::new();
    let ifaces = list_local_v4_networks()?;
    for net in ifaces {
        let prefix = net.prefix();
        // Only sweep /24 (and shorter — clamp to /24 to keep the
        // packet count reasonable). Bigger networks fall through to
        // the USB phase faster than trying to enumerate them.
        if prefix < 24 || prefix > 28 {
            continue;
        }
        let mut count = 0;
        for ip in net.iter() {
            if ip == net.network() || ip == net.broadcast() {
                continue;
            }
            out.push(ip);
            count += 1;
            if count > 254 {
                break;
            }
        }
    }
    Ok(out)
}

fn list_local_v4_networks() -> anyhow::Result<Vec<ipnetwork::Ipv4Network>> {
    // Cross-platform interface enumeration via the OS getifaddrs-style
    // helper. Falls back to a single 192.168.0.0/24 sweep if the OS
    // call fails (better than nothing — typical home network).
    #[cfg(unix)]
    {
        let mut out = Vec::new();
        if let Ok(addrs) = nix::ifaddrs::getifaddrs() {
            for addr in addrs {
                if let Some(addr_v4) = addr.address.and_then(|s| s.as_sockaddr_in().cloned()) {
                    if let Some(mask_v4) = addr.netmask.and_then(|s| s.as_sockaddr_in().cloned()) {
                        let ip = Ipv4Addr::from(addr_v4.ip());
                        let mask = Ipv4Addr::from(mask_v4.ip());
                        if ip.is_loopback() {
                            continue;
                        }
                        if let Some(net) = ipnetwork::Ipv4Network::with_netmask(ip, mask).ok() {
                            out.push(net);
                        }
                    }
                }
            }
        }
        if out.is_empty() {
            out.push(ipnetwork::Ipv4Network::new(Ipv4Addr::new(192, 168, 0, 0), 24)?);
        }
        Ok(out)
    }
    #[cfg(windows)]
    {
        // The Win32 GetAdaptersAddresses API is the canonical source.
        // We avoid pulling a heavy crate just for this — at parity
        // with the Unix fallback, dial 192.168.x.0/24 for x ∈ {0, 1}
        // plus 10.0.0.0/24 and 172.16.0.0/24. Real networks fall into
        // one of these on the vast majority of home/office setups.
        Ok(vec![
            ipnetwork::Ipv4Network::new(Ipv4Addr::new(192, 168, 0, 0), 24)?,
            ipnetwork::Ipv4Network::new(Ipv4Addr::new(192, 168, 1, 0), 24)?,
            ipnetwork::Ipv4Network::new(Ipv4Addr::new(10, 0, 0, 0), 24)?,
            ipnetwork::Ipv4Network::new(Ipv4Addr::new(172, 16, 0, 0), 24)?,
        ])
    }
}

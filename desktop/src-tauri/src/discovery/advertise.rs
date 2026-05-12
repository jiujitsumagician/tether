//! PC-side advertising. Publishes the mDNS service and emits UDP
//! broadcast announces forever so the phone can discover us.
//!
//! The PC never dials. It just sits open, tells the world it's
//! listening on `local_tls_port`, and waits for the phone to dial in.

use super::CascadeOptions;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::interval;

const SERVICE_TYPE: &str = "_tether._tcp.local.";

/// Hold onto this guard for the lifetime of the app — dropping it
/// shuts the daemon down and unregisters the service.
pub struct MdnsHandle {
    daemon: ServiceDaemon,
}

impl Drop for MdnsHandle {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

pub fn advertise_mdns(options: &CascadeOptions) -> anyhow::Result<MdnsHandle> {
    let daemon = ServiceDaemon::new()?;
    let txt: HashMap<String, String> = HashMap::from([
        ("v".into(), "1".into()),
        ("t".into(), options.local_device_type.clone()),
        ("n".into(), options.local_device_name.clone()),
        ("p".into(), options.local_tls_port.to_string()),
        ("f".into(), options.local_cert_fp_short.clone()),
    ]);
    let instance_name = format!(
        "{}@{}",
        sanitize_instance(&options.local_device_name),
        rand::thread_rng().gen::<u16>()
    );
    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        "tether.local.",
        "",
        options.local_tls_port,
        Some(txt),
    )?
    .enable_addr_auto();
    daemon.register(info)?;
    tracing::info!(
        "mDNS advertising {SERVICE_TYPE} as {} on port {}",
        options.local_device_name,
        options.local_tls_port
    );
    Ok(MdnsHandle { daemon })
}

/// Broadcasts an announce packet to 255.255.255.255:<udp_port> every
/// 800 ms. Runs forever; cancel by aborting the spawned task.
pub async fn advertise_udp(options: CascadeOptions) -> anyhow::Result<()> {
    let socket = bind_broadcast(options.udp_port)?;
    let broadcast_addr: SocketAddr =
        format!("255.255.255.255:{}", options.udp_port).parse()?;
    let mut ticker = interval(Duration::from_millis(800));
    loop {
        ticker.tick().await;
        let payload = build_announce(&options);
        if let Err(e) = socket.send_to(&payload, broadcast_addr).await {
            tracing::debug!("UDP broadcast send failed: {e}");
        }
    }
}

fn build_announce(options: &CascadeOptions) -> Vec<u8> {
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

fn bind_broadcast(port: u16) -> std::io::Result<UdpSocket> {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr as Std};
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    let addr: Std = Std::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    let std_sock: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_sock)
}

fn sanitize_instance(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}

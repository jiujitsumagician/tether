//! mDNS discovery — phase 1 of the cascade.
//!
//! Publishes `_tether._tcp.local.` with a TXT record carrying device
//! type, name, TLS port, and short cert fingerprint. Simultaneously
//! browses for the same service. Returns the first peer found whose
//! `device_type` is different from ours (PC ignores PC announcements,
//! and vice versa).

use super::{CascadeOptions, DiscoveredPeer, DiscoveryMethod};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::SocketAddr;

const SERVICE_TYPE: &str = "_tether._tcp.local.";

pub async fn run(options: &CascadeOptions) -> anyhow::Result<Option<DiscoveredPeer>> {
    let daemon = ServiceDaemon::new()?;

    // Publish our own service so the other side can find us. mdns-sd
    // will figure out the local interfaces; we just need to tell it
    // which port + TXT.
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
        random_suffix()
    );

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        "tether.local.",
        "", // empty addr → mdns-sd picks all non-loopback interfaces
        options.local_tls_port,
        Some(txt),
    )?
    .enable_addr_auto();

    daemon.register(info)?;

    // Browse and watch for peers different from us.
    let receiver = daemon.browse(SERVICE_TYPE)?;
    let our_type = options.local_device_type.clone();

    // Spin up a blocking task because mdns-sd's receiver is a
    // crossbeam channel, not a Tokio one. We don't want to busy-poll.
    let result = tokio::task::spawn_blocking(move || -> Option<DiscoveredPeer> {
        loop {
            match receiver.recv() {
                Ok(ServiceEvent::ServiceResolved(srv)) => {
                    if let Some(peer) = peer_from_resolved(&srv, &our_type) {
                        return Some(peer);
                    }
                }
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten();

    // Always shut down even if we return early; this also unregisters
    // our service so a stale TXT doesn't haunt the network.
    let _ = daemon.shutdown();
    Ok(result)
}

fn peer_from_resolved(
    srv: &mdns_sd::ServiceInfo,
    our_device_type: &str,
) -> Option<DiscoveredPeer> {
    let props = srv.get_properties();
    let device_type = props.get("t").map(|p| p.val_str().to_string())?;
    if device_type == our_device_type {
        // Ignore other devices of the same type — PC<->PC pairing is
        // not in scope.
        return None;
    }

    let device_name = props
        .get("n")
        .map(|p| p.val_str().to_string())
        .unwrap_or_else(|| srv.get_hostname().to_string());
    let port: u16 = props
        .get("p")
        .and_then(|p| p.val_str().parse().ok())
        .unwrap_or(srv.get_port());
    let cert_fp_short = props
        .get("f")
        .map(|p| p.val_str().to_string())
        .unwrap_or_default();

    let addr = srv.get_addresses().iter().next()?.to_owned();
    Some(DiscoveredPeer {
        device_type,
        device_name,
        socket: SocketAddr::new(addr, port),
        cert_fp_short,
        via: DiscoveryMethod::Mdns,
    })
}

fn sanitize_instance(name: &str) -> String {
    // mDNS instance names should not contain '.' or other dot-domain
    // boundary chars. Keep it printable ASCII-ish.
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}

fn random_suffix() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let n: u32 = rng.gen_range(1000..10_000);
    n.to_string()
}

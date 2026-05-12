//! USB / ADB-over-cable — phase 4 of the cascade.
//!
//! Runs the bundled `adb` binary in a track-devices loop. The first
//! device that appears authorised triggers:
//!   1. `adb reverse tcp:<tls_port> tcp:<tls_port>` so the phone can
//!      dial the PC's TLS server over USB.
//!   2. `adb shell content insert` to push a one-time pairing token to
//!      the Android app's ContentProvider, which then opens the TLS
//!      connection over loopback.
//!
//! When ADB reports the device as `unauthorized` or `no permissions`,
//! we emit `cascade.usb.debug` so the frontend can offer the "open
//! phone settings" deep-link button.

use super::{CascadeEvent, CascadeOptions, DiscoveredPeer, DiscoveryMethod};
use rand::RngCore;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::sleep;

pub async fn run(
    options: &CascadeOptions,
    tx: mpsc::Sender<CascadeEvent>,
) -> anyhow::Result<Option<DiscoveredPeer>> {
    let adb = bundled_adb_path()?;

    // Loop until we get an authorised device. Each turn:
    //   1. List devices.
    //   2. If authorised, set up reverse tunnel + push token, done.
    //   3. If unauthorised, emit cascade.usb.debug, wait for it to
    //      change, loop.
    //   4. If empty, just keep waiting on `adb track-devices`.

    let mut tracker = Command::new(&adb)
        .arg("track-devices")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = tracker.stdout.take().expect("track-devices stdout");
    let mut reader = BufReader::new(stdout).lines();

    // Each `track-devices` event is a short length-prefixed packet
    // followed by `<serial>\t<state>` lines. We don't need the
    // protocol details — the format is human-readable enough that
    // line-based parsing is fine for our use case.
    let mut last_emitted_debug = false;
    loop {
        let line = match reader.next_line().await? {
            Some(l) => l,
            None => {
                // tracker died — restart it after a short backoff.
                sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Lines look like "RF8M201ABCD\tdevice" or
        // "RF8M201ABCD\tunauthorized".
        let (serial, state) = match trimmed.split_once('\t') {
            Some(p) => p,
            None => continue,
        };

        match state {
            "device" => {
                if let Some(peer) = handle_authorised(&adb, serial, options).await? {
                    return Ok(Some(peer));
                }
                last_emitted_debug = false;
            }
            "unauthorized" | "no permissions" if !last_emitted_debug => {
                let _ = tx
                    .send(CascadeEvent::Phase {
                        key: "cascade.usb.debug".into(),
                    })
                    .await;
                last_emitted_debug = true;
                // Best-effort: deep-link the user into the right
                // settings page. If this command fails (e.g. the
                // device immediately disconnects) we just wait for
                // the next track-devices event.
                open_dev_options(&adb, serial).await.ok();
            }
            _ => {}
        }
    }
}

async fn handle_authorised(
    adb: &PathBuf,
    serial: &str,
    options: &CascadeOptions,
) -> anyhow::Result<Option<DiscoveredPeer>> {
    // 1. Reverse the TLS port. The Android app then connects to
    //    localhost:<tls_port> and the traffic tunnels back to us.
    let reverse_ok = Command::new(adb)
        .args([
            "-s",
            serial,
            "reverse",
            &format!("tcp:{}", options.local_tls_port),
            &format!("tcp:{}", options.local_tls_port),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    if !reverse_ok {
        tracing::warn!("adb reverse failed on {serial}");
        return Ok(None);
    }

    // 2. Push the one-time pairing token into the Tether content
    //    provider. The token is consumed exactly once by the Android
    //    app to authenticate the loopback TLS handshake.
    let token = generate_token();
    let mut child = Command::new(adb)
        .args([
            "-s",
            serial,
            "shell",
            "content",
            "insert",
            "--uri",
            "content://io.tether.pairing/token",
            "--bind",
            &format!("token:s:{token}"),
            "--bind",
            &format!("port:i:{}", options.local_tls_port),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let status = child.wait().await?;
    if !status.success() {
        return Ok(None);
    }

    Ok(Some(DiscoveredPeer {
        device_type: "phone".into(),
        device_name: format!("Phone ({serial})"),
        socket: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), options.local_tls_port),
        cert_fp_short: String::new(),
        via: DiscoveryMethod::Usb,
    }))
}

async fn open_dev_options(adb: &PathBuf, serial: &str) -> anyhow::Result<()> {
    // Try the developer-options screen first; if developer options
    // aren't enabled, drop to Device Info so the user can tap "Build
    // number" seven times.
    let primary = Command::new(adb)
        .args([
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "-a",
            "android.settings.APPLICATION_DEVELOPMENT_SETTINGS",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    if primary {
        return Ok(());
    }
    let _ = Command::new(adb)
        .args([
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "-a",
            "android.settings.DEVICE_INFO_SETTINGS",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    Ok(())
}

fn bundled_adb_path() -> anyhow::Result<PathBuf> {
    // In dev: the binary lives at desktop/src-tauri/adb/<os>/adb[.exe].
    // In a Tauri release bundle: tauri.conf.json `resources` puts it
    // under the app's resource dir, which the runtime exposes via
    // `tauri::path::resource_dir()`. We try a few candidate paths.
    let exe = if cfg!(target_os = "windows") { "adb.exe" } else { "adb" };
    let subdir = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };

    // 1. Resource dir, when running inside a bundle.
    if let Ok(curr) = std::env::current_exe() {
        if let Some(parent) = curr.parent() {
            let p = parent.join("resources").join("adb").join(subdir).join(exe);
            if p.exists() {
                return Ok(p);
            }
            let p2 = parent.join("adb").join(subdir).join(exe);
            if p2.exists() {
                return Ok(p2);
            }
        }
    }
    // 2. Dev source tree (cargo run).
    let dev = PathBuf::from("adb").join(subdir).join(exe);
    if dev.exists() {
        return Ok(dev);
    }
    // 3. Fall back to whatever's on PATH so this still works on a
    //    developer machine that has Android Studio.
    Ok(PathBuf::from(exe))
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    // URL-safe base64 without padding so it survives `content
    // insert`'s string quoting.
    use rand::Rng; // ensure RNG is in scope when fmt below uses it
    let _: u8 = rand::thread_rng().gen(); // suppress unused warning when build features change
    base64_urlsafe(&bytes)
}

fn base64_urlsafe(input: &[u8]) -> String {
    const ALPHA: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() * 4 + 2) / 3);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u32;
        let b1 = if i + 1 < input.len() { input[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3f) as usize] as char);
        if i + 1 < input.len() {
            out.push(ALPHA[((n >> 6) & 0x3f) as usize] as char);
        }
        if i + 2 < input.len() {
            out.push(ALPHA[(n & 0x3f) as usize] as char);
        }
        i += 3;
    }
    out
}

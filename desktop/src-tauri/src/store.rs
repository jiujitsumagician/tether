//! Persistence: paired devices + own TLS cert.
//!
//! The cert files live alongside the JSON metadata so a single
//! `tether reset` (or manual `rm -rf ~/.tether/`) wipes the lot.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::sync::Mutex;

const FILE_NAME: &str = "paired.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub peer_device_type: String,
    pub peer_device_name: String,
    pub peer_x25519_pubkey: Vec<u8>,
    pub peer_tls_cert_sha256: Vec<u8>,
    pub paired_at: u64,
}

pub struct Store {
    path: PathBuf,
    state: Mutex<StoreFile>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    devices: Vec<PairedDevice>,
}

impl Store {
    pub async fn open_default() -> anyhow::Result<Self> {
        let dir = crate::transport::tls::data_dir()?;
        crate::transport::tls::create_private_dir(&dir).await;
        let path = dir.join(FILE_NAME);
        let state = if path.exists() {
            let bytes = fs::read(&path).await?;
            match serde_json::from_slice::<StoreFile>(&bytes) {
                Ok(s) => s,
                Err(e) => {
                    // Don't silently discard the user's pairings: keep the
                    // unreadable file for inspection and start empty.
                    let bad = dir.join(format!("{FILE_NAME}.bad"));
                    tracing::warn!(
                        "{} is unreadable ({e}); moving it to {} and starting empty",
                        path.display(),
                        bad.display()
                    );
                    if let Err(re) = fs::rename(&path, &bad).await {
                        tracing::warn!("could not move unreadable store aside: {re}");
                    }
                    StoreFile::default()
                }
            }
        } else {
            StoreFile::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    /// Test-only: a store backed by an explicit (empty) path.
    #[cfg(test)]
    pub(crate) fn open_at(path: PathBuf) -> Self {
        Self {
            path,
            state: Mutex::new(StoreFile::default()),
        }
    }

    pub async fn add_paired(&self, dev: PairedDevice) -> anyhow::Result<()> {
        let mut g = self.state.lock().await;
        // De-dupe by fingerprint — a new pair with the same fingerprint
        // replaces the old entry.
        g.devices
            .retain(|d| d.peer_tls_cert_sha256 != dev.peer_tls_cert_sha256);
        g.devices.push(dev);
        self.persist(&g).await
    }

    /// Write the store atomically (temp file + rename) so a crash
    /// mid-write cannot truncate paired.json.
    async fn persist(&self, file: &StoreFile) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec_pretty(file)?;
        let mut tmp_name = self.path.as_os_str().to_owned();
        tmp_name.push(".tmp");
        let tmp = PathBuf::from(tmp_name);
        crate::transport::tls::write_private(&tmp, &bytes).await?;
        fs::rename(&tmp, &self.path).await?;
        Ok(())
    }

    pub async fn list_paired(&self) -> anyhow::Result<Vec<PairedDevice>> {
        Ok(self.state.lock().await.devices.clone())
    }

    /// Returns the persisted record for a peer whose TLS cert
    /// fingerprint matches, if any. Used during silent reconnect: if
    /// the dialed peer's cert doesn't match a stored entry, we abort
    /// before the user sees anything.
    pub async fn find_by_fingerprint(&self, fp: &[u8]) -> Option<PairedDevice> {
        let g = self.state.lock().await;
        g.devices
            .iter()
            .find(|d| d.peer_tls_cert_sha256 == fp)
            .cloned()
    }

    pub async fn forget(&self, fp: &[u8]) -> anyhow::Result<()> {
        let mut g = self.state.lock().await;
        g.devices.retain(|d| d.peer_tls_cert_sha256 != fp);
        self.persist(&g).await
    }
}

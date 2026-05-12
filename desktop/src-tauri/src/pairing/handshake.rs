//! Tether handshake — X25519 + HKDF + CBOR message exchange.
//!
//! Both sides emit `hello`, then `verify`, then `confirm`. The handshake
//! does NOT manage the underlying TLS layer — it expects an
//! `AsyncRead + AsyncWrite` that is already wrapping a TLS-protected
//! WebSocket.

use hkdf::Hkdf;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey};

pub const HKDF_INFO: &[u8] = b"tether/verify/v1";
pub const VERIFIER_LEN: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub v: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: u32,
    pub in_reply_to: Option<u32>,
    pub body: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloBody {
    pub device_type: String,
    pub device_name: String,
    pub protocol_version: u32,
    #[serde(with = "serde_bytes")]
    pub ecdh_pubkey: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub tls_cert_sha256: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyBody {
    #[serde(with = "serde_bytes")]
    pub fingerprint: Vec<u8>,
    pub emoji_indices: [u8; 3],
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmBody {
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MismatchBody {
    pub reason: String,
}

#[derive(Debug, Clone, Copy)]
pub enum MismatchReason {
    UserMismatch,
    Timeout,
    Protocol,
}

impl MismatchReason {
    pub fn wire(&self) -> &'static str {
        match self {
            MismatchReason::UserMismatch => "user_mismatch",
            MismatchReason::Timeout => "timeout",
            MismatchReason::Protocol => "protocol",
        }
    }
}

/// Owned holder for our half of the ephemeral X25519 key pair plus
/// the derived verifier once a peer pubkey is known.
pub struct Handshake {
    secret: EphemeralSecret,
    pub local_pub: PublicKey,
}

impl Default for Handshake {
    fn default() -> Self {
        Self::new()
    }
}

impl Handshake {
    pub fn new() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let local_pub = PublicKey::from(&secret);
        Self { secret, local_pub }
    }

    /// Mix in the peer's pubkey and derive the 16-byte verifier.
    pub fn derive(self, peer_pubkey: &[u8]) -> anyhow::Result<[u8; VERIFIER_LEN]> {
        if peer_pubkey.len() != 32 {
            anyhow::bail!("peer pubkey must be 32 bytes (got {})", peer_pubkey.len());
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(peer_pubkey);
        let peer = PublicKey::from(bytes);
        let shared = self.secret.diffie_hellman(&peer);
        let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut out = [0u8; VERIFIER_LEN];
        hk.expand(HKDF_INFO, &mut out)
            .map_err(|e| anyhow::anyhow!("HKDF expand failed: {e}"))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sides_derive_same_verifier() {
        let a = Handshake::new();
        let b = Handshake::new();
        let a_pub = a.local_pub.as_bytes().to_vec();
        let b_pub = b.local_pub.as_bytes().to_vec();

        let va = a.derive(&b_pub).unwrap();
        let vb = b.derive(&a_pub).unwrap();
        assert_eq!(va, vb, "both sides must derive the same verifier");
    }

    #[test]
    fn different_pairs_produce_different_verifiers() {
        let a = Handshake::new();
        let b = Handshake::new();
        let c = Handshake::new();
        let a_pub = a.local_pub.as_bytes().to_vec();
        let b_pub = b.local_pub.as_bytes().to_vec();
        let c_pub = c.local_pub.as_bytes().to_vec();
        let v_ab = a.derive(&b_pub).unwrap();
        // Re-derive on b's side: should match.
        let b2 = Handshake::new();
        let _ = b2.derive(&a_pub).unwrap();
        // Different peer ⇒ different verifier (overwhelmingly likely).
        let other = Handshake::new();
        let v_other = other.derive(&c_pub).unwrap();
        assert_ne!(v_ab, v_other);
    }

    #[test]
    fn envelope_roundtrip() {
        let env = Envelope {
            v: 1,
            kind: "hello".into(),
            id: 1,
            in_reply_to: None,
            body: HelloBody {
                device_type: "pc".into(),
                device_name: "test".into(),
                protocol_version: 1,
                ecdh_pubkey: vec![0u8; 32],
                tls_cert_sha256: vec![1u8; 32],
            },
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&env, &mut buf).unwrap();
        let back: Envelope<HelloBody> = ciborium::from_reader(&buf[..]).unwrap();
        assert_eq!(back.v, 1);
        assert_eq!(back.kind, "hello");
        assert_eq!(back.body.device_name, "test");
    }
}

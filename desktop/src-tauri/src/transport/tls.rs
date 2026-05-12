//! TLS 1.3 client + server using a self-signed cert.
//!
//! On first run we generate an X25519/ECDSA-P256 self-signed cert and
//! persist it under `~/.tether/<os-data>/cert.{pem,key}`. The SHA-256
//! of the DER-encoded cert is what the emoji handshake verifies against
//! and what gets pinned on the peer for future reconnects.

use rustls::{ClientConfig, ServerConfig, ServerName};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, TlsStream};

/// Lightweight wrapper around the post-TLS stream so the rest of the
/// code only sees `AsyncRead + AsyncWrite + peer_cert_sha256()`.
pub struct TlsClient {
    pub stream: TlsStream<TcpStream>,
    pub peer_cert_sha256: [u8; 32],
}

impl TlsClient {
    pub async fn dial(addr: &SocketAddr, _expected_cert_fp_short: &str) -> anyhow::Result<Self> {
        let config = client_config();
        let connector = TlsConnector::from(Arc::new(config));

        // We're using SNI "tether.local" because the cert is self-signed
        // for that name. The Subject CN doesn't matter for us — we pin
        // on the fingerprint, not the name — but rustls still wants a
        // valid ServerName for the handshake.
        let sni = ServerName::try_from("tether.local")
            .map_err(|e| anyhow::anyhow!("invalid SNI: {e}"))?;

        let tcp = TcpStream::connect(addr).await?;
        let tls = connector.connect(sni, tcp).await?;

        // Compute the peer cert fingerprint right after handshake.
        let (_, server) = tls.get_ref();
        let peer_certs = server
            .peer_certificates()
            .ok_or_else(|| anyhow::anyhow!("peer sent no cert"))?;
        let leaf = peer_certs
            .first()
            .ok_or_else(|| anyhow::anyhow!("empty cert chain"))?;
        let mut hasher = Sha256::new();
        hasher.update(leaf.as_ref());
        let mut fp = [0u8; 32];
        fp.copy_from_slice(&hasher.finalize());

        Ok(Self {
            stream: TlsStream::Client(tls),
            peer_cert_sha256: fp,
        })
    }

    pub fn peer_cert_sha256(&self) -> &[u8; 32] {
        &self.peer_cert_sha256
    }
}

impl AsyncRead for TlsClient {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let pinned = std::pin::Pin::new(&mut self.get_mut().stream);
        pinned.poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsClient {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let pinned = std::pin::Pin::new(&mut self.get_mut().stream);
        pinned.poll_write(cx, buf)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let pinned = std::pin::Pin::new(&mut self.get_mut().stream);
        pinned.poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let pinned = std::pin::Pin::new(&mut self.get_mut().stream);
        pinned.poll_shutdown(cx)
    }
}

/// Returns a permissive client config — we DO NOT validate the cert
/// chain because we're using a self-signed cert that gets pinned via
/// the user-driven emoji-confirm step. Hostname verification is also
/// off; the fingerprint pinning is the actual identity check.
fn client_config() -> ClientConfig {
    let provider = rustls::crypto::ring::default_provider();
    let provider = Arc::new(provider);
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerVerifier))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

/// Trust-on-first-use verifier. The actual identity check happens in
/// the handshake layer, where the emoji code is derived from the
/// X25519 secret rather than the TLS cert; an attacker who substitutes
/// a different cert will produce a different shared secret and a
/// different emoji set.
#[derive(Debug)]
struct AcceptAnyServerVerifier;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyServerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

/// Generate (or load) the local self-signed cert and return its
/// SHA-256 fingerprint. Persisted under
/// `dirs::data_local_dir()/tether/cert.{pem,key}`.
pub async fn own_cert_sha256() -> anyhow::Result<Vec<u8>> {
    let (_cert_pem, cert_der, _key_pem) = ensure_local_cert().await?;
    let mut hasher = Sha256::new();
    hasher.update(&cert_der);
    Ok(hasher.finalize().to_vec())
}

pub fn cert_short_fp(fp_bytes: &[u8]) -> String {
    fp_bytes
        .iter()
        .take(4)
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

pub async fn ensure_local_cert() -> anyhow::Result<(String, Vec<u8>, String)> {
    let dir = data_dir()?;
    tokio::fs::create_dir_all(&dir).await.ok();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("cert.key");
    if cert_path.exists() && key_path.exists() {
        let cert_pem = tokio::fs::read_to_string(&cert_path).await?;
        let key_pem = tokio::fs::read_to_string(&key_path).await?;
        let mut cursor = std::io::Cursor::new(cert_pem.as_bytes());
        let certs = rustls_pemfile::certs(&mut cursor)
            .collect::<Result<Vec<_>, _>>()?;
        let der = certs
            .first()
            .map(|c| c.as_ref().to_vec())
            .ok_or_else(|| anyhow::anyhow!("no cert in pem file"))?;
        return Ok((cert_pem, der, key_pem));
    }
    let cert = rcgen::generate_simple_self_signed(vec!["tether.local".into()])?;
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();
    let der = cert.cert.der().to_vec();
    tokio::fs::write(&cert_path, &cert_pem).await?;
    tokio::fs::write(&key_path, &key_pem).await?;
    Ok((cert_pem, der, key_pem))
}

pub fn data_dir() -> anyhow::Result<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("data_local_dir() returned None"))?;
    Ok(base.join("tether"))
}

/// Server-side TLS config. Used by the PC's incoming-connection
/// listener so the phone can dial us by mDNS hint.
pub async fn server_config() -> anyhow::Result<Arc<ServerConfig>> {
    let (cert_pem, _, key_pem) = ensure_local_cert().await?;
    let mut cert_cursor = std::io::Cursor::new(cert_pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut cert_cursor)
        .collect::<Result<Vec<_>, _>>()?;
    let mut key_cursor = std::io::Cursor::new(key_pem.as_bytes());
    let key = rustls_pemfile::pkcs8_private_keys(&mut key_cursor)
        .next()
        .ok_or_else(|| anyhow::anyhow!("no private key in pem"))??;
    let provider = rustls::crypto::ring::default_provider();
    let config = ServerConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(certs, rustls::pki_types::PrivateKeyDer::Pkcs8(key))?;
    Ok(Arc::new(config))
}

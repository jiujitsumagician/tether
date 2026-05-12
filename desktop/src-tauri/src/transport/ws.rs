//! WebSocket-framed CBOR control channel.
//!
//! Sits on top of an established TLS stream. Each WebSocket binary
//! frame carries one CBOR-encoded `Envelope<T>` from the handshake
//! module. Generic over the underlying stream so the same channel
//! type works for both the client (dial → TlsClient → upgrade) and
//! server (accept → ServerTlsStream → from_accepted) paths.

use super::tls::TlsClient;
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub struct WsChannel<S: AsyncRead + AsyncWrite + Unpin> {
    inner: WebSocketStream<S>,
}

/// Raw envelope used when we don't yet know which body type to expect
/// — used by the confirm/mismatch waiting loop in the state machine.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawEnvelope {
    pub v: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: u32,
    pub in_reply_to: Option<u32>,
    pub body: Vec<u8>,
}

impl WsChannel<TlsClient> {
    /// Client-side: dial → TLS → WS upgrade against /tether/v1.
    pub async fn upgrade(
        stream: TlsClient,
        pin: Option<&str>,
    ) -> anyhow::Result<Self> {
        let path = match pin {
            Some(p) => format!("/tether/v1?pin={}", urlencode(p)),
            None => "/tether/v1".into(),
        };
        let request = format!("ws://tether.local{path}");
        let (ws, _resp) = tokio_tungstenite::client_async(request, stream).await?;
        Ok(Self { inner: ws })
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> WsChannel<S> {
    /// Server-side: take an already-upgraded WebSocketStream (the
    /// listener has done the HTTP/1.1 upgrade dance) and wrap it.
    pub fn from_accepted(ws: WebSocketStream<S>) -> Self {
        Self { inner: ws }
    }

    pub async fn send_cbor<T: Serialize>(&mut self, env: &T) -> anyhow::Result<()> {
        let mut buf = Vec::with_capacity(256);
        ciborium::into_writer(env, &mut buf)?;
        self.inner.send(Message::Binary(buf)).await?;
        Ok(())
    }

    /// Receive a single envelope and decode it as a specific body
    /// type. Returns Ok(None) on a clean close.
    pub async fn recv_cbor<T: DeserializeOwned>(
        &mut self,
    ) -> anyhow::Result<Option<crate::pairing::handshake::Envelope<T>>> {
        loop {
            match self.inner.next().await {
                Some(Ok(Message::Binary(bytes))) => {
                    let env: crate::pairing::handshake::Envelope<T> =
                        ciborium::from_reader(&bytes[..])?;
                    return Ok(Some(env));
                }
                Some(Ok(Message::Text(_))) => continue,
                Some(Ok(Message::Ping(p))) => {
                    self.inner.send(Message::Pong(p)).await.ok();
                }
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) | None => return Ok(None),
                Some(Ok(Message::Frame(_))) => continue,
                Some(Err(e)) => return Err(e.into()),
            }
        }
    }

    /// Generic peek — read one frame, return the type tag and the raw
    /// body bytes. Used by the confirm/mismatch select loop.
    pub async fn recv_envelope(&mut self) -> anyhow::Result<Option<RawEnvelope>> {
        loop {
            match self.inner.next().await {
                Some(Ok(Message::Binary(bytes))) => {
                    let v: ciborium::Value = ciborium::from_reader(&bytes[..])?;
                    let map = v
                        .as_map()
                        .ok_or_else(|| anyhow::anyhow!("envelope was not a map"))?;
                    let mut out = RawEnvelope {
                        v: 0,
                        kind: String::new(),
                        id: 0,
                        in_reply_to: None,
                        body: Vec::new(),
                    };
                    for (k, val) in map {
                        if let Some(key) = k.as_text() {
                            match key {
                                "v" => out.v = val.as_integer().and_then(|i| i.try_into().ok()).unwrap_or(0),
                                "type" => {
                                    if let Some(s) = val.as_text() {
                                        out.kind = s.to_string();
                                    }
                                }
                                "id" => {
                                    out.id = val.as_integer().and_then(|i| i.try_into().ok()).unwrap_or(0);
                                }
                                "in_reply_to" => {
                                    out.in_reply_to = val
                                        .as_integer()
                                        .and_then(|i| i.try_into().ok());
                                }
                                "body" => {
                                    let mut sub = Vec::new();
                                    ciborium::into_writer(val, &mut sub)?;
                                    out.body = sub;
                                }
                                _ => {}
                            }
                        }
                    }
                    return Ok(Some(out));
                }
                Some(Ok(Message::Ping(p))) => {
                    self.inner.send(Message::Pong(p)).await.ok();
                }
                Some(Ok(Message::Close(_))) | None => return Ok(None),
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(e.into()),
            }
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            let bytes = c.to_string();
            for b in bytes.bytes() {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

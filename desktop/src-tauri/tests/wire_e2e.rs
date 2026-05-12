//! End-to-end wire-protocol test.
//!
//! Spawns the real PC-side listener (transport::server::serve) on a
//! loopback ephemeral port, then drives a hand-rolled Rust client
//! through the exact sequence the Android client follows: TLS dial,
//! WS upgrade, hello / verify / confirm exchange. Both sides MUST
//! derive the same 16-byte verifier and both MUST exchange confirm
//! messages without throwing.
//!
//! If this test ever fails, the protocol is broken at the wire layer
//! — discovery, firewall, and network configuration are not the
//! cause. If this test passes, the protocol is correct and any field
//! failures are environmental.
//!
//! Run with:
//!   cargo test --test wire_e2e --no-default-features -- --nocapture

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tether_core::{
    pairing::{
        emoji_code,
        handshake::{ConfirmBody, Envelope, Handshake, HelloBody, VerifyBody},
    },
    transport::{
        server::{serve, AcceptedClient},
        tls::{own_cert_sha256, TlsClient},
        ws::WsChannel,
    },
};
use tokio::sync::mpsc;

#[tokio::test]
async fn pair_completes_end_to_end_over_loopback() {
    // tracing helps when --nocapture; silently a no-op otherwise.
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let port = pick_port().await;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    // 1) Server: bind + accept.
    let (accept_tx, mut accept_rx) = mpsc::channel::<AcceptedClient>(1);
    tokio::spawn(async move {
        if let Err(e) = serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port), accept_tx).await {
            eprintln!("server::serve exited: {e:#}");
        }
    });

    // Give the listener a beat to actually bind.
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 2) Client side: dial + run client-role handshake.
    let client_task = tokio::spawn(async move { client_handshake(addr).await });

    // 3) Server side: accept + run server-role handshake.
    let accepted = tokio::time::timeout(Duration::from_secs(5), accept_rx.recv())
        .await
        .expect("server didn't accept within 5s")
        .expect("accept channel closed");
    let server_task = tokio::spawn(async move { server_handshake(accepted).await });

    // 4) Wait for both. The handshake should never need more than a
    //    couple seconds on loopback; 10s is a generous ceiling.
    let (client_v, server_v) = tokio::time::timeout(Duration::from_secs(10), async {
        (
            client_task.await.expect("client task panicked"),
            server_task.await.expect("server task panicked"),
        )
    })
    .await
    .expect("handshake didn't complete within 10s");

    let client_v = client_v.expect("client handshake errored");
    let server_v = server_v.expect("server handshake errored");

    assert_eq!(
        client_v.verifier, server_v.verifier,
        "client and server must derive identical verifiers"
    );
    assert_eq!(
        emoji_code::from_verifier(&client_v.verifier),
        emoji_code::from_verifier(&server_v.verifier),
        "emojis must match"
    );
}

struct HandshakeOutcome {
    verifier: [u8; 16],
}

async fn client_handshake(addr: SocketAddr) -> anyhow::Result<HandshakeOutcome> {
    let tls = TlsClient::dial(&addr, "").await?;
    let mut ws = WsChannel::upgrade(tls, None).await?;

    let hs = Handshake::new();
    let local_pub = hs.local_pub.as_bytes().to_vec();
    let local_cert_fp = own_cert_sha256().await?;

    let hello = Envelope {
        v: 1,
        kind: "hello".into(),
        id: 1,
        in_reply_to: None,
        body: HelloBody {
            device_type: "phone".into(),
            device_name: "test-phone".into(),
            protocol_version: 1,
            ecdh_pubkey: local_pub,
            tls_cert_sha256: local_cert_fp,
        },
    };
    ws.send_cbor(&hello).await?;

    let peer_hello: Envelope<HelloBody> = ws
        .recv_cbor()
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed before peer hello"))?;
    let verifier = hs.derive(&peer_hello.body.ecdh_pubkey)?;
    let indices = emoji_code::indices_from_verifier(&verifier);

    let verify = Envelope {
        v: 1,
        kind: "verify".into(),
        id: 2,
        in_reply_to: Some(peer_hello.id),
        body: VerifyBody {
            fingerprint: verifier.to_vec(),
            emoji_indices: indices,
            device_name: "test-phone".into(),
        },
    };
    ws.send_cbor(&verify).await?;

    let peer_verify: Envelope<VerifyBody> = ws
        .recv_cbor()
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed before peer verify"))?;
    assert_eq!(
        peer_verify.body.fingerprint,
        verifier.to_vec(),
        "peer's verifier disagrees with ours"
    );

    let confirm = Envelope {
        v: 1,
        kind: "confirm".into(),
        id: 3,
        in_reply_to: Some(peer_verify.id),
        body: ConfirmBody { confirmed: true },
    };
    ws.send_cbor(&confirm).await?;

    let peer_confirm: Envelope<ConfirmBody> = ws
        .recv_cbor()
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed before peer confirm"))?;
    assert!(peer_confirm.body.confirmed);

    Ok(HandshakeOutcome { verifier })
}

async fn server_handshake(client: AcceptedClient) -> anyhow::Result<HandshakeOutcome> {
    let mut ws = WsChannel::from_accepted(client.ws);

    let hs = Handshake::new();
    let local_pub = hs.local_pub.as_bytes().to_vec();
    let local_cert_fp = own_cert_sha256().await?;

    let hello = Envelope {
        v: 1,
        kind: "hello".into(),
        id: 1,
        in_reply_to: None,
        body: HelloBody {
            device_type: "pc".into(),
            device_name: "test-pc".into(),
            protocol_version: 1,
            ecdh_pubkey: local_pub,
            tls_cert_sha256: local_cert_fp,
        },
    };
    ws.send_cbor(&hello).await?;

    let peer_hello: Envelope<HelloBody> = ws
        .recv_cbor()
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed before peer hello"))?;
    let verifier = hs.derive(&peer_hello.body.ecdh_pubkey)?;
    let indices = emoji_code::indices_from_verifier(&verifier);

    let verify = Envelope {
        v: 1,
        kind: "verify".into(),
        id: 2,
        in_reply_to: Some(peer_hello.id),
        body: VerifyBody {
            fingerprint: verifier.to_vec(),
            emoji_indices: indices,
            device_name: "test-pc".into(),
        },
    };
    ws.send_cbor(&verify).await?;

    let peer_verify: Envelope<VerifyBody> = ws
        .recv_cbor()
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed before peer verify"))?;
    assert_eq!(peer_verify.body.fingerprint, verifier.to_vec());

    let peer_confirm: Envelope<ConfirmBody> = ws
        .recv_cbor()
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed before peer confirm"))?;
    assert!(peer_confirm.body.confirmed);

    let confirm = Envelope {
        v: 1,
        kind: "confirm".into(),
        id: 3,
        in_reply_to: Some(peer_verify.id),
        body: ConfirmBody { confirmed: true },
    };
    ws.send_cbor(&confirm).await?;

    Ok(HandshakeOutcome { verifier })
}

async fn pick_port() -> u16 {
    let s = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = s.local_addr().unwrap().port();
    drop(s);
    p
}

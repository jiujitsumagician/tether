# Architecture

```
┌───────────────────────────┐               ┌───────────────────────────┐
│       Desktop (PC)        │               │        Android            │
│  Tauri 2 + Rust + TS UI   │               │  Kotlin + Compose         │
├───────────────────────────┤               ├───────────────────────────┤
│  Discovery cascade        │ ←────LAN────→ │  Discovery cascade        │
│  (mDNS / UDP / probe /    │   USB cable   │  (mDNS / UDP / probe /    │
│   USB / hotspot / manual) │  ← ADB ←──→   │   ADB ContentProvider /   │
│                           │               │   manual entry)           │
├───────────────────────────┤               ├───────────────────────────┤
│  TLS 1.3 (self-signed,    │ ─── TLS ───── │  TLS 1.3 (cert pinned     │
│  cert pinned by handshake │               │  by handshake fingerprint)│
│  fingerprint)             │               │                           │
├───────────────────────────┤               ├───────────────────────────┤
│  WebSocket text frames    │ ─── WS ────── │  Hand-rolled WS framing   │
│  (tungstenite)            │               │                           │
├───────────────────────────┤               ├───────────────────────────┤
│  CBOR envelopes:          │ ── CBOR ───── │  CBOR envelopes (co.nstant│
│  hello → verify → confirm │               │  .in.cbor)                │
├───────────────────────────┤               ├───────────────────────────┤
│  X25519 + HKDF-SHA256     │               │  X25519 + HKDF-SHA256     │
│  (x25519-dalek + hkdf)    │               │  (BouncyCastle)           │
├───────────────────────────┤               ├───────────────────────────┤
│  Store: JSON in           │               │  Store:                   │
│  ~/.local/share/tether/   │               │  EncryptedSharedPrefs     │
└───────────────────────────┘               └───────────────────────────┘
```

## Module responsibilities

### Desktop (`desktop/src-tauri/`)

| Module | Responsibility |
|---|---|
| `discovery/mdns.rs` | Publish + browse `_tether._tcp.local.` |
| `discovery/udp_broadcast.rs` | Send + receive announce packets on `255.255.255.255:31413` |
| `discovery/subnet_probe.rs` | Sweep the local /24 with unicast announces |
| `discovery/usb.rs` | Drive bundled `adb`: track-devices, reverse tunnel, push pairing token, deep-link USB-debug settings on `unauthorized` |
| `pairing/handshake.rs` | X25519 + HKDF, CBOR envelope (de)serialisation |
| `pairing/emoji_code.rs` | Map 16-byte verifier → 3 emojis from `EMOJIS[256]` |
| `pairing/state.rs` | Drive the cascade, set up TLS+WS, run the hello/verify/confirm sequence, persist on success |
| `transport/tls.rs` | Self-signed cert generation + TLS 1.3 client with trust-anything verifier (we pin on fingerprint) |
| `transport/ws.rs` | WebSocket framing over a `TlsClient` |
| `store.rs` | JSON-backed list of paired devices under `dirs::data_local_dir()/tether/` |

### Android (`android/app/src/main/kotlin/io/tether/`)

| Module | Responsibility |
|---|---|
| `discovery/Cascade.kt` | Orchestrates the same five phases in the same order |
| `discovery/MdnsClient.kt` | JmDNS browse with a held `MulticastLock` |
| `discovery/UdpListener.kt` | UDP broadcast send + listen on `:31413` |
| `discovery/SubnetProbe.kt` | /24 sweep using local `Inet4Address`/prefix info |
| `discovery/PairingTokenProvider.kt` | ContentProvider that the desktop pushes a pairing token to via `adb shell content insert` |
| `pairing/Handshake.kt` | X25519 + HKDF via BouncyCastle |
| `pairing/EmojiSet.kt` | Same 256-emoji table, NEVER reordered |
| `pairing/Envelope.kt` | CBOR (de)serialisation of the wire schemas |
| `pairing/PairingViewModel.kt` | Single source of truth for what's on screen during pairing |
| `transport/TlsClient.kt` | Self-signed cert generation + TLS 1.3 dial with trust-anything trust manager |
| `transport/WsChannel.kt` | Hand-rolled WebSocket client framing over `TlsClient` |
| `store/PairedPcStore.kt` | EncryptedSharedPreferences-backed list of paired PCs |

## Cross-cutting invariants

- The cascade phases run in a strict sequence. The pairing state
  machine treats every `DiscoveredPeer` the same regardless of which
  phase produced it.
- The TLS layer is "trust anything"; identity is established by
  X25519 + HKDF, displayed as an emoji code, and confirmed by the
  user. After confirm, the cert fingerprint is pinned for life.
- The 14 user-visible strings live in exactly two files (one per
  app); both are mirrors of `PAIRING.md`.
- The 256 emojis are indexed identically on both sides. A parity test
  in `test-harness/` will catch drift.
- All discovery payloads share the `TETHER1` magic header so unrelated
  UDP noise is dropped without parsing.
- No external services. No internet. Pairing works with both devices
  on a captive-portal Wi-Fi with zero outbound reachability.

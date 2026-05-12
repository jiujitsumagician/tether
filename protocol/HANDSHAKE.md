# Handshake

The Tether handshake runs **once per pair**, inside an already-established
TLS 1.3 session. Its job is to derive a shared secret that survives long
enough to display a verification code, prove to the user that both devices
arrived at the same secret, and pin each side's TLS certificate
fingerprint for all future reconnects.

```
Phone                                              PC
  │                                                │
  │  ─── TLS 1.3 handshake (self-signed) ───────▶  │
  │  ◀─── TLS 1.3 server cert (self-signed) ────   │
  │                                                │
  │  ─── WebSocket upgrade on TLS ──────────────▶  │
  │  ◀─── WebSocket accepted ──────────────────    │
  │                                                │
  │  ─── hello { ecdh_pubkey_phone, … } ─────────▶ │
  │  ◀─── hello { ecdh_pubkey_pc, … } ──────────   │
  │                                                │
  │  [both sides compute X25519(my_sk, their_pk) → shared_secret]
  │  [both sides derive verifier = HKDF-SHA256(shared, "tether/verify/v1", 16)]
  │  [both sides take verifier[0..3] → display 3 emoji from indexed set]
  │                                                │
  │  ◀─── verify { fingerprint, emoji_indices } ─  │
  │  ─── verify { fingerprint, emoji_indices } ─▶  │
  │                                                │
  │  [each side cross-checks the peer's reported   │
  │   fingerprint against its own; mismatch ⇒      │
  │   abort with "protocol" reason]                │
  │                                                │
  │      ⏳ User looks at both screens.             │
  │      User taps Confirm on each side.            │
  │                                                │
  │  ─── confirm { confirmed: true } ────────────▶ │
  │  ◀─── confirm { confirmed: true } ──────────   │
  │                                                │
  │  [both sides persist:                          │
  │     peer_x25519_pubkey                         │
  │     peer_tls_cert_sha256                       │
  │     peer_device_name                           │
  │     paired_at                                  │
  │  ]                                             │
  │                                                │
  └──────────── permanent pair ────────────────────┘
```

## Algorithms

| Step | Algorithm | Notes |
|---|---|---|
| Key agreement | X25519 (RFC 7748) | Per-pairing ephemeral keys; not reused for subsequent reconnects. |
| Key derivation | HKDF-SHA256 (RFC 5869) | `info` string includes a version tag (`tether/verify/v1`). |
| Verifier length | 16 bytes | First 3 bytes become emoji indices; remaining 13 are unused at present but reserved for an optional 6-digit numeric backup PIN. |
| Cert fingerprint | SHA-256 of the DER-encoded server certificate | Same value computed by both ends. |
| Transport | TLS 1.3, no client cert during initial pair; mutual TLS on reconnect | rustls 0.23+ / OkHttp 4.12+ |
| Framing | WebSocket text frames carrying CBOR objects | `Content-Type: application/cbor`, opcode 0x1 (text). |

The bytes of the verifier come from the X25519 shared secret, not the
TLS cert, on purpose: if the TLS layer is compromised by an attacker
who substitutes their own cert, the X25519 keys (which the attacker
cannot derive without compromising the endpoints themselves) will
produce different shared secrets on the two sides — and therefore
different emoji codes — and the user will refuse to confirm.

## Wire schemas

All messages are CBOR objects with a common envelope:

```
{ "v": 1,
  "type": <string>,
  "id": <u32, monotonic per direction>,
  "in_reply_to": <u32 or null>,
  "body": <type-specific object> }
```

### `hello`

```
{ device_type:    "pc" | "phone",
  device_name:    <UTF-8 string, ≤ 30 chars>,
  protocol_version: 1,
  ecdh_pubkey:    bytes(32),    // X25519 public key, little-endian
  tls_cert_sha256: bytes(32) }  // SHA-256 of own TLS cert, sent so
                                // the peer can cross-check it against
                                // what TLS already gave them.
```

Sent immediately after the WebSocket upgrade. Both sides emit a hello;
neither side proceeds to `verify` until it has received the peer's.

### `verify`

```
{ fingerprint:    bytes(16),    // HKDF-SHA256(shared, "tether/verify/v1", 16)
  emoji_indices:  [u8; 3],      // first three bytes of `fingerprint`
  device_name:    <UTF-8 string> }
```

Sent immediately after `hello` exchange. Each side independently
computes its own copy of `fingerprint` and `emoji_indices`; the wire
copy is a cross-check, not a source of truth. If the cross-check fails,
the connection aborts before the pairing card is rendered.

### `confirm`

```
{ confirmed: true }
```

Sent when the user taps the Confirm button. The Confirm button on each
side is disabled until that side has received the peer's `verify`
message — neither user can confirm without having seen the emoji.

A peer waiting for both confirms maintains a 60-second timeout from the
moment the pairing card was rendered. If either side fails to confirm
within that window, the handshake aborts with `mismatch { reason: "timeout" }`.

### `mismatch`

```
{ reason: "user_mismatch" | "timeout" | "protocol" }
```

Sent by either side if the user reports a mismatch, the timeout fires,
or a protocol invariant is broken (unknown message type, malformed
CBOR, etc.). Either side receiving a `mismatch` immediately closes the
TLS session.

A `mismatch` with `reason: "user_mismatch"` is logged for rate-limit
purposes: three from the same source IP within 60 seconds triggers a
10-minute backoff on that source.

## Reconnect

The handshake above runs **only** during the initial pair. On
subsequent app launches:

1. Both sides run the discovery cascade (mDNS → UDP → subnet → USB)
   exactly as on first pair.
2. Once a transport is up, each side checks the peer's TLS cert
   fingerprint against the pinned value. Mismatch ⇒ refuse the
   connection and surface a one-line warning to the user.
3. With matching fingerprints, both sides skip `hello`/`verify`/
   `confirm` and proceed directly to the post-pair control channel
   (out of scope for this milestone).

There is no `re-pair` mechanism. To pair a new phone with a previously
paired PC, the user explicitly unpairs first (operation lives in the
post-pair home, not the pairing UI).

## Failure modes worth defending against

| Failure | Defense |
|---|---|
| Network MITM substituting their own X25519 pubkey | Emojis on the two sides will differ; user refuses to confirm. |
| Network MITM substituting their own TLS cert | Same: cert affects shared secret only through TLS-channel binding (RFC 9266) is not used here, so the X25519 layer is independent — different secret, different emojis. |
| Stale pairing card from a previous session | Each `verify` message includes the peer's reported TLS cert fingerprint; cross-check fails before any UI is shown. |
| Replay of a captured handshake on a new TCP session | Ephemeral X25519 keys, generated per session, prevent replay producing the same shared secret. |
| User confirms without looking | Both sides require the peer's `verify` to land before enabling Confirm — protects against a malicious phone that auto-clicks Confirm. |
| Device with two Tether apps installed (debugging) | Pin both the cert fingerprint AND the X25519 pubkey on each side; a debug build with a different keypair will be rejected on reconnect. |

## Versioning

The `v: 1` envelope field is a hard protocol version. A peer receiving
`v: 2` from a newer client immediately responds with
`mismatch { reason: "protocol" }` and closes the connection. There is
no in-band version negotiation; the user is expected to update both
sides together.

When introducing a `v: 2`, the HKDF `info` string changes to
`tether/verify/v2` so that an in-progress `v: 1` pair cannot be
silently upgraded into a `v: 2` session with a different verifier.

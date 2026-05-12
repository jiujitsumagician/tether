# Tether handshake CBOR schemas

All handshake messages travel as **CBOR text frames** (WebSocket
opcode 0x1) on top of TLS 1.3. Each frame is a single self-describing
CBOR map; there is no length prefix beyond what the WebSocket frame
already provides.

CBOR was chosen over JSON for two reasons: byte fields (X25519 pubkeys,
fingerprints) round-trip in their natural representation without base64
overhead, and the wire size is ~30–50% smaller than JSON for the same
data.

## Envelope

Every message is a map with these top-level keys:

```cbor
{
  "v":           uint(1),                // hard protocol version
  "type":        text,                   // see "Types" below
  "id":          uint,                   // sender-local monotonic counter
  "in_reply_to": uint or null,           // id of the message this answers
  "body":        map                     // type-specific
}
```

## Types

### `hello`

```cbor
{
  "v":           1,
  "type":        "hello",
  "id":          1,
  "in_reply_to": null,
  "body": {
    "device_type":      "pc"   // or "phone"
    "device_name":      "Mike's Laptop",
    "protocol_version": 1,
    "ecdh_pubkey":      h'89ab…' (32 bytes, X25519 public key),
    "tls_cert_sha256":  h'a3f2…' (32 bytes, SHA-256 of own TLS cert)
  }
}
```

Sent by both sides immediately after WebSocket upgrade. Neither side
proceeds to `verify` until it has received the peer's `hello`.

### `verify`

```cbor
{
  "v":           1,
  "type":        "verify",
  "id":          2,
  "in_reply_to": <id of peer's hello>,
  "body": {
    "fingerprint":     h'7c…' (16 bytes, HKDF-SHA256(shared,
                                "tether/verify/v1", 16)),
    "emoji_indices":   [u8, u8, u8],   // = fingerprint[0..3]
    "device_name":     "Mike's Laptop"
  }
}
```

Sent by both sides as soon as they receive the peer's `hello`. Each
side independently computes `fingerprint` and `emoji_indices`; the wire
copy is a cross-check. If the wire copy disagrees with the locally
computed value, the connection is aborted with
`mismatch { reason: "protocol" }`.

### `confirm`

```cbor
{
  "v":           1,
  "type":        "confirm",
  "id":          3,
  "in_reply_to": <id of peer's verify>,
  "body": { "confirmed": true }
}
```

Sent when the user taps **Confirm pairing**. The Confirm button is
disabled on each side until `verify` has been received from the peer,
so an attacker cannot get the user to confirm without ever having seen
the emoji.

When both sides have exchanged `confirm`, the pair is permanent on
both ends and the TLS / X25519 fingerprints are persisted.

### `mismatch`

```cbor
{
  "v":           1,
  "type":        "mismatch",
  "id":          3,
  "in_reply_to": <id of peer's verify, or null on timeout>,
  "body": { "reason": "user_mismatch" }  // or "timeout" or "protocol"
}
```

Either side sends a `mismatch` to terminate the handshake. The
recipient closes the WebSocket and TLS sessions immediately.

`reason` semantics:

| value | meaning |
|---|---|
| `user_mismatch` | The user reported the emoji didn't match. Logged for rate-limit purposes. |
| `timeout` | 60 seconds elapsed in the verify stage with no `confirm` from either side. |
| `protocol` | An invariant was broken: unknown `type`, bad `v`, malformed CBOR, etc. |

## Sequence diagram

```
phone                                pc
  │   hello (id=1) ────────────────▶ │
  │ ◀──────────────── hello (id=1)   │
  │                                  │
  │   verify (id=2, reply=1) ──────▶ │
  │ ◀───────── verify (id=2, reply=1)│
  │                                  │
  │           (user looks)           │
  │                                  │
  │   confirm (id=3, reply=2) ─────▶ │
  │ ◀──────── confirm (id=3, reply=2)│
  │                                  │
  │      ─── pair persisted ───      │
```

## Reference vectors

Both desktop and Android ship a test that asserts the same fixed
inputs produce the same emoji output. Vector (do not change without
also updating the test on both sides):

```
my_x25519_sk      = 0x77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a
their_x25519_pk   = 0xde9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f
shared_secret     = X25519(my_sk, their_pk)
                  = 0x4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742
verifier          = HKDF-SHA256(shared_secret,
                                 info="tether/verify/v1",
                                 salt=empty,
                                 L=16)
                  = 0xb7a0db2d3a8aa1e1c2b58a3f9f5a3c47

emoji_indices     = [0xb7, 0xa0, 0xdb]  = [183, 160, 219]
emojis            = [🚑, 🔫, ⛲]
                    (indices 183, 160, 219 in protocol/EMOJI_SET.md)
```

(Replace the secret-key example with non-RFC-7748 test vectors before
shipping; the values above are placeholders for the doc.)

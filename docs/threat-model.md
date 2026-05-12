# Threat model

## Scope

Tether's pairing flow protects the moment when a phone and a PC
exchange the keys that secure all future communication between them.
After pairing, the two devices speak over mutual-TLS sessions whose
cert fingerprints are pinned from the pairing handshake.

## Adversaries we defend against

### A1 — Passive observer on the same LAN

**Capability:** Can read all UDP broadcast traffic, all mDNS queries
and responses, and the encrypted bytes of every TLS session on the
local network.

**Defense:** TLS 1.3 + an X25519 ECDH whose result is never sent on
the wire. The verifier (and the emoji code) is computed on each side
from local material; the observer learns only the X25519 *public* keys,
which are insufficient to recover the shared secret.

### A2 — Active man-in-the-middle

**Capability:** Can intercept TCP connections (e.g. via an ARP poison)
and substitute their own TLS certificate, or their own X25519
public key, or both.

**Defense:** The shared secret depends on BOTH sides' X25519 secret
keys. A MITM who swaps in their own pubkey ends up with a different
shared secret on each side, and therefore different emojis. The user
sees the mismatch and refuses to confirm. We additionally include
the TLS cert SHA-256 inside the `hello` body so that a MITM who only
substitutes the TLS cert (but not the X25519 pubkey) is caught by a
cross-check before the pairing card is rendered.

### A3 — Rogue device on the same broadcast domain

**Capability:** Can advertise its own `_tether._tcp.local.` service or
respond to UDP broadcasts.

**Defense:** Same as A2. A rogue device successfully racing the real
peer to handshake will produce its own pair of emojis, which won't
match the user's other screen.

### A4 — Stale pairing card

**Capability:** A previous handshake that timed out left the user with
a card on screen; an attacker tries to get them to tap Confirm on
that stale card while the attacker is now pairing.

**Defense:** Each `verify` message includes the peer's reported TLS
cert fingerprint, which the local side cross-checks against what the
TLS layer actually gave it. The Confirm button on each side is also
disabled until that side has received the peer's `verify` — a stale
card cannot be confirmed without a fresh hello/verify pair from a
live peer.

### A5 — Captured handshake replayed later

**Defense:** Per-session ephemeral X25519 keys. Replaying a captured
handshake against a new TCP session produces a different shared
secret (the local side's ephemeral key changes every session) and
therefore different emojis. The user notices.

### A6 — Newer/older version of the protocol

**Defense:** The `v: 1` field in every envelope is hard. A receiver
seeing `v: 2` immediately sends `mismatch { reason: "protocol" }` and
closes. The HKDF `info` string also changes between versions, so an
in-progress v1 session cannot be silently upgraded.

## Adversaries we do NOT defend against

### N1 — Endpoint compromise

If malware on either device can control the display, fake the
"Confirm pairing" button, or read the local stored secrets, the
verification model breaks. This is out of scope — Tether is not a
sandbox for compromised devices.

### N2 — Coordinated double-screen substitution

If an attacker can simultaneously alter what both screens display
(e.g. by rooting the phone and running a remote-control trojan on
the PC), they can show two matching but fake emoji codes. We do not
defend against this; the same attacker has direct access to the keys
they're trying to steal anyway.

### N3 — Social engineering

If a user can be convinced to tap Confirm regardless of what the
emojis look like, no protocol can save them. We mitigate this by
making the emoji code visually prominent and short enough to glance
between two screens; we do not enforce that the user actually looks.

## Rate limiting

Three `mismatch { reason: "user_mismatch" }` reports from the same
source IP within 60 seconds trigger a 10-minute backoff on that
source. This is per-IP, not per-peer — the goal is to slow down a
machine repeatedly attempting MITMs, not to forbid a real user who
mis-tapped twice in a row.

## Logging

Failed handshakes are logged at INFO level (Rust) / Logcat (Android)
with the discovery method that produced the peer, the cert fingerprint
short hash, and the mismatch reason. No secret material is ever
logged. No telemetry is sent off-device.

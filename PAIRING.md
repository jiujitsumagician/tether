# Pairing

The pairing flow is the entire product. This document specifies every
state, every transition, every fallback, and the exact wording shown to
the user at each step.

If a user-visible string is not in this document, it does not appear in
the app. The 14 strings below are the complete vocabulary of the pairing
flow.

---

## Happy path (15 seconds, zero typing)

```
T+0.0s  PC launches.                              "Open Tether on your phone."
T+0.0s  User picks up phone, taps Tether icon.
T+0.5s  Phone app launches.                       "Looking for your computer…"
T+1.0s  mDNS discovery starts on both sides.      "Looking on your Wi-Fi…"
T+2.5s  Each side resolves the other's TXT.       (status text unchanged)
T+3.0s  TLS handshake completes over mDNS-found endpoint.
T+3.5s  X25519 hello exchange.
T+4.0s  Verification code derived. Both sides:    "Pair with Mike's Pixel?"
                                                  (or "Pair with Mike's Laptop?")
T+4.0s                                            "These three emojis should
                                                   match on both screens. Tap
                                                   Confirm on both."

                                                  🦊  🌵  🎺

                                                  [ Confirm pairing ]

T+8.0s  User taps Confirm on PC.
T+9.0s  User taps Confirm on phone.
T+9.2s  Both confirms cross the wire. Cert
        fingerprints pinned on both sides.        "You're paired. You can
                                                   close this."
T+11.0s Success card auto-dismisses, PC drops
        to the post-pair home (out of scope for
        this milestone).
```

The hard requirement is `T+0 → T+15s` end-to-end on a normal home Wi-Fi.

---

## Auto-cascading discovery chain

The cascade is one state machine on each side. The phone and PC run the
same logic; the only difference is the PC has the USB and hotspot
fallbacks. Each phase has a hard timeout; transitions are automatic.

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                     │
│   ┌────────────┐       ┌────────────────┐     ┌─────────────────┐   │
│   │ App opens  │──5s──▶│ mDNS discovery │──Y─▶│  Both endpoints │   │
│   │ on PC and  │       │ _tether._tcp   │     │  resolved → TLS │   │
│   │ phone      │       └───────┬────────┘     └────────┬────────┘   │
│   └────────────┘               │ no                    │            │
│                                ▼                       │            │
│                       ┌────────────────┐               │            │
│                       │ UDP broadcast  │──Y────────────┤            │
│                       │ 255.255.255    │               │            │
│                       │ :255:31413 ×3s │               │            │
│                       └───────┬────────┘               │            │
│                               │ no                     │            │
│                               ▼                        │            │
│                       ┌────────────────┐               │            │
│                       │ /24 subnet     │──Y────────────┤            │
│                       │ probe (2s)     │               │            │
│                       └───────┬────────┘               │            │
│                               │ no                     │            │
│                               ▼                        │            │
│                       ┌────────────────┐               │            │
│                       │ "Connect your  │──cable in────▶│            │
│                       │ phone with any │               │            │
│                       │ USB cable."    │               │            │
│                       └───────┬────────┘               │            │
│                               │ debug off              │            │
│                               ▼                        │            │
│                       ┌────────────────┐               │            │
│                       │ Deep-link to   │──debug on────▶│            │
│                       │ Android debug  │               │            │
│                       │ settings page  │               │            │
│                       └───────┬────────┘               │            │
│                               │ user dismisses         │            │
│                               ▼                        │            │
│                       ┌────────────────┐               │            │
│                       │ Hotspot prompt │──PC joins────▶│            │
│                       │ (PC side only) │               │            │
│                       └───────┬────────┘               │            │
│                               │ user clicks            │            │
│                               ▼ "Pair another way"     │            │
│                       ┌────────────────┐               │            │
│                       │ Manual entry:  │──valid───────▶│            │
│                       │ IP + 6-digit   │               │            │
│                       │ PIN            │               │            │
│                       └────────────────┘               │            │
│                                                        ▼            │
│                                                  ┌──────────────┐   │
│                                                  │ X25519 hello │   │
│                                                  │  + emoji UI  │   │
│                                                  └──────┬───────┘   │
│                                                         │           │
│                                                         ▼           │
│                                                  ┌──────────────┐   │
│                                                  │ Both Confirm │   │
│                                                  │  → permanent │   │
│                                                  │     pair     │   │
│                                                  └──────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### Phase 1 — mDNS (5-second timeout)

Both sides publish `_tether._tcp.local.` with a TXT record containing
device type, name, TLS port, and first 8 hex chars of the cert
fingerprint. Both sides also browse the same service.

The instant both sides see each other, mDNS browsing stops and the TLS
handshake begins. If 5 seconds elapse with no peer seen, fall through.

Status line shown during this phase:
> **Looking on your Wi-Fi…**

This phase fails on networks that filter multicast (most corporate
Wi-Fi, many hotel/guest networks, some VPNs). When it fails, the
status line changes to phase 2 — the user is told *what is happening*,
never asked *what to do*.

### Phase 2 — UDP broadcast (3-second timeout)

The PC and phone both broadcast a fixed-port announce packet to
`255.255.255.255:31413` every 800 ms and listen on the same port.
Many networks block mDNS but pass directed broadcast traffic — this
phase exists exclusively for those.

Packet format (newline-delimited key=value, fits in 200 bytes):

```
TETHER1
type=announce
device_type=pc
device_name=Mike's Laptop
port=31415
cert_fp_short=a3f2c1d4
nonce=<8 hex, randomized per packet>
```

The first peer seen wins. The receiving side connects to the announced
TCP port.

Status line:
> **Trying another way…**

(Same string is reused for phases 2 and 3 — the user doesn't need a
breakdown of *which* fallback is running; only that the app is still
trying.)

### Phase 3 — Subnet probe (2-second timeout)

The PC quietly probes the local /24 by sending the same announce packet
to every unicast address on the subnet (max 64 concurrent). Receivers
respond addressed back to the probe source.

This catches the rare network that blocks both mDNS *and* directed
broadcast but still allows unicast — most commonly enterprise Wi-Fi
with client isolation half-enabled.

Status line (same as phase 2):
> **Trying another way…**

### Phase 4 — USB / ADB-over-cable

After all wireless paths have failed (10 s total), the PC switches to:

> **Connect your phone with any USB cable.**

Behind the scenes, the PC runs a bundled `adb` binary (located at
`desktop/src-tauri/adb/<os>/adb[.exe]`) in a `track-devices` loop.

When a device is detected:

1. The PC pushes a one-time pairing token via `adb shell` to the Tether
   app's content provider at `content://io.tether.pairing/token`.
2. The Android app receives the token (via the content provider's
   `insert`), uses it to authenticate a localhost loopback TLS session
   started by `adb reverse tcp:31415 tcp:31415`.
3. From the app's perspective, this is the same TLS handshake it would
   have done over Wi-Fi — same hello, same emoji derivation, same
   confirm flow. Only the transport differs.

Status line during this phase:
> **Got it. Finishing up…**

#### USB debugging disabled

If `adb` reports the device as `unauthorized` or `no permissions`, the
PC shows:

> **Tap below to turn on USB debugging — it takes 10 seconds.**
>
> [ Open phone settings ]

The button uses ADB to deep-link into Android's debugging settings:

```
adb shell am start -a android.settings.APPLICATION_DEVELOPMENT_SETTINGS
```

If developer options aren't enabled, fall back to:

```
adb shell am start -a android.settings.DEVICE_INFO_SETTINGS
```

with overlay text guiding the user to tap "Build number" seven times.
Both flows are documented inline in `desktop/src-tauri/src/discovery/usb.rs`.

### Phase 5 — Hotspot fallback

Manual, opt-in. Shown only if USB also failed (no cable, no permission
to use ADB, etc.). PC shows:

> **Turn on your phone's hotspot. We'll connect to it automatically.**

The PC watches available Wi-Fi networks; the moment one appears whose
SSID matches the phone's broadcast hotspot name (which the phone
announces on its initial UDP broadcast packet), the PC joins it and
the cascade restarts from phase 2.

This phase is best-effort and gated on platform support for programmatic
Wi-Fi join (`netsh` on Windows; `nmcli` / `NEHotspotConfiguration` on
others). Where unsupported, the PC falls through to manual entry.

### Phase 6 — Manual entry (escape hatch)

A small "**Pair another way**" link in the corner reveals two fields:

- The PC's address (visible on the PC screen as a hostname or IP).
- A 6-digit PIN (visible on the PC screen above the manual-entry form).

This is **the only place numbers ever appear** in the entire UX. 99% of
users never see it.

The 6-digit PIN is independent from the emoji code — it gates the
transport, not the verification. Once a transport is established via
manual entry, the same emoji-confirmation step runs.

---

## Verification

After the transport is up, both sides exchange X25519 public keys in a
`hello` CBOR message. From the resulting shared secret:

```
verifier = HKDF-SHA256(shared_secret, info="tether/verify/v1", L=16)
emoji_indices = verifier[0..3]   // three bytes, each 0–255
```

Both sides look up `emoji_indices` in `protocol/EMOJI_SET.md` (the same
256-entry list compiled into both apps) and display them.

The emojis appear large (~64 px on phone, ~80 px on PC), spaced enough
to be readable from across a desk. Underneath them:

> **These three emojis should match on both screens. Tap Confirm on
> both.**

User taps **Confirm pairing** on each side. The button stays disabled
on the side that hasn't received the peer's `confirm` message yet —
this is a quiet protection against the user confirming without ever
having seen the other screen.

When both `confirm` messages have been exchanged:

- The TLS cert fingerprints (SHA-256 of the DER-encoded public key,
  not the entire cert) are pinned on both sides.
- The X25519 public keys are pinned alongside, so future reconnects
  can use mutual TLS plus a shared-secret HMAC challenge for an extra
  layer of binding.
- Both apps drop to the post-pair home with:

> **You're paired. You can close this.**

The success card auto-dismisses after 3 seconds.

### Mismatch

If the user sees different emojis on each screen, they tap a smaller
"**These emojis don't match. Don't confirm — start over from both apps.**"
link instead of Confirm. Both sides terminate the handshake and return
to the discovery cascade from phase 1.

A mismatch is a strong signal of a MITM attempt (or a stale connection
to the wrong device on a busy network). The mismatch event is logged
on both sides; three mismatches in a row from the same source IP within
60 seconds triggers a 10-minute backoff on that source.

---

## Reconnect on app launch

Both sides persist the peer's:

- X25519 public key
- TLS cert SHA-256 fingerprint
- Device name
- `paired_at` timestamp

On every subsequent app launch:

1. mDNS discovery runs as in phase 1, but the pair card is *not* shown.
2. As soon as a peer is found *and* its cert fingerprint matches the
   pinned one, the transport is brought up silently.
3. If the fingerprint doesn't match, the connection is aborted and the
   user is shown a one-line warning explaining that the previously
   paired device's identity changed.

There is no user action required for a normal reconnect. The phone and
PC simply find each other and the post-pair home opens.

---

## The 14 strings

| ID | Where | Exact text |
|----|---|---|
| `home.pc.idle` | PC home, before phone is seen | **Open Tether on your phone.** |
| `home.phone.idle` | Phone home, before PC is seen | **Looking for your computer…** |
| `cascade.mdns` | Status line — mDNS phase | Looking on your Wi-Fi… |
| `cascade.fallback` | Status line — UDP / subnet probe | Trying another way… |
| `cascade.usb.prompt` | USB fallback prompt | **Connect your phone with any USB cable.** |
| `cascade.usb.detected` | USB cable detected | Got it. Finishing up… |
| `cascade.usb.debug` | USB debugging guidance | Tap below to turn on USB debugging — it takes 10 seconds. |
| `cascade.hotspot` | Hotspot fallback prompt | Turn on your phone's hotspot. We'll connect to it automatically. |
| `pair.card.title` | Pairing card title | **Pair with {peer}?** |
| `pair.card.subhead` | Pairing card subhead | These three emojis should match on both screens. Tap Confirm on both. |
| `pair.card.confirm` | Confirm button | Confirm pairing |
| `pair.mismatch` | Mismatch link / warning | These emojis don't match. Don't confirm — start over from both apps. |
| `pair.success` | Success message | You're paired. You can close this. |
| `pair.manual` | Escape-hatch link | Pair another way |

These exact strings live in `desktop/src/ui/strings.ts` and
`android/app/src/main/kotlin/io/tether/ui/Strings.kt`. Any UI string not
in this table is a bug.

---

## Threat model

Tether's verification model assumes an attacker who can observe and
mutate traffic on the local network but cannot tamper with the user's
display. The emoji code defends against a MITM attacker substituting
their own X25519 pubkey: a successful MITM would produce different
shared secrets on each side, and therefore different emojis, which the
user would notice.

The defense does **not** hold against:

- An attacker who can simultaneously control both displays (e.g.
  malware on either device).
- An attacker who can show fake "matching" emojis on a tampered phone
  UI without going through the real app.

Both of those are out of scope. The pinned cert fingerprint and X25519
public key make reconnect resistant to subsequent MITM attempts: any
future TLS session whose cert doesn't match the pinned fingerprint is
rejected before the user sees it.

Random-mode-of-failure tests (mDNS blocked, broadcast blocked, both
blocked, subnet isolated, no Wi-Fi at all, USB debugging disabled) live
in `test-harness/runners/`.

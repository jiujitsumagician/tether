# Tether

Android↔PC companion app. The entire product is judged on one thing: pairing
must be the simplest, most reliable, hardest-to-screw-up flow anywhere on
the desktop.

Microsoft Phone Link's QR-code pairing breaks the moment any network does
anything mildly unusual — client isolation, mDNS filtering, captive portal,
VPN, corporate firewall, slow DHCP. When it fails it just spins forever.

This fixes that. We are not adding a single feature until pairing is
bulletproof on every hostile network we can find.

> **Pairing target:** A non-technical user, on a Wi-Fi network we have
> never seen before, can pair their phone to their PC in **under 15
> seconds** without typing anything, scanning anything, or seeing a
> single number.

## The pairing flow at a glance

1. PC app launches → shows one sentence: **"Open Tether on your phone."**
2. Phone app launches → silently runs the discovery cascade (below).
3. Pairing card appears on both devices showing the same three emoji.
4. User glances at both screens, taps Confirm on each.
5. Pair is permanent. Reconnect on every subsequent app launch is invisible.

Full step-by-step (every state, every fallback): [`PAIRING.md`](./PAIRING.md).

## Auto-cascading discovery chain

The user never picks a method. The app picks. Each phase has a hard
timeout; transitions are automatic.

| Phase | What it does | Timeout |
|---|---|---|
| **1. mDNS** | `_tether._tcp.local.` on both sides | 5 s |
| **2. UDP broadcast** | `255.255.255.255:31413` every 800 ms | 3 s |
| **3. Subnet probe** | Unicast announce packet to every /24 host | 2 s |
| **4. USB / ADB** | "Plug in any USB cable." Bundled adb pushes a one-time pairing token. | until plugged |
| **5. Hotspot** | "Turn on your phone's hotspot." PC joins automatically. | manual |
| **6. Manual entry** | Hidden behind a "Pair another way" link. The only place numbers appear. | manual |

If Wi-Fi discovery fails entirely the USB prompt appears on its own — the
user is never asked to choose between methods.

## Verification

Once a transport is established, both devices derive a 24-bit verification
code from the X25519 ECDH shared secret:

```
HKDF-SHA256(shared_secret, info="tether/verify/v1", L=16)
  → take first 3 bytes → three indices into the curated 256-emoji set
```

Both sides display the same three emoji. User taps Confirm on each. That
is the security model.

The curated 256-emoji set lives in [`protocol/EMOJI_SET.md`](./protocol/EMOJI_SET.md)
— no faces, no flags, no foods, no skin-tone modifiers, nothing that has
been redrawn across Android versions.

After confirmation the TLS cert fingerprint is pinned on both sides. The
pair is permanent until explicitly removed.

## Tech stack

| Layer | Choice | Why |
|---|---|---|
| PC client | Tauri 2 (Rust + vanilla TS) | Sub-2-second cold start, <100 MB RAM. Electron is forbidden — it's what we're replacing. |
| Android app | Kotlin + Jetpack Compose, minSdk 26 | Native PWA-quality UI without webview overhead. |
| Transport | TLS 1.3 over TCP, WebSocket framing | rustls / OkHttp; self-signed cert generated on first run. |
| Discovery | mDNS + UDP broadcast + subnet probe + ADB-over-USB | Each independent so one being blocked doesn't kill the others. |
| Serialization | CBOR (handshake) + plain k/v (discovery) | No codegen step; tiny on the wire; survives 200-byte UDP packets. |
| External services | **None.** | Works with both devices in airplane mode connected by USB. |

## Repo layout

```
tether/
├── protocol/        Shared spec — emoji set, handshake, wire schemas
├── desktop/         Tauri PC client (Rust + vanilla TS)
├── android/         Kotlin + Compose mobile client
├── docs/            Architecture, threat model, pairing-flow diagram
└── test-harness/    Network simulator + cascade integration tests
```

## Build

### Desktop

```bash
cd desktop
npm install
npm run tauri dev          # development build, hot-reload UI
npm run tauri build        # production bundle (~6 MB on Windows)
```

Requires Rust toolchain (`rustup install stable`).

### Android

```bash
cd android
./gradlew assembleDebug
./gradlew installDebug     # installs to a connected device
```

Requires JDK 17 and the Android SDK with platform 34.

### Test harness

```bash
cd test-harness
./network-simulator/block-mdns.sh on        # drop port 5353 with iptables
npm test                                     # run cascade integration tests
./network-simulator/block-mdns.sh off
```

Runs on Linux only (the network simulators use iptables).

## Anti-patterns we will never do

- No "choose your pairing method" screen.
- No QR codes by default. (QR is buried inside Manual entry.)
- No account, no sign-in, no email, no Tether ID.
- No internet check. Pairing works on a network that has zero external
  reachability.
- No IP addresses, port numbers, or hex strings visible outside the
  manual-entry escape hatch.
- No permissions wall before pairing. Notification access etc. is
  requested *after* the user is paired.
- No spinner without a sentence next to it.

## License

Proprietary. © 2026 DickSoft Industries. All rights reserved.

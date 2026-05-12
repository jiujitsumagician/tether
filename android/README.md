# Tether — Android

Kotlin + Jetpack Compose, minSdk 26, targetSdk 34.

## Build

Requires JDK 17, Android SDK with platform 34, and the Android Gradle
plugin 8.6.

```bash
./gradlew assembleDebug
./gradlew installDebug
```

Open `tether/android/` in Android Studio Hedgehog or newer for normal
development.

## What's here

- `MainActivity` — the only Activity. Sets up Compose and hands off to
  `HomeScreen`.
- `ui/HomeScreen.kt` — single-screen state machine, one branch per
  `PairingUiState`.
- `ui/Strings.kt` — every user-visible string, mirroring
  `desktop/src/ui/strings.ts`.
- `pairing/` — handshake, emoji-set, CBOR envelope codec, state-machine
  ViewModel.
- `discovery/` — mDNS via JmDNS, UDP broadcast, /24 subnet probe,
  ContentProvider hook for the USB / ADB fallback.
- `transport/` — TLS 1.3 client + hand-rolled WebSocket framing on
  top.
- `store/` — `EncryptedSharedPreferences`-backed list of paired PCs.

The cascade and the handshake are deliberately structured to mirror
the Rust desktop code one-to-one: same phases in the same order, same
HKDF info string, same emoji-set indices.

## USB / ADB integration

The Android app does not "see" the USB cable itself — the desktop
detects insertion via `adb`, sets up `adb reverse tcp:31415`, then
calls into our ContentProvider:

```
content://io.tether.pairing/token
```

`PairingTokenProvider.insert()` shoves the resulting `DiscoveredPeer`
onto a channel that the cascade's USB-phase reads from. The TLS dial
then targets `localhost:31415` over the reverse tunnel.

See `protocol/schema/discovery.txt` for the exact `content insert`
invocation.

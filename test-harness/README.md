# Test harness

Integration tests that prove the cascade works through every realistic
failure mode. Runs on Linux only — the network simulators use
`iptables` and Linux network namespaces.

## What we test

| Test | Failure injected | Expected cascade phase |
|---|---|---|
| `happy-path.test.ts` | none | phase 1 (mDNS), pair completes in <15 s |
| `mdns-blocked.test.ts` | drop UDP/5353 | phase 2 (UDP broadcast) |
| `broadcast-blocked.test.ts` | drop 255.255.255.255 | phase 3 (subnet probe) |
| `isolated-network.test.ts` | drop everything from peer's IP | phase 4 (USB prompt) |
| `usb-cable.test.ts` | wireless blocked + emulated `adb` push | pair via USB |
| `reconnect.test.ts` | pair, restart, expect silent reconnect | none — must be invisible |
| `emoji-set-parity.test.ts` | parity check between desktop + Android emoji tables | n/a — pure data assertion |

## Requirements

- Linux with `iptables` available as the current user (run as root or
  inside a privileged container).
- Node 20 + `npm install` inside this directory.
- A built copy of the desktop binary at `../desktop/src-tauri/target/release/tether-desktop`.
- A built copy of the Android APK at `../android/app/build/outputs/apk/debug/app-debug.apk`,
  installed on an emulator that the harness can find via `adb`.

## Running

```bash
npm install
sudo ./network-simulator/block-mdns.sh on
npm test -- mdns-blocked
sudo ./network-simulator/block-mdns.sh off
```

Or all tests in sequence:

```bash
sudo npm test
```

Each test resets the network state in its `afterAll` hook, so an
interrupted run won't leave the local machine cut off from its own
LAN.

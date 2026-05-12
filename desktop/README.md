# Tether — Desktop (PC client)

Tauri 2 app, Rust backend + vanilla TypeScript frontend.

## Build

Requires:

- Rust toolchain (`rustup install stable`, `rustup default stable`)
- Node 20+
- The [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
  for your OS (WebView2 on Windows; webkit2gtk on Linux; nothing extra
  on macOS).

```bash
npm install
npm run tauri:dev          # hot-reload UI + Rust rebuild on save
npm run tauri:build        # production bundle
```

The production bundle lands in `src-tauri/target/release/bundle/`.

## ADB binary

The USB / ADB fallback uses a bundled `adb` binary located at
`src-tauri/adb/<os>/adb[.exe]`. The repo does **not** ship the binaries
themselves — fetch them from Google's platform-tools and drop them in
before building the production bundle:

```bash
# Linux / macOS
curl -L https://dl.google.com/android/repository/platform-tools-latest-darwin.zip -o pt.zip
unzip -j pt.zip platform-tools/adb -d src-tauri/adb/macos/
chmod +x src-tauri/adb/macos/adb

# Windows
curl -L https://dl.google.com/android/repository/platform-tools-latest-windows.zip -o pt.zip
unzip -j pt.zip platform-tools/adb.exe -d src-tauri/adb/windows/
```

`tauri.conf.json` already lists `adb/**/*` in `bundle.resources`, so
the binary ships with the release.

In dev (`cargo run` / `npm run tauri:dev`) the Rust side falls back to
whatever `adb` is on PATH if the bundled binary is missing.

## Tests

Unit tests covering the handshake and emoji-code derivation:

```bash
cd src-tauri
cargo test
```

Integration tests that drive the full cascade live in
`../test-harness/runners/`.

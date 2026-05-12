# Bundled ADB binaries

Drop platform-specific `adb` binaries here before producing a release
bundle. The Rust side looks for them at:

```
adb/windows/adb.exe
adb/linux/adb
adb/macos/adb
```

In dev (`cargo run` / `npm run tauri:dev`) the code falls back to
whatever `adb` is on `PATH` if the bundled binary is missing — so a
laptop with Android Studio installed Just Works without anything here.

To bundle for release, add the `resources` entry back to
`tauri.conf.json`:

```json
"bundle": {
  "resources": ["adb/**/*"]
}
```

Fetch the binaries from Google's platform-tools:

```bash
# Linux
curl -L https://dl.google.com/android/repository/platform-tools-latest-linux.zip -o pt.zip
unzip -j pt.zip platform-tools/adb -d adb/linux/
chmod +x adb/linux/adb

# macOS
curl -L https://dl.google.com/android/repository/platform-tools-latest-darwin.zip -o pt.zip
unzip -j pt.zip platform-tools/adb -d adb/macos/
chmod +x adb/macos/adb

# Windows
curl -L https://dl.google.com/android/repository/platform-tools-latest-windows.zip -o pt.zip
unzip -j pt.zip platform-tools/adb.exe platform-tools/AdbWinApi.dll platform-tools/AdbWinUsbApi.dll -d adb/windows/
```

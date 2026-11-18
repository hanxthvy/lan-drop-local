# Membangun LAN DROP untuk semua platform

Satu basis kode Rust, satu UI (`public/`), lima keluaran: `.deb`, `.rpm`,
AppImage, `.exe` (NSIS), `.dmg`/`.app`, dan `.apk`. Tidak ada Node di jalur
aplikasi; npm hanya dipakai untuk pemeriksa regresi dan CLI Tauri.

| Platform | Keluaran | Dibangun di | Butuh |
|---|---|---|---|
| Linux | `<ver>_amd64.deb`, `.rpm` | Linux | `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libsoup-3.0-dev`, `rpm` |
| Windows | `LAN Drop_<ver>_x64-setup.exe` (NSIS) | Windows | WebView2 (sudah ada di Windows 10/11), toolchain MSVC |
| macOS | `LAN Drop.app`, `LAN Drop_<ver>_<arch>.dmg` | macOS | Xcode command line tools |
| Android | `lan-drop-<ver>.apk` | Linux/macOS | JDK 17, Android SDK 34+, NDK 26.1, `platforms;android-36` |

CLI Tauri berasal dari cargo (bukan npm), supaya jalur build distribusi bersih:

```bash
cargo install tauri-cli --version '^2'
```

## Linux

```bash
cd rust/app
cargo tauri build --bundles deb,rpm
ls ../../target/release/bundle
```

AppImage sengaja tidak dipakai: bundler Tauri 2.12 salah membaca hasil
`pkg-config` appindicator di Void Linux (`/usr/lib64 -L/usr/lib/...` dikirim
sebagai satu path) sehingga bundling gagal. `.deb` dan `.rpm` tidak terpengaruh;
di distro Debian/Ubuntu masalah ini tidak muncul.

`bundle.linux.deb.depends` di `tauri.conf.json` mencantumkan
`libwebkit2gtk-4.1-0` dan `libayatana-appindicator3-1`.

## Windows

```powershell
cd rust/app
cargo tauri build
```

Menghasilkan NSIS `.exe`. Tray memakai notification area sistem; tidak ada
PowerShell helper lagi seperti versi Node dulu. WebView2 Runtime sudah ada di
Windows 10 (1803+) dan 11 — kalau tidak, installer NSIS Tauri menawarkannya.

## macOS

```bash
cd rust/app
cargo tauri build            # .app + .dmg untuk arsitektur host
cargo tauri build --target universal-apple-darwin   # Intel + Apple Silicon
```

Untuk distribusi di luar mesin sendiri, `.app` harus di-`codesign` dan
di-notarize. Tanpa itu Gatekeeper menolak membukanya. Itu langkah rilis, bukan
langkah build.

## Android

Sekali saja, setelah `cargo tauri android init`:

```bash
export ANDROID_HOME=$HOME/Android/Sdk
export NDK_HOME=$ANDROID_HOME/ndk/26.1.10909125
export JAVA_HOME=/usr/lib/jvm/openjdk17

cd rust/app
npx tauri android build --apk                  # semua ABI
npx tauri android build --apk --target aarch64 # hanya arm64 (paling cepat)
```

APK hasil build release **belum ditandatangani**. Tanda tangani sebelum
dipasang:

```bash
BT=$ANDROID_HOME/build-tools/36.0.0
$BT/zipalign -f 4 app-universal-release-unsigned.apk app-aligned.apk
$BT/apksigner sign --ks ~/.config/lan-drop/release.keystore \
  --ks-pass pass:landrop-dev --out lan-drop-0.1.0.apk app-aligned.apk
```

Yang perlu diketahui tentang Android (jujur, bukan klaim):

- **Server tetap jalan di dalam aplikasi.** `start_server` mengikat
  `0.0.0.0:3000` dan WebView memuat `http://127.0.0.1:3000`. Manifest rilis
  memasang `usesCleartextTraffic="true"` karena UI-nya memang HTTP polos di
  jaringan lokal — bukan karena TLS dimatikan untuk internet.
- **Multicast lock.** Android mematikan paket broadcast untuk aplikasi yang
  tidak meminta `WifiManager.MulticastLock`. Belum dipasang, jadi discovery
  broadcast bisa tidak jalan di sebagian HP; `LANDROP_PEERS` (daftar peer
  manual) tetap bisa dipakai sebagai jalan pintas sampai itu dikerjakan.
- **Folder simpan** mengikuti data dir aplikasi (sandbox), bukan galeri.
  Menyimpan ke galeri butuh `MediaStore`, belum dikerjakan.
- **Belum diuji di perangkat.** APK dibangun dan ditandatangani, tetapi belum
  dijalankan di HP/emulator di mesin build ini.

## Rilis otomatis

`.github/workflows/release.yml` membangun semuanya di runner aslinya
(`ubuntu-22.04`, `windows-latest`, `macos-14`, `macos-13`) plus satu job
Android, lalu mengunggah artefak. Tag `v*` memicunya. Build lintas-arsitektur
tidak dipakai: `.dmg` dan NSIS butuh toolchain host, dan hasil cross-compile
tidak boleh diklaim teruji.

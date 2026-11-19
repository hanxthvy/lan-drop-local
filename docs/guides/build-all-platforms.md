# Membangun LAN DROP untuk semua platform

Satu basis kode Rust, satu UI (`public/`), empat platform: `.deb`/`.rpm`,
`.exe` (NSIS), `.dmg`/`.app` universal (Intel + Apple Silicon), dan `.apk`.
Tidak ada Node di jalur aplikasi; npm hanya dipakai untuk pemeriksa regresi
dan CLI Tauri.

Satu tag `v*` membangun semuanya di runner aslinya dan melampirkan hasilnya ke
rilis GitHub (`.github/workflows/release.yml`). Rilis 0.1.0 sudah terbukti:
<https://github.com/hanxthvy/lan-drop-local/releases/tag/v0.1.0>

| Platform | Keluaran | Dibangun di | Butuh |
|---|---|---|---|
| Linux | `<ver>_amd64.deb`, `.x86_64.rpm` | Linux | `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libsoup-3.0-dev`, `rpm` |
| Windows | `LAN Drop_<ver>_x64-setup.exe` (NSIS) | Windows | WebView2 (sudah ada di Windows 10/11), toolchain MSVC |
| macOS | `LAN Drop.app`, `LAN Drop_<ver>_universal.dmg` | macOS (intel) | Xcode command line tools, target `x86_64` + `aarch64-apple-darwin` |
| Android | `lan-drop.apk` (universal, 4 ABI, ditandatangani) | Linux | JDK 17, Android SDK (`platforms;android-36`), NDK 26.1 |

Artefak 0.1.0 yang sudah diperiksa isinya: `.deb` 3.7 MB, `.rpm` 3.7 MB,
`.exe` 2.8 MB (PE32), `.dmg` 7.1 MB (Mach-O fat: x86_64 + arm64), `.apk`
29 MB (`id.hanxthvy.landrop`, minSdk 24, sertifikat `CN=LAN Drop`).

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
```

Bangun lalu pecah dalam satu perintah:

```bash
npm run android:apk          # -> dist/android/lan-drop-<versi>-{32bit,64bit,universal}.apk
```

Tiga berkas yang dihasilkan, masing-masing sudah di-zipalign dan
ditandatangani:

| Berkas | Isi | Perkiraan |
|---|---|---|
| `-32bit.apk` | `armeabi-v7a` + `x86` | 13,8 MB |
| `-64bit.apk` | `arm64-v8a` + `x86_64` | 15,6 MB |
| `-universal.apk` | keempatnya | 27,8 MB |

Gradle hanya bisa memecah per-ABI, jadi pengelompokan 32/64 dikerjakan
`scripts/android-apk-split.py` setelah build: APK universal dipangkas
direktori `lib/<abi>/`-nya, kompresi tiap entri dipertahankan (`resources.arsc`
dan `.so` tetap STORED, syarat targetSdk ≥ 30), lalu di-zipalign dan
ditandatangani ulang.

### Izin dan folder di Android

| Izin | Untuk apa |
|---|---|
| `INTERNET` | server lokal dan transfer TCP |
| `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE` | memilih alamat LAN yang benar |
| `CHANGE_WIFI_MULTICAST_STATE` | memegang `MulticastLock`; tanpa ini broadcast discovery dibuang saat layar mati |
| `WRITE_EXTERNAL_STORAGE` (`maxSdkVersion=28`) | hanya untuk Android 9 ke bawah |

Tidak ada izin "semua file". Folder simpan diambil dari
`getExternalFilesDir("Download")` milik aplikasi
(`/storage/emulated/0/Android/data/id.hanxthvy.landrop/files/Download`): bisa
ditulis tanpa dialog izin dan masih terlihat lewat USB/pengelola berkas.
Path-nya dicatat `MainActivity` ke `files/landrop-save-dir` sebelum Rust
menyala, karena sejak Android 10 aplikasi tidak boleh membuat folder
`Android/data/<paket>` sendiri — percobaan pertama di emulator gagal dengan
`Read-only file system` justru karena itu.

Folder data (riwayat, pengaturan) tetap di `/data/user/0/<paket>`.

### Yang sudah diuji di emulator (Android 14, x86_64)

- APK 64-bit terpasang dan UI tampil lengkap (tab Devices/Activity, tombol
  Connect/Dashboard, empty state)
- `/status` di dalam perangkat: `runtime: rust`, `platform: android`
- kirim 5 MB dari binary desktop ke aplikasi lewat TCP 3001: berkas mendarat
  di folder simpan dengan SHA-1 identik
- APK 32-bit tidak bisa dipasang di image x86_64 (tidak ada ABI 32-bit),
  jadi isinya diperiksa lewat `aapt2 dump badging` saja

Kunci dicari berurutan: `ANDROID_KEYSTORE` + `ANDROID_KEY_PASS` di env,
`~/.config/lan-drop/keystore.env`, atau tanpa tanda tangan. Di CI kuncinya
datang dari secret `ANDROID_KEY_BASE64` dan `ANDROID_KEY_PASS`.

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

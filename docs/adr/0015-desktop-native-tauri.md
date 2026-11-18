# 15 — Aplikasi desktop dipindah ke Tauri, Node hanya alat uji

Tanggal: 2026-11-18
Status: diterima

## Konteks

Setelah inti server diport ke Rust (ADR-0013), yang tersisa di jalur runtime
masih Node: `main.js` (jendela, tray, single instance, dialog), `preload.js`
(jembatan IPC), dan `tui.js` (menu terminal). Electron ikut membawa Chromium
dan NodeRuntime ke setiap instalasi, jadi "sudah Rust" baru benar untuk bagian
server — bukan untuk aplikasi yang dipakai orang.

Sasaran yang diminta pemilik proyek: **tidak ada Node di jalur aplikasi, npm
hanya untuk dependensi dan pemeriksaan**. `public/` tidak boleh berubah dan
tidak boleh ada build step frontend.

## Keputusan

1. `rust/` menjadi workspace dua crate: `core` (library + binary CLI) dan
   `app` (binary desktop Tauri 2).
2. `app` memakai `lan_drop_core::start_server` — server HTTP, discovery UDP dan
   transfer TCP berjalan di proses yang sama. Tidak ada proses anak, tidak ada
   IPC, tidak ada jalur server kedua yang bisa menyimpang.
3. Jendela memuat `http://127.0.0.1:<port>` lewat `WebviewUrl::External`. UI
   yang dilihat pengguna adalah `public/` yang sama dengan versi browser, jadi
   tidak ada duplikat frontend dan tidak ada aset yang perlu dibangun ulang.
4. `main.js`, `preload.js`, `tui.js`, dan pemeriksa `check-tui.js` /
   `check-tray.js` dihapus. `package.json` tidak lagi punya `main`,
   `dependencies` kosong, dan `electron` hilang dari devDependencies.
5. Binary tetap bisa dipakai tanpa GUI: `--core` / `--headless` /
   `LANDROP_HEADLESS=1` menjalankan `run_cli` yang sama dengan CLI.
6. Tray bersifat opsional saat runtime. Di Linux tray butuh
   `libayatana-appindicator3`; kalau tidak ada, aplikasi mencetak satu baris dan
   tetap berjalan dengan jendela. Kegagalan tray tidak boleh mematikan server
   yang sudah menyala.

## Konsekuensi

- Dependensi sistem baru untuk membangun: `webkit2gtk-4.1`, `libsoup-3.0`,
  `gtk+3`, `librsvg`, `pkg-config`, dan `libayatana-appindicator` untuk tray.
  CI memasangnya di runner Ubuntu.
- Tidak ada lagi `ipcRenderer`, `contextBridge`, atau `window.landropDesktop`.
  Frontend yang ada memang tidak memakainya (diperiksa sebelum penghapusan),
  jadi tidak ada perilaku yang hilang.
- Keluar dari aplikasi menutup server lewat `RunEvent::Exit` supaya riwayat
  yang masih di buffer ikut tertulis — pemeriksa `check-settings.js` sekarang
  membaca `rust/app/src/main.rs`, bukan `main.js`.
- Bundel (`.deb`, AppImage, NSIS, dmg) dibangun dengan `cargo tauri build`;
  CLI Tauri berasal dari cargo, bukan npm, agar tidak ada Node di jalur build
  distribusi.
- Jalur Windows untuk jendela/tray belum diuji di mesin ini. Yang diuji:
  build, server, window creation di Xvfb, dan seluruh pemeriksaan otomatis.

## Pemeriksa

`scripts/check-desktop.js` menjaga keputusan ini: gagal kalau `main.js` /
`preload.js` / `tui.js` muncul lagi, kalau `package.json` punya `main` atau
dependensi runtime, kalau lockfile menyisakan `electron`, kalau `app` tidak
memakai `start_server`, kalau tray kembali memakai `?`, kalau `frontendDist`
tidak menunjuk `public/`, atau kalau binary desktop tidak bisa melayani
`/status` dan dashboard sendirian.

## Tambahan: target platform

Satu crate `app` menghasilkan empat bentuk distribusi tanpa jalur kode kedua:

| Platform | Bundel | Catatan |
|---|---|---|
| Linux | `.deb`, `.rpm` | dibangun di mesin ini, isi paket diperiksa |
| Windows | NSIS `.exe` | workflow siap; belum dibangun di mesin ini |
| macOS | `.app`, `.dmg` | workflow siap; belum dibangun di mesin ini |
| Android | `.apk` universal | dibangun + ditandatangani di mesin ini, belum dijalankan di HP |

Yang membuat Android mungkin tanpa menulis ulang UI: server tetap dijalankan
`start_server` di dalam aplikasi, dan WebView sistem memuat
`http://127.0.0.1:<port>`. Karena itu `crate-type` menyertakan `staticlib` dan
`cdylib`, jendela didefinisikan di `tauri.conf.json` (Android menolak pembuatan
jendela dari kode), dan seluruh tray/clipboard berada di balik `cfg(desktop)`.

Batasnya dicatat supaya tidak diklaim lebih dari kenyataan:

- Android mematikan broadcast tanpa `MulticastLock`; belum dipasang, jadi
  discovery bisa kosong di sebagian HP. `LANDROP_PEERS` masih bisa dipakai.
- Folder simpan mengikuti sandbox aplikasi, bukan galeri (`MediaStore` belum).
- AppImage tidak dipakai: bundler Tauri 2.12 salah membaca `pkg-config`
  appindicator di Void (`/usr/lib64 -L/usr/lib/...` sebagai satu path).

#!/usr/bin/env node
/**
 * Regresi: aplikasi desktop harus tetap native Rust.
 *
 * Setelah `main.js`, `preload.js` dan `tui.js` dihapus, satu-satunya jalan agar
 * Node tidak diam-diam kembali adalah memeriksa repo ini secara eksplisit:
 * tidak ada entri Electron, tidak ada dependensi runtime npm, dan binary
 * desktop benar-benar bisa melayani dashboard tanpa bantuan Node.
 *
 * Yang TIDAK diuji di sini: jendela webview butuh display, jadi hanya diuji
 * manual (lihat docs/adr/0015). Pemeriksaan ini menguji jalur server dan
 * memastikan kode jendela masih ada.
 *
 * Usage: node scripts/check-desktop.js
 */
const assert = require('assert');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');

const ROOT = path.join(__dirname, '..');
const APPDIR = path.join(ROOT, 'rust', 'app');
const BIN = path.join(
  ROOT,
  'rust',
  'target',
  'release',
  process.platform === 'win32' ? 'lan-drop.exe' : 'lan-drop',
);

let failures = 0;
function check(label, fn) {
  try {
    fn();
    console.log(`  OK   ${label}`);
  } catch (err) {
    failures += 1;
    console.log(`  FAIL ${label} — ${err.message}`);
  }
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf-8'));
}

/// Seluruh sumber aplikasi: `lib.rs` (logika) + `main.rs` (entrypoint desktop).
function desktopSource() {
  return ['lib.rs', 'main.rs']
    .map((name) => fs.readFileSync(path.join(APPDIR, 'src', name), 'utf-8'))
    .join('\n');
}

// --- 1. sisa runtime Node/Electron -----------------------------------------

for (const name of ['main.js', 'preload.js', 'tui.js']) {
  check(`entrypoint Node ${name} sudah tidak ada`, () => {
    assert(!fs.existsSync(path.join(ROOT, name)), `${name} masih ada di akar repo`);
  });
}

check('package.json tidak menunjuk file JS sebagai entrypoint', () => {
  const pkg = readJson(path.join(ROOT, 'package.json'));
  assert(!pkg.main, `package.json masih punya "main": ${pkg.main}`);
  assert(!pkg.dependencies || Object.keys(pkg.dependencies).length === 0,
    `masih ada dependensi runtime: ${Object.keys(pkg.dependencies || {}).join(', ')}`);
  const dev = Object.keys(pkg.devDependencies || {});
  assert(!dev.includes('electron'), 'electron masih ada di devDependencies');
  assert(!dev.includes('electron-builder'), 'electron-builder masih ada di devDependencies');
});

check('lockfile tidak menyisakan electron', () => {
  const lock = path.join(ROOT, 'package-lock.json');
  if (!fs.existsSync(lock)) return;
  const text = fs.readFileSync(lock, 'utf-8');
  assert(!/"node_modules\/electron"/.test(text), 'package-lock.json masih memuat paket electron');
});

check('tidak ada require("electron") yang tertinggal', () => {
  const offenders = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (['node_modules', '.git', 'target', 'public'].includes(entry.name)) continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (/\.(js|cjs|mjs)$/.test(entry.name)) {
        // Skrip ini sendiri memuat pola `require('electron')` sebagai teks.
        if (path.join(dir, entry.name) === __filename) continue;
        const text = fs.readFileSync(full, 'utf-8');
        if (/require\(['"]electron['"]\)|from ['"]electron['"]/.test(text)) {
          offenders.push(path.relative(ROOT, full));
        }
      }
    }
  };
  walk(ROOT);
  assert(offenders.length === 0, `masih memuat electron: ${offenders.join(', ')}`);
});

// --- 2. struktur workspace Rust -------------------------------------------

check('workspace Rust memuat core dan app', () => {
  const workspace = fs.readFileSync(path.join(ROOT, 'rust', 'Cargo.toml'), 'utf-8');
  assert(/members\s*=\s*\[[^\]]*"core"/.test(workspace), 'crate core tidak ada di members');
  assert(/members\s*=\s*\[[^\]]*"app"/.test(workspace), 'crate app tidak ada di members');
});

check('aplikasi desktop memakai inti yang sama, bukan server kedua', () => {
  const main = desktopSource();
  assert(/start_server/.test(main), 'main.rs tidak memanggil start_server');
  // Jendela dibuat dari konfigurasi (syarat Android) lalu diarahkan ke port
  // server yang benar-benar dipakai proses ini.
  assert(/127\.0\.0\.1:\{port\}/.test(main), 'jendela tidak diarahkan ke server lokal');
  assert(/window\.navigate/.test(main), 'jendela tidak diarahkan ulang saat port berbeda');
});

check('tray tidak mematikan aplikasi saat appindicator tidak ada', () => {
  const main = desktopSource();
  assert(/if let Err\(err\) = [\w:]*build_tray/.test(main),
    'kegagalan tray masih memakai `?`, jadi aplikasi mati di distro tanpa appindicator');
});

check('konfigurasi Tauri memakai public/ apa adanya', () => {
  const conf = readJson(path.join(APPDIR, 'tauri.conf.json'));
  assert(conf.identifier, 'identifier kosong');
  assert(conf.build && conf.build.frontendDist, 'frontendDist kosong');
  const dist = path.resolve(APPDIR, conf.build.frontendDist);
  assert(path.resolve(dist) === path.resolve(ROOT, 'public'),
    `frontendDist menunjuk ${dist}, bukan public/`);
  assert(fs.existsSync(path.join(ROOT, 'public', 'index.html')), 'public/index.html tidak ada');
  for (const icon of conf.bundle.icon || []) {
    assert(fs.existsSync(path.resolve(APPDIR, icon)), `ikon bundel hilang: ${icon}`);
  }
});

check('bundel mencakup semua platform yang dijanjikan', () => {
  const conf = readJson(path.join(APPDIR, 'tauri.conf.json'));
  const targets = (conf.bundle && conf.bundle.targets) || [];
  for (const need of ['deb', 'rpm', 'nsis', 'dmg']) {
    assert(targets.includes(need), `target bundel ${need} hilang`);
  }
  assert(conf.identifier && !/\.app$/.test(conf.identifier),
    `identifier "${conf.identifier}" berakhiran .app dan menabrak ekstensi bundel macOS`);
});

check('jendela didefinisikan di konfigurasi (syarat Android)', () => {
  const conf = readJson(path.join(APPDIR, 'tauri.conf.json'));
  const windows = (conf.app && conf.app.windows) || [];
  const main = windows.find((w) => w.label === 'main');
  assert(main, 'tidak ada jendela "main" di tauri.conf.json');
  assert(/^http:\/\/127\.0\.0\.1:\d+/.test(main.url || ''),
    `jendela tidak memuat server lokal: ${main.url}`);
});

check('proyek Android ada dan memakai paket yang sama', () => {
  const conf = readJson(path.join(APPDIR, 'tauri.conf.json'));
  const gradle = path.join(APPDIR, 'gen', 'android', 'app', 'build.gradle.kts');
  assert(fs.existsSync(gradle), 'gen/android belum di-init (npx tauri android init)');
  const text = fs.readFileSync(gradle, 'utf-8');
  assert(text.includes(`applicationId = "${conf.identifier}"`),
    'applicationId Android tidak sama dengan identifier Tauri');
  // Aplikasi ini memang bicara HTTP polos ke 127.0.0.1 dan ke perangkat LAN.
  assert(/manifestPlaceholders\["usesCleartextTraffic"\] = "true"/.test(text),
    'release build tidak mengizinkan cleartext, jendela tidak bisa memuat server lokal');
});

check('workflow rilis membangun semua platform', () => {
  const file = path.join(ROOT, '.github', 'workflows', 'release.yml');
  assert(fs.existsSync(file), 'release.yml tidak ada');
  const text = fs.readFileSync(file, 'utf-8');
  for (const need of ['ubuntu', 'windows', 'macos', 'android']) {
    assert(new RegExp(need, 'i').test(text), `matriks rilis tidak menyebut ${need}`);
  }
  assert(/tauri android build|android-apks\.sh/.test(text), 'workflow tidak membangun APK');
});

// Android punya tiga berkas, bukan satu: 32-bit, 64-bit, universal. Pemecahan
// terjadi setelah gradle, karena gradle hanya bisa memecah per-ABI.
check('APK dipecah jadi 32-bit, 64-bit, dan universal', () => {
  const split = path.join(ROOT, 'scripts', 'android-apk-split.py');
  assert(fs.existsSync(split), 'scripts/android-apk-split.py tidak ada');
  const script = fs.readFileSync(split, 'utf-8');
  assert(/"32bit": \{"armeabi-v7a", "x86"\}/.test(script),
    'grup 32-bit bukan armeabi-v7a + x86');
  assert(/"64bit": \{"arm64-v8a", "x86_64"\}/.test(script),
    'grup 64-bit bukan arm64-v8a + x86_64');
  // resources.arsc dan .so harus tetap STORED, kalau tidak Android menolak.
  assert(/new\.compress_type = item\.compress_type/.test(script),
    'pemecah APK membuang jenis kompresi asli');

  const wrapper = path.join(ROOT, 'scripts', 'android-apks.sh');
  assert(fs.existsSync(wrapper), 'scripts/android-apks.sh tidak ada');
  const wrapText = fs.readFileSync(wrapper, 'utf-8');
  assert(/android-apk-split\.py/.test(wrapText), 'skrip pembungkus tidak memanggil pemecah');

  const pkg = readJson(path.join(ROOT, 'package.json'));
  assert(/android-apks\.sh/.test(pkg.scripts['android:apk'] || ''),
    'npm run android:apk tidak memakai skrip pemecah');

  const release = fs.readFileSync(path.join(ROOT, '.github', 'workflows', 'release.yml'), 'utf-8');
  assert(/dist\/android\/\*\.apk/.test(release), 'workflow rilis tidak mengunggah hasil pemecahan');
});

// --- 3. binary desktop benar-benar melayani --------------------------------

function freePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.once('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

function waitFor(url, deadline) {
  return new Promise((resolve, reject) => {
    const tick = async () => {
      if (Date.now() > deadline) return reject(new Error('server tidak siap dalam 20 detik'));
      try {
        const res = await fetch(url);
        if (res.ok) return resolve(await res.json());
      } catch (_) { /* belum siap */ }
      setTimeout(tick, 250);
    };
    tick();
  });
}

async function liveChecks() {
  if (process.env.SKIP_BINARY === '1') {
    console.log('  --   jalur binary dilewati (SKIP_BINARY=1)');
    return;
  }
  if (!fs.existsSync(BIN)) {
    failures += 1;
    console.log(`  FAIL binary desktop belum dibangun — jalankan npm run app:build (${BIN})`);
    return;
  }

  const port = await freePort();
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'landrop-desktop-'));
  const child = spawn(BIN, ['--core'], {
    env: {
      ...process.env,
      PORT: String(port),
      TRANSFER_PORT: String(port + 1),
      DISCOVERY_PORT: String(port + 2),
      LANDROP_DATA_DIR: dataDir,
      LANDROP_SAVE_DIR: path.join(dataDir, 'save'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });

  try {
    const status = await waitFor(`http://127.0.0.1:${port}/status`, Date.now() + 20000);
    assert.strictEqual(status.runtime, 'rust', `runtime bukan rust: ${status.runtime}`);
    assert.strictEqual(status.pid, child.pid, 'pid server bukan proses aplikasi desktop');
    const home = await fetch(`http://127.0.0.1:${port}/`);
    assert(home.ok, `dashboard menolak request: ${home.status}`);
    const html = await home.text();
    assert(/<html/i.test(html), 'dashboard tidak mengirim HTML');
    console.log('  OK   binary desktop melayani /status dan dashboard tanpa Node');
  } catch (err) {
    failures += 1;
    console.log(`  FAIL jalur server binary desktop — ${err.message}${stderr ? ` (${stderr.trim().split('\n')[0]})` : ''}`);
  } finally {
    child.kill('SIGKILL');
    fs.rmSync(dataDir, { recursive: true, force: true });
  }
}

liveChecks().then(() => {
  if (failures > 0) {
    console.error(`\n${failures} pemeriksaan desktop gagal.`);
    process.exit(1);
  }
  console.log('\nOk — desktop native Rust, tanpa Node/Electron di jalur aplikasi.');
});

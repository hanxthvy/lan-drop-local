//! Inti LAN DROP versi Rust — server lengkap, bukan potongan.
//!
//! Status port: lihat `docs/adr/0013-port-inti-ke-rust.md` dan
//! `docs/prd-port-rust.md`. Yang diport di sini adalah seluruh inti Node:
//! HTTP + gerbang akses, discovery UDP, transfer TCP, riwayat, PIN dashboard,
//! dan thumbnail. Frontend di `public/` dipakai apa adanya tanpa build step.
//!
//! Kontrak yang dipertahankan (diuji `scripts/check-parity.js`):
//!   * frame TCP: 4 byte panjang big-endian + JSON UTF-8
//!   * announce UDP: `{type:"announce", deviceId, deviceName, ip, port, transferPort}`
//!   * kunci resume: sha1(`senderId|resumeId|fileName|fileSize`) → 16 hex
//!   * cookie `landrop_key`, header `X-Landrop-Key`, `?k=` → cookie + redirect
//!
//! Crate ini adalah inti aplikasi: server HTTP, discovery UDP, transfer TCP.
//! Yang memakainya ada dua: binari CLI (`lan-drop-core`) untuk mode terminal,
//! dan aplikasi desktop (`lan-drop`, Tauri) yang menjalankan server yang sama
//! di belakang jendela. Keduanya lewat `start_server`, jadi tidak ada jalur
//! kedua yang harus dirawat terpisah.

pub mod access;
pub mod discovery;
pub mod ffmpeg;
pub mod history;
pub mod httpapi;
pub mod logbus;
pub mod pin;
pub mod platform;
pub mod ports;
pub mod protocol;
pub mod repo;
pub mod security;
pub mod settings;
pub mod state;
pub mod thumbnail;
pub mod transfer;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use tokio::net::TcpListener;

use crate::discovery::Discovery;
use crate::history::History;
use crate::logbus::LogBus;
use crate::pin::Pin;
use crate::ports::Ports;
use crate::settings::Settings;
use crate::state::AppState;
use crate::thumbnail::Thumbnails;
use crate::transfer::Transfers;

const MAX_PORT_TRIES: u16 = 10;

fn env_port(name: &str, fallback: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(fallback)
}

fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(httpapi::static_fallback))
        .route("/manifest.webmanifest", get(httpapi::manifest))
        .route("/sw.js", get(httpapi::service_worker))
        .route("/dashboard", get(httpapi::dashboard))
        .route("/dashboard.html", get(httpapi::dashboard_legacy))
        .route("/dashboard/pin-info", get(httpapi::pin_info))
        .route("/dashboard/login", post(httpapi::dashboard_login))
        .route("/dashboard/logout", post(httpapi::dashboard_logout))
        .route("/dashboard/pin", post(httpapi::dashboard_set_pin))
        .route("/me", get(httpapi::me))
        .route("/status", get(httpapi::status))
        .route("/logs", get(httpapi::logs))
        .route("/devices", get(httpapi::devices))
        .route("/discovery/status", get(httpapi::discovery_status))
        .route("/transfer/active", get(httpapi::transfer_active))
        .route("/transfer/request", post(httpapi::transfer_request))
        .route(
            "/transfer/request-text",
            post(httpapi::transfer_request_text),
        )
        .route("/transfer/pending", get(httpapi::transfer_pending))
        .route("/transfer/respond", post(httpapi::transfer_respond))
        .route(
            "/transfer/status/{request_id}",
            get(httpapi::transfer_status),
        )
        .route(
            "/transfer/cancel/{request_id}",
            post(httpapi::transfer_cancel),
        )
        .route("/transfer/send-text", post(httpapi::transfer_send_text))
        .route("/transfer/send-file", post(httpapi::transfer_send_file))
        .route("/transfer/text", get(httpapi::transfer_text_list))
        .route("/transfer/text/{id}/ack", post(httpapi::transfer_text_ack))
        .route(
            "/transfer/progress/{request_id}",
            get(httpapi::transfer_progress),
        )
        // Batas body axum default 2 MB — cukup untuk semua rute lain, tapi
        // mematikan upload video. Batas sebenarnya dicek per berkas di handler
        // (LANDROP_MAX_UPLOAD_MB); di sini hanya longgarkan pintunya.
        .route(
            "/transfer/upload",
            post(httpapi::transfer_upload).layer(axum::extract::DefaultBodyLimit::max(
                platform::max_upload_mb() as usize * 1024 * 1024 + 16 * 1024 * 1024,
            )),
        )
        .route("/thumbnail/{id}", get(httpapi::thumbnail))
        .route("/system/disk", get(httpapi::system_disk))
        .route(
            "/settings/auto-approve",
            get(httpapi::auto_approve_get).post(httpapi::auto_approve_set),
        )
        .route("/history", get(httpapi::history_list))
        .route("/history/clear", post(httpapi::history_clear))
        .fallback(get(httpapi::static_fallback))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            httpapi::guard,
        ))
        .with_state(state)
}

/// Server yang sudah mendengarkan. Pemanggil boleh memakai portnya untuk
/// membuka jendela atau mencetak alamat, lalu menghentikannya lewat
/// `stop`.
pub struct ServerHandle {
    pub http_port: u16,
    pub transfer_port: u16,
    pub state: Arc<AppState>,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ServerHandle {
    /// Hentikan listener HTTP dengan rapi. Discovery dan transfer ikut mati
    /// bersama runtime saat proses berakhir.
    pub fn stop(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Nyalakan server di background task dan kembalikan alamat yang benar-benar
/// listen. `http_port` = `None` berarti ambil dari env `PORT` (default 3000).
pub async fn start_server(http_port: Option<u16>) -> Result<ServerHandle, String> {
    let data_dir = platform::data_dir();
    let settings = Arc::new(Settings::load(&data_dir));
    if settings.access_key().is_empty() {
        eprintln!(
            "[Rust] settings.json di {} belum punya accessKey.",
            data_dir.display()
        );
        std::process::exit(2);
    }

    let logbus = Arc::new(LogBus::new());
    let history = Arc::new(History::load(&data_dir));
    let pin = Arc::new(Pin::new(settings.clone()));
    // Cache thumbnail di folder data: bertahan antar restart, dan ikut hilang
    // kalau pengguna menghapus folder data (bukan folder simpan filenya).
    let thumbnails = Arc::new(Thumbnails::new(data_dir.join("thumbnails")));
    let transfers = Arc::new(Transfers::new());

    let http_port = match http_port {
        Some(port) => port,
        None => env_port("PORT", 3000),
    };
    let transfer_port = env_port("TRANSFER_PORT", http_port.saturating_add(1));
    // Port discovery default sama dengan Node; env dipakai supaya dua instance
    // bisa hidup berdampingan di satu host (pengujian, dua akun).
    let discovery_port = env_port("DISCOVERY_PORT", 41234);

    let ports = Arc::new(Ports::new(http_port, transfer_port));
    let discovery = Arc::new(Discovery::new(
        settings.device_id(),
        platform::device_name(),
        ports.clone(),
        discovery_port,
    ));

    let state = Arc::new(AppState::new(
        settings.clone(),
        pin.clone(),
        logbus.clone(),
        history,
        thumbnails,
        transfers,
        discovery.clone(),
        ports.clone(),
    ));

    // Bind HTTP dengan retry: `EADDRINUSE` tanpa handler = proses mati dengan
    // stack trace, dan user cuma tahu "gak jalan".
    let mut port = http_port;
    let mut listener: Option<TcpListener> = None;
    for _ in 0..=MAX_PORT_TRIES {
        let bind = SocketAddr::from(([0, 0, 0, 0], port));
        match TcpListener::bind(bind).await {
            Ok(l) => {
                listener = Some(l);
                break;
            }
            Err(err) => {
                eprintln!(
                    "⚠️ Port {port} is already taken by another app — LAN DROP moved to {}",
                    port + 1
                );
                let _ = err;
                port += 1;
            }
        }
    }
    let Some(listener) = listener else {
        eprintln!(
            "[Server] Port {} sampai +{} semuanya terpakai — jalankan dengan PORT=<angka lain>.",
            http_port, MAX_PORT_TRIES
        );
        std::process::exit(1);
    };
    ports.set_http(port);

    // Listener HTTP sudah hidup sebelum discovery/transfer: halaman yang dibuka
    // langsung setelah baris ini keluar tidak pernah kena "connection refused".
    discovery.start();
    transfer::start_server(state.clone());

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let app = router(state.clone());
    tokio::spawn(async move {
        if let Err(err) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = rx.await;
            println!("[Server] Shutting down...");
        })
        .await
        {
            eprintln!("[Server] berhenti: {err}");
        }
    });

    Ok(ServerHandle {
        http_port: port,
        transfer_port: state.ports.transfer(),
        state,
        stop_tx: Some(tx),
    })
}

/// Banner yang sama dengan versi Node — alamat dashboard, IP LAN, peringatan
/// PIN bawaan. Dipakai mode terminal dan log aplikasi desktop.
pub fn print_banner(handle: &ServerHandle) -> Vec<String> {
    let state = &handle.state;
    let port = handle.http_port;
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("LAN DROP server — Rust on http://localhost:{port}"));
    lines.push(format!("[Server] Device name: {}", state.device_name));
    let ips = platform::lan_interfaces();
    if ips.is_empty() {
        lines.push("\u{26a0}\u{fe0f}  No LAN IP found — check your WiFi connection".to_string());
    } else {
        for (name, ip, _) in &ips {
            lines.push(format!("\u{1f4e1} {name} \u{2192} http://{ip}:{port}"));
        }
    }
    if state.pin.is_default() {
        lines.push(format!(
            "\u{1f511} Dashboard PIN is still the default: {} — change it in the dashboard so Wi-Fi neighbours cannot open your transfer history.",
            state.pin.pin()
        ));
    }
    lines.push(format!(
        "[Server] Port {port} (HTTP) & {} (transfer) listening on 0.0.0.0 — other devices on the same network can reach it",
        handle.transfer_port
    ));
    let other = ips.first().map(|(_, ip, _)| ip.clone());
    lines.push(format!(
        "✅ LAN DROP is running — dashboard: http://localhost:{port}/dashboard{}",
        other
            .map(|ip| format!("  •  from another device: http://{ip}:{port}/dashboard"))
            .unwrap_or_default()
    ));
    for line in &lines {
        println!("{line}");
    }
    lines
}

/// Mode terminal: banner ke stdout, jalan sampai Ctrl+C.
pub async fn run_cli() {
    let handle = match start_server(None).await {
        Ok(handle) => handle,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };
    print_banner(&handle);
    let _ = tokio::signal::ctrl_c().await;
    handle.stop();
}

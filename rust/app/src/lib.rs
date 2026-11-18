//! LAN DROP — Tauri 2 (desktop + Android).
//!
//! Tidak ada Node sama sekali. Server HTTP, discovery UDP dan transfer TCP
//! dijalankan di proses yang sama lewat `lan_drop_core::start_server`, lalu
//! jendela memuat `http://127.0.0.1:<port>` seperti browser biasa. Jadi
//! `public/` tetap dipakai apa adanya, tanpa build step frontend, dan tidak ada
//! proses anak yang harus dijaga hidupnya.
//!
//! Jendela didefinisikan di `tauri.conf.json` (`label: main`) karena Android
//! tidak mengizinkan pembuatan jendela dari kode. Desktop hanya menampilkan,
//! menyembunyikan, atau mengarahkan ulang jendela itu.
//!
//! Mode terminal (`--core`/`--headless`) menjalankan `run_cli`, jadi satu
//! binary bisa dipakai di server tanpa GUI.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, RunEvent};
use lan_drop_core::{print_banner, start_server, ServerHandle};

const WINDOW: &str = "main";
/// Port yang ditulis di `tauri.conf.json`; kalau server terpaksa pindah port,
/// jendela diarahkan ulang ke port yang sebenarnya.
const CONFIG_PORT: u16 = 3000;

/// Server yang hidup selama aplikasi berjalan. Disimpan di state supaya bisa
/// dimatikan rapi saat keluar, bukan ikut mati bersama proses tanpa penutupan
/// listener.
struct CoreState {
    handle: Mutex<Option<ServerHandle>>,
    /// Dibaca menu tray (desktop). Di Android port datang dari konfigurasi.
    #[cfg_attr(mobile, allow(dead_code))]
    port: u16,
}

fn parse_port() -> Option<u16> {
    std::env::var("PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
}

#[cfg_attr(mobile, allow(dead_code))]
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Arahkan jendela ke port yang benar-benar dipakai server. Kalau port sesuai
/// konfigurasi, tidak ada navigasi ulang — halaman tidak berkedip.
#[cfg_attr(mobile, allow(dead_code))]
fn point_window(app: &AppHandle, port: u16) {
    if port == CONFIG_PORT {
        return;
    }
    if let Some(window) = app.get_webview_window(WINDOW) {
        if let Ok(url) = format!("http://127.0.0.1:{port}").parse::<tauri::Url>() {
            let _ = window.navigate(url);
        }
    }
}

#[cfg(desktop)]
mod desktop {
    use super::{show_window, CoreState, WINDOW};
    use lan_drop_core::platform;
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::{AppHandle, Manager};
    use tauri_plugin_clipboard_manager::ClipboardExt;

    /// URL dashboard yang bisa dibuka perangkat lain — dipakai menu tray
    /// "Salin alamat dashboard".
    fn dashboard_url(port: u16) -> String {
        let lan = platform::lan_interfaces()
            .into_iter()
            .next()
            .map(|(_, ip, _)| ip);
        match lan {
            Some(ip) => format!("http://{ip}:{port}/dashboard"),
            None => format!("http://localhost:{port}/dashboard"),
        }
    }

    fn open_in_browser(url: &str) -> Result<(), String> {
        let opener = if cfg!(target_os = "windows") {
            ("rundll32", vec!["url.dll,FileProtocolHandler", url])
        } else if cfg!(target_os = "macos") {
            ("open", vec![url])
        } else {
            ("xdg-open", vec![url])
        };
        std::process::Command::new(opener.0)
            .args(opener.1)
            .spawn()
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
        let open = MenuItem::with_id(app, "open", "Buka LAN DROP", true, None::<&str>)?;
        let browser = MenuItem::with_id(
            app,
            "browser",
            "Buka dashboard di browser",
            true,
            None::<&str>,
        )?;
        let copy = MenuItem::with_id(app, "copy", "Salin alamat dashboard", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &browser, &copy, &quit])?;

        let mut builder = TrayIconBuilder::with_id("lan-drop")
            .tooltip("LAN DROP — jalan di background")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| {
                let port = app.state::<CoreState>().port;
                match event.id().as_ref() {
                    "open" => show_window(app),
                    "browser" => {
                        let _ = open_in_browser(&format!("http://localhost:{port}/dashboard"));
                    }
                    "copy" => {
                        let _ = app.clipboard().write_text(dashboard_url(port));
                    }
                    "quit" => app.exit(0),
                    _ => {}
                }
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_window(tray.app_handle());
                }
            });

        // Ikon yang sama dengan PWA: satu aset, tidak ada gambar kedua yang
        // harus ikut dirawat.
        if let Some(icon) = app.default_window_icon().cloned() {
            builder = builder.icon(icon);
        }
        builder.build(app)?;
        Ok(())
    }

    /// Desktop: mulai tanpa jendela kalau `LANDROP_TRAY=1`, selain itu
    /// tampilkan jendela dari konfigurasi.
    pub fn arrange_window(app: &AppHandle) {
        if std::env::var("LANDROP_TRAY").as_deref() == Ok("1") {
            if let Some(window) = app.get_webview_window(WINDOW) {
                let _ = window.hide();
            }
        } else {
            show_window(app);
        }
    }

    pub fn init_plugins(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
        builder.plugin(tauri_plugin_clipboard_manager::init())
    }
}

/// Titik masuk aplikasi. Di Android fungsi ini yang dipanggil
/// `tauri::mobile_entry_point`; di desktop dipanggil `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Mode tanpa jendela: dipakai di server/headless dan saat `--core`
    // diminta. Tidak perlu memasang webview untuk sekadar melayani transfer.
    // Di Android argumen bukan milik kita, jadi jalur ini hanya untuk desktop.
    #[cfg(desktop)]
    {
        let headless = std::env::args().any(|a| a == "--core")
            || std::env::args().any(|a| a == "--headless")
            || std::env::var("LANDROP_HEADLESS").as_deref() == Ok("1");
        if headless {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(lan_drop_core::run_cli());
            return;
        }
    }

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = desktop::init_plugins(builder);
    }

    let app = builder
        .setup(|app| {
            let handle = app.handle().clone();
            let port_request = parse_port();
            // Server dinyalakan di runtime async milik Tauri; `block_on` di
            // sini aman karena setup berjalan sebelum event loop.
            let server = tauri::async_runtime::block_on(start_server(port_request))
                .map_err(|message| -> Box<dyn std::error::Error> { message.into() })?;
            let port = server.http_port;
            print_banner(&server);
            app.manage(CoreState {
                handle: Mutex::new(Some(server)),
                port,
            });
            point_window(&handle, port);

            #[cfg(desktop)]
            {
                if let Err(err) = desktop::build_tray(&handle) {
                    // Tray butuh appindicator di Linux (libayatana-appindicator3).
                    // Tanpa itu jangan matikan aplikasi: jendela tetap berguna.
                    eprintln!("[Tray] tidak tersedia: {err}");
                }
                desktop::arrange_window(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("gagal membangun aplikasi");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = app.try_state::<CoreState>() {
                if let Ok(mut guard) = state.handle.lock() {
                    if let Some(server) = guard.take() {
                        server.stop();
                    }
                }
            }
        }
    });
}

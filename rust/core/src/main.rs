//! Binari CLI: `lan-drop-core` menjalankan server di terminal, tanpa jendela.
//! Aplikasi desktop memakai crate yang sama lewat `lan_drop_core::start_server`.

#[tokio::main]
async fn main() {
    lan_drop_core::run_cli().await;
}

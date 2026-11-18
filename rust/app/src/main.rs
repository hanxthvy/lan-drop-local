//! Entrypoint desktop. Seluruh logikanya ada di `lib.rs` supaya Android bisa
//! memakai fungsi yang sama lewat `mobile_entry_point`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lan_drop_app::run();
}

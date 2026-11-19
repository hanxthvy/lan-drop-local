//! Aset frontend yang ikut dikompilasi ke dalam binary.
//!
//! Di desktop `public/` dibaca langsung dari disk, jadi perubahan UI tidak perlu
//! build ulang. Di Android tidak ada direktori repo sama sekali — APK hanya
//! berisi binary — sehingga seluruh isi `public/` harus ditanam saat kompilasi.
//! Kalau berkas tidak ditemukan di disk, jalur ini yang dipakai.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../public/"]
struct PublicAssets;

/// Isi berkas dari aset tertanam, atau `None` kalau namanya tidak ada.
pub fn get(rel: &str) -> Option<Vec<u8>> {
    PublicAssets::get(rel).map(|file| file.data.into_owned())
}

#[cfg(test)]
mod tests {
    use super::get;

    #[test]
    fn index_dan_aset_kunci_tertanam() {
        let index = get("index.html").expect("index.html harus tertanam");
        assert!(
            String::from_utf8_lossy(&index).contains("LAN"),
            "index.html tertanam tidak berisi UI"
        );
        assert!(
            get("js/app.js").is_some()
                || get("js/main.js").is_some()
                || get("js/core.js").is_some(),
            "skrip frontend tidak ikut tertanam"
        );
        assert!(
            get("../Cargo.toml").is_none(),
            "path keluar folder ikut tertanam"
        );
    }
}

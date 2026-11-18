//! Akses ke berkas repo (package.json, .git, public/) tanpa menyalin nilainya.

use std::path::{Path, PathBuf};

/// Akar repo: folder terdekat (dari crate ini ke atas) yang punya
/// `package.json`. Dulu `parent()` saja cukup karena crate ada di `rust/`,
/// tetapi setelah workspace dipecah jadi `rust/core`, satu tingkat ke atas
/// berhenti di `rust/` — dan versi yang dilaporkan `/status` jadi `0.0.0`.
pub fn root() -> PathBuf {
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dir = start.as_path();
    for _ in 0..6 {
        if dir.join("package.json").exists() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    start.parent().map(Path::to_path_buf).unwrap_or(start)
}

pub fn read_package_version() -> Option<String> {
    let text = std::fs::read_to_string(root().join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("version")?.as_str().map(str::to_string)
}

/// Commit yang sedang dijalankan, dibaca langsung dari `.git` (tanpa shell-out).
/// `None` kalau bukan repo git (mis. hasil build installer).
pub fn read_commit() -> Option<String> {
    let git = root().join(".git");
    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(rest) = head.strip_prefix("ref:") {
        let path = git.join(rest.trim());
        let sha = std::fs::read_to_string(path).ok()?;
        return Some(sha.trim().chars().take(7).collect());
    }
    Some(head.chars().take(7).collect())
}

pub fn public_dir() -> PathBuf {
    if let Ok(p) = std::env::var("LANDROP_PUBLIC_DIR") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    for candidate in [root().join("public"), PathBuf::from("public")] {
        if candidate.join("index.html").exists() {
            return candidate;
        }
    }
    PathBuf::from("public")
}

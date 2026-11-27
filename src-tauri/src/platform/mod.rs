//! Everything that differs per operating system: opening, hiding and quitting apps,
//! finding running apps and reading app icons.
//!
//! Each OS module provides:
//! - `APP_EXTENSIONS`, `DEFAULT_APP_DIR`
//! - `identity(entry)`: the key a running app is known by (bundle path, .exe path, binary)
//! - `running()`: running apps by identity, with "hidden" state where the OS has one
//! - `open(entry, args)`, `hide(entry)`, `unhide(entry)`, `quit(entry)`
//! - `icon(entry, cache_dir) -> (bytes, mime)`, `app_name(entry)`

#[derive(serde::Serialize, Clone, Debug)]
pub struct Running {
    /// Identity on the platform side; the app entry's path when sent to the UI.
    pub path: String,
    pub hidden: bool,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

/// File name without extension (fallback display name).
pub fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Entries whose app is running, keyed by the entry's own path (what the UI knows).
pub fn running_entries(entry_paths: &[String]) -> Vec<Running> {
    let running = running();
    entry_paths
        .iter()
        .filter_map(|entry| {
            let id = identity(entry);
            running.iter().find(|r| r.path == id).map(|r| Running { path: entry.clone(), hidden: r.hidden })
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

//! OS / filesystem / PATH introspection. Pure read-only helpers.
//!
//! TODO(impl): fill in every function; see doc comments for the contract.

use std::path::PathBuf;

use crate::models::{OsInfo, Platform};

pub fn platform() -> Platform {
    match std::env::consts::OS {
        "windows" => Platform::Windows,
        "macos" => Platform::Macos,
        "linux" => Platform::Linux,
        _ => Platform::Unknown,
    }
}

pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Expands a leading `~` to the home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if let Some(home) = home_dir() {
            return home.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(path)
}

/// Best-effort OS description (version, build number on Windows, arch, default shell, admin?).
pub fn os_info() -> OsInfo {
    // TODO(impl): use os_info crate + registry (Windows build) + $SHELL / ComSpec + admin check
    OsInfo {
        platform: platform(),
        version: String::new(),
        build: None,
        arch: std::env::consts::ARCH.to_owned(),
        shell: String::new(),
        home_dir: home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        is_admin: None,
    }
}

/// Finds an executable by name on the current `PATH` (`which`), including `.cmd`/`.exe`
/// shims on Windows.
pub fn find_on_path(binary: &str) -> Option<PathBuf> {
    which::which(binary).ok()
}

//! Node.js LTS installer discovery for the one-click Node install: which file to download for
//! this machine, from the Node dist mirror the network probe chose, and its SHA-256.
//!
//! The dist layout is identical on nodejs.org and the npmmirror mirror:
//!
//! ```text
//! <mirror>/index.json                         [{ "version": "v24.19.0", "lts": "Krypton", … }, …]
//! <mirror>/v24.19.0/node-v24.19.0-x64.msi     Windows x64 (per-machine MSI, asks for admin)
//! <mirror>/v24.19.0/node-v24.19.0-arm64.msi   Windows arm64
//! <mirror>/v24.19.0/node-v24.19.0.pkg         macOS universal pkg (Installer.app asks for admin)
//! <mirror>/v24.19.0/SHASUMS256.txt            `<hex>  <file>` lines
//! ```
//!
//! `select_lts` / `asset_name` / `parse_sha256_listing` are pure and unit-tested; only
//! `latest_lts` talks to the network (two small GETs, 10 s each).

use std::time::Duration;

use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::{InstallTarget, InstallerRelease, MirrorEntry, Platform};

use super::cc_switch::parse_sha256_listing;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// `index.json` is ~1 MB today; refuse anything absurd.
const MAX_INDEX_BYTES: usize = 16 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: usize = 256 * 1024;

/// One entry of `index.json` (only the fields used).
#[derive(Debug, Clone, Deserialize)]
pub struct DistEntry {
    pub version: String,
    /// `false` or the LTS code name (`"Krypton"`).
    #[serde(default)]
    pub lts: serde_json::Value,
}

/// Newest LTS entry of the index (entries are newest-first; `lts` is a string for LTS lines).
pub fn select_lts(entries: &[DistEntry]) -> Option<&DistEntry> {
    entries
        .iter()
        .filter(|e| e.lts.is_string())
        .max_by(|a, b| version_key(&a.version).cmp(&version_key(&b.version)))
}

fn version_key(v: &str) -> (u64, u64, u64) {
    let mut parts = v
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u64>().ok());
    let major = parts.next().flatten().unwrap_or(0);
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    (major, minor, patch)
}

/// Installer file name for `platform` / `arch` at `version` (`v24.19.0`). `None` when there is
/// no official installer (Linux, unknown arch).
pub fn asset_name(version: &str, platform: Platform, arch: &str) -> Option<String> {
    let v = version.trim();
    match platform {
        Platform::Windows => {
            let arch = match arch {
                "x86_64" | "x64" | "amd64" => "x64",
                "aarch64" | "arm64" => "arm64",
                "x86" | "i686" => "x86",
                _ => return None,
            };
            Some(format!("node-{v}-{arch}.msi"))
        }
        Platform::Macos => Some(format!("node-{v}.pkg")),
        Platform::Linux | Platform::Unknown => None,
    }
}

/// `<mirror>/<version>/<file>` (mirror URL with or without trailing slash).
pub fn asset_url(mirror_url: &str, version: &str, file: &str) -> String {
    format!(
        "{}/{}/{}",
        mirror_url.trim_end_matches('/'),
        version.trim(),
        file
    )
}

/// Resolves the newest LTS installer for this machine from `mirror`.
pub async fn latest_lts(
    client: &reqwest::Client,
    mirror: &MirrorEntry,
    platform: Platform,
    arch: &str,
) -> AppResult<InstallerRelease> {
    let base = mirror.url.trim();
    if base.is_empty() {
        return Err(AppError::Config("node dist mirror url is empty".to_owned()));
    }
    let index_url = format!("{}/index.json", base.trim_end_matches('/'));
    let text = fetch_text(client, &index_url, MAX_INDEX_BYTES).await?;
    let entries: Vec<DistEntry> =
        serde_json::from_str(&text).map_err(|e| AppError::Serde(format!("{index_url}: {e}")))?;
    let lts = select_lts(&entries)
        .ok_or_else(|| AppError::Other(format!("{index_url}: no LTS release listed")))?;
    let file = asset_name(&lts.version, platform, arch).ok_or_else(|| {
        AppError::Unsupported(format!("no Node.js installer for {platform:?}/{arch}"))
    })?;
    let sums_url = asset_url(base, &lts.version, "SHASUMS256.txt");
    let sha256 = match fetch_text(client, &sums_url, MAX_CHECKSUM_BYTES).await {
        Ok(sums) => parse_sha256_listing(&sums, &file),
        Err(e) => {
            log::warn!("SHASUMS256.txt unavailable from {sums_url}: {e}");
            None
        }
    };
    log::info!(
        "node lts {} from {}: {file} (sha256 {})",
        lts.version,
        mirror.id,
        if sha256.is_some() { "known" } else { "unknown" }
    );
    Ok(InstallerRelease {
        target: InstallTarget::Node,
        version: lts.version.clone(),
        asset_name: file.clone(),
        download_url: asset_url(base, &lts.version, &file),
        sha256,
        source: mirror.id.clone(),
        requires_admin: true,
    })
}

async fn fetch_text(client: &reqwest::Client, url: &str, max: usize) -> AppResult<String> {
    let response = client.get(url).timeout(REQUEST_TIMEOUT).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http {
            status: status.as_u16(),
            url: url.to_owned(),
        });
    }
    let bytes = response.bytes().await?;
    if bytes.len() > max {
        return Err(AppError::Other(format!("{url}: response too large")));
    }
    String::from_utf8(bytes.to_vec()).map_err(|e| AppError::Other(format!("{url}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(version: &str, lts: serde_json::Value) -> DistEntry {
        DistEntry {
            version: version.to_owned(),
            lts,
        }
    }

    #[test]
    fn picks_the_newest_lts_not_the_newest_release() {
        let entries = vec![
            entry("v26.7.0", serde_json::Value::Bool(false)),
            entry("v24.19.0", serde_json::json!("Krypton")),
            entry("v24.18.0", serde_json::json!("Krypton")),
            entry("v22.20.0", serde_json::json!("Jod")),
        ];
        assert_eq!(
            select_lts(&entries).map(|e| e.version.as_str()),
            Some("v24.19.0")
        );
        assert!(select_lts(&[entry("v26.0.0", serde_json::Value::Bool(false))]).is_none());
    }

    #[test]
    fn asset_names_per_platform() {
        assert_eq!(
            asset_name("v24.19.0", Platform::Windows, "x86_64").as_deref(),
            Some("node-v24.19.0-x64.msi")
        );
        assert_eq!(
            asset_name("v24.19.0", Platform::Windows, "aarch64").as_deref(),
            Some("node-v24.19.0-arm64.msi")
        );
        assert_eq!(
            asset_name("v24.19.0", Platform::Macos, "aarch64").as_deref(),
            Some("node-v24.19.0.pkg")
        );
        assert_eq!(asset_name("v24.19.0", Platform::Linux, "x86_64"), None);
        assert_eq!(asset_name("v24.19.0", Platform::Windows, "mips"), None);
        assert_eq!(
            asset_url("https://nodejs.org/dist/", "v24.19.0", "SHASUMS256.txt"),
            "https://nodejs.org/dist/v24.19.0/SHASUMS256.txt"
        );
    }

    #[test]
    fn index_json_shape_parses() {
        let text = r#"[{"version":"v26.7.0","date":"2026-08-12","lts":false,"security":false},
                       {"version":"v24.19.0","date":"2026-08-03","lts":"Krypton","security":false}]"#;
        let entries: Vec<DistEntry> = serde_json::from_str(text).expect("parse");
        assert_eq!(
            select_lts(&entries).map(|e| e.version.as_str()),
            Some("v24.19.0")
        );
    }
}

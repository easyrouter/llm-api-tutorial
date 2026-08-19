//! CC Switch release discovery: which installer to download for this machine, from where,
//! and (when the publisher provides one) its SHA-256.
//!
//! Sources (in order of precedence)
//! 1. **Intranet mirror** — when `ccSwitch.intranetMirror` is non-empty, `GET
//!    <intranetMirror>/latest.json` must return
//!    ```json
//!    {
//!      "version": "3.2.1",
//!      "assets": [
//!        { "name": "CC-Switch_3.2.1_x64-setup.exe", "url": "https://mirror/…/x64-setup.exe",
//!          "sha256": "<64 hex chars, optional>" }
//!      ]
//!    }
//!    ```
//!    Asset `name`s follow the GitHub release naming so the same selection heuristics apply;
//!    `sha256` is optional but strongly recommended (`source = "intranet"`).
//! 2. **GitHub releases API** — `ccSwitch.releasesApi` (`…/releases/latest`), parsed for
//!    `tag_name` + `assets[] { name, browser_download_url, digest? }` (`source = "github"`).
//!
//! Asset selection (`select_asset`, pure): Windows prefers `*-setup.exe`, then `.msi`, then
//! any `.exe`; macOS takes `.dmg`. Within a kind, an asset naming the host architecture wins
//! (`x64`/`x86_64`/`amd64` vs `arm64`/`aarch64`), then `universal`, then arch-less names;
//! assets naming a *different* architecture are never chosen.
//!
//! SHA-256 lookup order: GitHub `digest` field / intranet `sha256` → sibling asset
//! `<asset>.sha256` → `SHA256SUMS*` asset (both fetched with a 64 KiB cap and parsed with
//! `parse_sha256_listing`). Absence is not an error: the download is then reported as
//! unverified (`DownloadResult.verified == None`).

use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::{AppConfig, InstallTarget, InstallerRelease, Platform};
use crate::redact::redact_secrets;

/// Timeout for metadata requests (release JSON, checksum files).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Checksum files are tiny; anything larger is refused (guards against a wrong asset).
const MAX_CHECKSUM_BYTES: u64 = 64 * 1024;
/// GitHub REST API media type.
const GITHUB_ACCEPT: &str = "application/vnd.github+json";
/// GitHub REST API version pin.
const GITHUB_API_VERSION: &str = "2022-11-28";

const SOURCE_GITHUB: &str = "github";
const SOURCE_INTRANET: &str = "intranet";

// ---------------------------------------------------------------------------
// Wire formats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    /// `sha256:<hex>` — present on newer GitHub API responses.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct IntranetManifest {
    version: String,
    #[serde(default)]
    assets: Vec<IntranetAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct IntranetAsset {
    name: String,
    url: String,
    #[serde(default)]
    sha256: Option<String>,
}

/// Source-agnostic release view fed into the selection logic.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseListing {
    version: String,
    source: &'static str,
    assets: Vec<AssetRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetRef {
    name: String,
    url: String,
    /// Publisher-supplied hash (already normalised to bare lower-case hex), if any.
    sha256: Option<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Resolves the CC Switch installer for `platform`/`arch` (see module docs).
pub async fn latest_release(
    client: &reqwest::Client,
    config: &AppConfig,
    platform: Platform,
    arch: &str,
) -> AppResult<InstallerRelease> {
    let spec = &config.cc_switch;
    let mirror = spec.intranet_mirror.trim();
    let listing = if mirror.is_empty() {
        fetch_github(client, spec.releases_api.trim()).await?
    } else {
        fetch_intranet(client, mirror).await?
    };

    let names: Vec<String> = listing.assets.iter().map(|a| a.name.clone()).collect();
    let index = select_asset(&names, platform, arch).ok_or_else(|| {
        AppError::Unsupported(format!(
            "no CC Switch installer for {platform:?}/{arch} in {} {}",
            listing.source, listing.version
        ))
    })?;
    let asset = &listing.assets[index];
    let sha256 = match &asset.sha256 {
        Some(hash) => Some(hash.clone()),
        None => find_sibling_sha256(client, &listing.assets, &asset.name).await,
    };
    log::info!(
        "cc-switch release {} from {}: asset {} (sha256 {})",
        listing.version,
        listing.source,
        asset.name,
        if sha256.is_some() { "known" } else { "unknown" }
    );
    Ok(InstallerRelease {
        target: InstallTarget::CcSwitch,
        version: listing.version,
        asset_name: asset.name.clone(),
        download_url: asset.url.clone(),
        sha256,
        source: listing.source.to_owned(),
        requires_admin: false,
    })
}

// ---------------------------------------------------------------------------
// Fetching
// ---------------------------------------------------------------------------

async fn fetch_github(client: &reqwest::Client, api_url: &str) -> AppResult<ReleaseListing> {
    if api_url.is_empty() {
        return Err(AppError::Config("ccSwitch.releasesApi is empty".to_owned()));
    }
    let response = client
        .get(api_url)
        .header(reqwest::header::ACCEPT, GITHUB_ACCEPT)
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await?;
    let response = ensure_success(response, api_url)?;
    let release: GithubRelease = response.json().await?;
    Ok(listing_from_github(release))
}

async fn fetch_intranet(client: &reqwest::Client, mirror: &str) -> AppResult<ReleaseListing> {
    let url = format!("{}/latest.json", mirror.trim_end_matches('/'));
    let response = client.get(&url).timeout(REQUEST_TIMEOUT).send().await?;
    let response = ensure_success(response, &url)?;
    let manifest: IntranetManifest = response.json().await?;
    Ok(listing_from_intranet(manifest))
}

fn ensure_success(response: reqwest::Response, url: &str) -> AppResult<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(AppError::Http {
            status: status.as_u16(),
            url: redact_secrets(url),
        })
    }
}

fn listing_from_github(release: GithubRelease) -> ReleaseListing {
    ReleaseListing {
        version: normalise_version(&release.tag_name),
        source: SOURCE_GITHUB,
        assets: release
            .assets
            .into_iter()
            .map(|a| AssetRef {
                name: a.name,
                url: a.browser_download_url,
                sha256: a.digest.as_deref().and_then(normalise_sha256),
            })
            .collect(),
    }
}

fn listing_from_intranet(manifest: IntranetManifest) -> ReleaseListing {
    ReleaseListing {
        version: normalise_version(&manifest.version),
        source: SOURCE_INTRANET,
        assets: manifest
            .assets
            .into_iter()
            .map(|a| AssetRef {
                name: a.name,
                url: a.url,
                sha256: a.sha256.as_deref().and_then(normalise_sha256),
            })
            .collect(),
    }
}

/// `v3.2.1` → `3.2.1` (display only; the tag itself is not needed downstream).
fn normalise_version(tag: &str) -> String {
    let trimmed = tag.trim();
    trimmed
        .strip_prefix('v')
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(trimmed)
        .to_owned()
}

/// Accepts `<hex>` or `sha256:<hex>`; returns bare lower-case hex or `None` when malformed.
fn normalise_sha256(value: &str) -> Option<String> {
    let v = value.trim();
    let v = v
        .strip_prefix("sha256:")
        .or_else(|| v.strip_prefix("SHA256:"))
        .unwrap_or(v)
        .trim();
    is_sha256_hex(v).then(|| v.to_ascii_lowercase())
}

/// Reads a small text asset (checksum files); refuses bodies over [`MAX_CHECKSUM_BYTES`].
async fn fetch_small_text(client: &reqwest::Client, url: &str) -> AppResult<String> {
    let response = client.get(url).timeout(REQUEST_TIMEOUT).send().await?;
    let response = ensure_success(response, url)?;
    let too_large = || AppError::InvalidInput("checksum file too large".to_owned());
    if response
        .content_length()
        .is_some_and(|len| len > MAX_CHECKSUM_BYTES)
    {
        return Err(too_large());
    }
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let next_len = u64::try_from(buf.len().saturating_add(chunk.len())).unwrap_or(u64::MAX);
        if next_len > MAX_CHECKSUM_BYTES {
            return Err(too_large());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Looks for `<asset>.sha256` then `SHA256SUMS*` among the release assets and extracts the
/// hash for `asset_name`. Best effort: fetch/parse failures are logged and yield `None`.
async fn find_sibling_sha256(
    client: &reqwest::Client,
    assets: &[AssetRef],
    asset_name: &str,
) -> Option<String> {
    let names: Vec<String> = assets.iter().map(|a| a.name.clone()).collect();
    for index in checksum_candidates(&names, asset_name) {
        let candidate = &assets[index];
        match fetch_small_text(client, &candidate.url).await {
            Ok(text) => {
                let found = parse_sha256_listing(&text, asset_name).or_else(|| {
                    is_dedicated_checksum(&candidate.name, asset_name)
                        .then(|| parse_bare_sha256(&text))
                        .flatten()
                });
                if found.is_some() {
                    return found;
                }
                log::info!(
                    "checksum asset {} does not list {asset_name}",
                    candidate.name
                );
            }
            Err(e) => log::warn!("cannot read checksum asset {}: {e}", candidate.name),
        }
    }
    None
}

/// Indices of assets that may carry the hash of `asset_name`: the dedicated `<asset>.sha256`
/// first, then every `SHA256SUMS*` file (pure).
pub fn checksum_candidates(names: &[String], asset_name: &str) -> Vec<usize> {
    let mut dedicated = Vec::new();
    let mut sums = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if is_dedicated_checksum(name, asset_name) {
            dedicated.push(i);
        } else if name.to_ascii_lowercase().starts_with("sha256sums") {
            sums.push(i);
        }
    }
    dedicated.extend(sums);
    dedicated
}

fn is_dedicated_checksum(name: &str, asset_name: &str) -> bool {
    name.len() == asset_name.len() + ".sha256".len()
        && name
            .strip_suffix(".sha256")
            .or_else(|| name.strip_suffix(".SHA256"))
            .is_some_and(|stem| stem.eq_ignore_ascii_case(asset_name))
}

// ---------------------------------------------------------------------------
// Asset selection (pure)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostArch {
    X64,
    Arm64,
    Other,
}

const X64_TOKENS: [&str; 3] = ["x86_64", "amd64", "x64"];
const ARM64_TOKENS: [&str; 2] = ["aarch64", "arm64"];
const X86_32_TOKENS: [&str; 2] = ["ia32", "i686"];

fn host_arch(arch: &str) -> HostArch {
    match arch.trim().to_ascii_lowercase().as_str() {
        "x86_64" | "x64" | "amd64" => HostArch::X64,
        "aarch64" | "arm64" => HostArch::Arm64,
        _ => HostArch::Other,
    }
}

/// Case-insensitive file-extension test (`ext` without the dot).
fn has_extension(name: &str, ext: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Installer kind rank for `platform` (`None` = not an installer for that platform).
fn kind_rank(name_lower: &str, platform: Platform) -> Option<u32> {
    match platform {
        Platform::Windows => {
            if name_lower.ends_with("setup.exe") {
                Some(3)
            } else if has_extension(name_lower, "msi") {
                Some(2)
            } else if has_extension(name_lower, "exe") {
                Some(1)
            } else {
                None
            }
        }
        Platform::Macos => has_extension(name_lower, "dmg").then_some(1),
        Platform::Linux | Platform::Unknown => None,
    }
}

/// Architecture affinity rank (`None` = built for a different architecture).
fn arch_rank(name_lower: &str, arch: HostArch) -> Option<u32> {
    let has = |tokens: &[&str]| tokens.iter().any(|t| name_lower.contains(t));
    let x64 = has(&X64_TOKENS);
    let arm64 = has(&ARM64_TOKENS);
    let x86_32 = has(&X86_32_TOKENS) || (name_lower.contains("x86") && !x64);
    let neutral = if name_lower.contains("universal") {
        2
    } else {
        1
    };
    match arch {
        HostArch::X64 => {
            if x64 {
                Some(3)
            } else if arm64 {
                None
            } else if x86_32 {
                Some(0)
            } else {
                Some(neutral)
            }
        }
        HostArch::Arm64 => {
            if arm64 {
                Some(3)
            } else if x64 || x86_32 {
                None
            } else {
                Some(neutral)
            }
        }
        HostArch::Other => Some(neutral),
    }
}

/// Picks the best installer among release asset `names` for `platform`/`arch` (see module
/// docs for the ranking). Returns the index into `names`; ties keep the first.
pub fn select_asset(names: &[String], platform: Platform, arch: &str) -> Option<usize> {
    let host = host_arch(arch);
    names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let lower = name.to_ascii_lowercase();
            let kind = kind_rank(&lower, platform)?;
            let arch = arch_rank(&lower, host)?;
            Some((i, arch * 10 + kind))
        })
        // `max_by_key` keeps the *last* maximum; reverse so the first one wins ties.
        .rev()
        .max_by_key(|&(_, score)| score)
        .map(|(i, _)| i)
}

// ---------------------------------------------------------------------------
// Checksum parsing (pure)
// ---------------------------------------------------------------------------

fn is_sha256_hex(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Strips the decorations checksum tools put around file names (`*name`, `./name`,
/// `(name)`, `name:`).
fn clean_file_token(token: &str) -> &str {
    let t = token
        .trim_matches(|c: char| matches!(c, '*' | '(' | ')' | ':' | '"' | '\''))
        .trim();
    t.strip_prefix("./").unwrap_or(t)
}

/// Extracts the SHA-256 for `asset_name` from checksum-list text. Understands the common
/// layouts: `<hex>  <name>`, `<hex> *<name>`, `<name>: <hex>`, `SHA256 (<name>) = <hex>`.
/// Returns lower-case hex; `None` when the asset is not listed.
pub fn parse_sha256_listing(text: &str, asset_name: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let hex = tokens
                .iter()
                .map(|t| t.trim_matches(|c: char| matches!(c, ':' | ',' | ';')))
                .find(|t| is_sha256_hex(t))?;
            let names_asset = tokens
                .iter()
                .map(|t| clean_file_token(t))
                .any(|t| t.eq_ignore_ascii_case(asset_name));
            names_asset.then(|| hex.to_ascii_lowercase())
        })
}

/// For a checksum file dedicated to one asset (`<asset>.sha256`): the first hex-looking token
/// of the first non-empty line, lower-cased.
pub fn parse_bare_sha256(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.split_whitespace().next())
        .and_then(normalise_sha256)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const HEX_B: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    const TAURI_RELEASE: [&str; 8] = [
        "CC-Switch_3.2.1_x64-setup.exe",
        "CC-Switch_3.2.1_x64_en-US.msi",
        "CC-Switch_3.2.1_arm64-setup.exe",
        "CC-Switch_3.2.1_aarch64.dmg",
        "CC-Switch_3.2.1_x64.dmg",
        "CC-Switch_3.2.1_x64-setup.exe.sig",
        "CC-Switch_3.2.1_x64-setup.nsis.zip",
        "SHA256SUMS.txt",
    ];

    /// (label, asset names, platform, host arch, expected index)
    type SelectCase = (
        &'static str,
        Vec<&'static str>,
        Platform,
        &'static str,
        Option<usize>,
    );

    #[test]
    fn select_asset_table() {
        let cases: Vec<SelectCase> = vec![
            (
                "win x64 prefers setup.exe",
                TAURI_RELEASE.to_vec(),
                Platform::Windows,
                "x86_64",
                Some(0),
            ),
            (
                "win arm64",
                TAURI_RELEASE.to_vec(),
                Platform::Windows,
                "aarch64",
                Some(2),
            ),
            (
                "mac apple silicon",
                TAURI_RELEASE.to_vec(),
                Platform::Macos,
                "aarch64",
                Some(3),
            ),
            (
                "mac intel",
                TAURI_RELEASE.to_vec(),
                Platform::Macos,
                "x86_64",
                Some(4),
            ),
            (
                "linux unsupported",
                TAURI_RELEASE.to_vec(),
                Platform::Linux,
                "x86_64",
                None,
            ),
            ("empty", vec![], Platform::Windows, "x86_64", None),
            (
                "arch-less exe accepted",
                vec!["cc-switch-setup.exe", "cc-switch.dmg"],
                Platform::Windows,
                "x86_64",
                Some(0),
            ),
            (
                "matching arch beats installer kind",
                vec!["cc-switch-setup.exe", "cc-switch_x64.msi"],
                Platform::Windows,
                "x86_64",
                Some(1),
            ),
            (
                "msi over plain exe",
                vec!["cc-switch_x64.exe", "cc-switch_x64.msi"],
                Platform::Windows,
                "x86_64",
                Some(1),
            ),
            (
                "wrong arch never chosen",
                vec!["cc-switch_x64-setup.exe", "cc-switch_amd64.msi"],
                Platform::Windows,
                "aarch64",
                None,
            ),
            (
                "32-bit ranks below arch-less on x64",
                vec!["cc_x86-setup.exe", "cc-setup.exe"],
                Platform::Windows,
                "x86_64",
                Some(1),
            ),
            (
                "x86_64 is not 32-bit",
                vec!["cc_x86-setup.exe", "cc_x86_64-setup.exe"],
                Platform::Windows,
                "x86_64",
                Some(1),
            ),
            (
                "mac universal fallback",
                vec!["cc_universal.dmg", "cc_x64.dmg"],
                Platform::Macos,
                "aarch64",
                Some(0),
            ),
            (
                "mac any dmg fallback",
                vec!["cc.dmg", "cc_x64-setup.exe"],
                Platform::Macos,
                "aarch64",
                Some(0),
            ),
            (
                "unknown arch picks by kind",
                vec!["cc_x64.msi", "cc_arm64-setup.exe"],
                Platform::Windows,
                "riscv64",
                Some(1),
            ),
            (
                "case-insensitive names",
                vec!["CC-SWITCH_X64-SETUP.EXE"],
                Platform::Windows,
                "x86_64",
                Some(0),
            ),
            (
                "ties keep first",
                vec!["a_x64-setup.exe", "b_x64-setup.exe"],
                Platform::Windows,
                "x86_64",
                Some(0),
            ),
        ];
        for (label, list, platform, arch, expected) in cases {
            assert_eq!(
                select_asset(&names(&list), platform, arch),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn parse_listing_formats() {
        let asset = "CC-Switch_3.2.1_x64-setup.exe";
        let cases = [
            (
                "gnu two-space",
                format!("{HEX_B}  other.dmg\n{HEX_A}  {asset}\n"),
            ),
            ("gnu binary marker", format!("{HEX_A} *{asset}")),
            ("relative path", format!("{HEX_A}  ./{asset}")),
            ("name colon hex", format!("{asset}: {HEX_A}")),
            ("bsd style", format!("SHA256 ({asset}) = {HEX_A}")),
            (
                "upper-case hex",
                format!("{}  {asset}", HEX_A.to_ascii_uppercase()),
            ),
            (
                "comment lines skipped",
                format!("# generated\n\n{HEX_A}  {asset}"),
            ),
            (
                "case-insensitive name",
                format!("{HEX_A}  {}", asset.to_ascii_lowercase()),
            ),
        ];
        for (label, text) in cases {
            assert_eq!(
                parse_sha256_listing(&text, asset).as_deref(),
                Some(HEX_A),
                "{label}"
            );
        }
    }

    #[test]
    fn parse_listing_misses() {
        let asset = "CC-Switch_3.2.1_x64-setup.exe";
        let cases = [
            ("not listed", format!("{HEX_A}  other-setup.exe")),
            ("prefix only", format!("{HEX_A}  {asset}.sig")),
            ("short hash", format!("{}  {asset}", &HEX_A[..40])),
            ("no hash", asset.to_owned()),
            ("empty", String::new()),
        ];
        for (label, text) in cases {
            assert_eq!(parse_sha256_listing(&text, asset), None, "{label}");
        }
    }

    #[test]
    fn parse_bare_hash() {
        assert_eq!(
            parse_bare_sha256(&format!("{HEX_A}\n")).as_deref(),
            Some(HEX_A)
        );
        assert_eq!(
            parse_bare_sha256(&format!("  {}  file.exe", HEX_A.to_ascii_uppercase())).as_deref(),
            Some(HEX_A)
        );
        assert_eq!(
            parse_bare_sha256(&format!("sha256:{HEX_A}")).as_deref(),
            Some(HEX_A)
        );
        assert_eq!(parse_bare_sha256("not a hash"), None);
        assert_eq!(parse_bare_sha256(""), None);
        assert_eq!(parse_bare_sha256(&HEX_A[..63]), None);
    }

    #[test]
    fn checksum_candidates_prefers_dedicated_then_sums() {
        let list = names(&[
            "SHA256SUMS",
            "CC-Switch_x64-setup.exe",
            "CC-Switch_x64-setup.exe.sha256",
            "sha256sums.txt",
            "CC-Switch_x64-setup.exe.sig",
        ]);
        assert_eq!(
            checksum_candidates(&list, "CC-Switch_x64-setup.exe"),
            vec![2, 0, 3]
        );
        assert_eq!(checksum_candidates(&list, "other.dmg"), vec![0, 3]);
        assert!(checksum_candidates(&names(&["a.exe"]), "a.exe").is_empty());
    }

    #[test]
    fn version_and_digest_normalisation() {
        assert_eq!(normalise_version("v3.2.1"), "3.2.1");
        assert_eq!(normalise_version("3.2.1"), "3.2.1");
        assert_eq!(normalise_version(" vNext "), "vNext");
        assert_eq!(
            normalise_sha256(&format!("sha256:{HEX_A}")).as_deref(),
            Some(HEX_A)
        );
        assert_eq!(
            normalise_sha256(HEX_A.to_ascii_uppercase().as_str()).as_deref(),
            Some(HEX_A)
        );
        assert_eq!(normalise_sha256("sha1:abc"), None);
        assert_eq!(normalise_sha256(""), None);
    }

    #[test]
    fn github_release_json_maps_to_listing() {
        let json = format!(
            r#"{{
              "tag_name": "v3.2.1",
              "name": "CC Switch 3.2.1",
              "assets": [
                {{ "name": "CC-Switch_3.2.1_x64-setup.exe",
                   "browser_download_url": "https://github.com/x/y/releases/download/v3.2.1/CC-Switch_3.2.1_x64-setup.exe",
                   "digest": "sha256:{HEX_A}", "size": 123 }},
                {{ "name": "CC-Switch_3.2.1_aarch64.dmg",
                   "browser_download_url": "https://github.com/x/y/releases/download/v3.2.1/CC-Switch_3.2.1_aarch64.dmg" }}
              ]
            }}"#
        );
        let release: GithubRelease = serde_json::from_str(&json).expect("parses");
        let listing = listing_from_github(release);
        assert_eq!(listing.version, "3.2.1");
        assert_eq!(listing.source, "github");
        assert_eq!(listing.assets.len(), 2);
        assert_eq!(listing.assets[0].sha256.as_deref(), Some(HEX_A));
        assert_eq!(listing.assets[1].sha256, None);
        assert!(listing.assets[1].url.ends_with("aarch64.dmg"));
    }

    #[test]
    fn intranet_manifest_json_maps_to_listing() {
        let json = format!(
            r#"{{ "version": "3.2.1", "assets": [
                 {{ "name": "CC-Switch_3.2.1_x64-setup.exe", "url": "https://mirror.corp/cc/x64-setup.exe", "sha256": "{HEX_A}" }},
                 {{ "name": "CC-Switch_3.2.1_x64.dmg", "url": "https://mirror.corp/cc/x64.dmg" }} ] }}"#
        );
        let manifest: IntranetManifest = serde_json::from_str(&json).expect("parses");
        let listing = listing_from_intranet(manifest);
        assert_eq!(listing.source, "intranet");
        assert_eq!(listing.assets[0].sha256.as_deref(), Some(HEX_A));
        assert_eq!(listing.assets[1].sha256, None);
        let picked = select_asset(
            &listing
                .assets
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>(),
            Platform::Windows,
            "x86_64",
        );
        assert_eq!(picked, Some(0));
    }
}

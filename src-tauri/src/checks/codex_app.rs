//! Codex desktop client check (recommendation): is the ChatGPT / Codex app installed?
//!
//! Since July 2026 the Codex app ships inside the ChatGPT desktop app (bundle id
//! `com.openai.codex`); on Windows it is a Store-signed MSIX (`OpenAI.Codex`), on macOS
//! `/Applications/ChatGPT.app` (older installs: `Codex.app`).
//!
//! Codes: `codex_app.ok` (params `path`, `version`), `codex_app.missing` (Warn — fixes:
//! install via the wizard, open the Store page / region settings on Windows),
//! `codex_app.not_applicable` (Skipped on other platforms).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::{CodexAppSpec, FixAction, InstallTarget, Platform, SystemUri};
use crate::platform;
use crate::process::{self, CommandSpec};

use super::Verdict;

/// `Get-AppxPackage` is slow on first use.
const APPX_TIMEOUT: Duration = Duration::from_secs(30);
/// Label code for the download-page fix (`common:fixes.labels.codex_app_download`).
pub const CODEX_APP_DOWNLOAD_LABEL: &str = "codex_app_download";

/// Installed app facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledCodexApp {
    pub path: String,
    pub version: Option<String>,
}

/// Runs the check.
pub async fn check(spec: &CodexAppSpec) -> Verdict {
    let platform = platform::platform();
    let found = match platform {
        Platform::Windows => windows_appx().await,
        Platform::Macos => macos_app(platform::home_dir().as_deref()),
        Platform::Linux | Platform::Unknown => {
            return Verdict::new(
                crate::models::CheckStatus::Skipped,
                "codex_app.not_applicable",
            )
        }
    };
    verdict_for(found.as_ref(), platform, spec)
}

/// Pure mapping to the verdict.
pub fn verdict_for(
    found: Option<&InstalledCodexApp>,
    platform: Platform,
    spec: &CodexAppSpec,
) -> Verdict {
    match found {
        Some(app) => Verdict::pass("codex_app.ok")
            .param("path", app.path.clone())
            .param_opt("version", app.version.clone())
            .detail(app.path.clone()),
        None => Verdict::warn("codex_app.missing").fixes(missing_fixes(platform, spec)),
    }
}

/// Fixes for a missing client: the wizard install (download + run), the Store page and the
/// region settings on Windows, the public download page when configured.
pub fn missing_fixes(platform: Platform, spec: &CodexAppSpec) -> Vec<FixAction> {
    let mut fixes = vec![FixAction::Install {
        tool: InstallTarget::CodexApp,
    }];
    if platform == Platform::Windows {
        if !spec.store_product_id.trim().is_empty() {
            fixes.push(FixAction::OpenSystemUri {
                uri: SystemUri::MsStoreCodexApp,
            });
        }
        fixes.push(FixAction::OpenSystemUri {
            uri: SystemUri::WindowsRegionSettings,
        });
    }
    let page = spec.download_page.trim();
    if !page.is_empty() {
        fixes.push(FixAction::OpenUrl {
            url: page.to_owned(),
            label_code: CODEX_APP_DOWNLOAD_LABEL.to_owned(),
        });
    }
    fixes.push(FixAction::Rerun);
    fixes
}

/// `Get-AppxPackage -Name OpenAI.Codex` (fixed query, no interpolation): `InstallLocation`
/// and `Version` of the newest package.
async fn windows_appx() -> Option<InstalledCodexApp> {
    let query = "$p = Get-AppxPackage -Name OpenAI.Codex | Sort-Object Version -Descending \
                 | Select-Object -First 1; if ($p) { Write-Output $p.InstallLocation; Write-Output $p.Version }";
    let spec = CommandSpec::new(
        "powershell.exe",
        [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            query,
        ],
    )
    .with_timeout(APPX_TIMEOUT);
    let out = process::run(&spec).await.ok()?;
    if !out.success() {
        log::info!("codex appx probe exited with {:?}", out.exit_code);
        return None;
    }
    parse_appx_output(&out.stdout)
}

/// Two lines: install location, version. Pure.
pub fn parse_appx_output(stdout: &str) -> Option<InstalledCodexApp> {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let path = lines.next()?.to_owned();
    if path.is_empty() {
        return None;
    }
    let version = lines.next().map(str::to_owned);
    Some(InstalledCodexApp { path, version })
}

/// App bundles to look for on macOS (newest naming first).
pub fn macos_app_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let names = ["ChatGPT.app", "Codex.app"];
    let mut list: Vec<PathBuf> = names
        .iter()
        .map(|n| PathBuf::from("/Applications").join(n))
        .collect();
    if let Some(home) = home {
        list.extend(names.iter().map(|n| home.join("Applications").join(n)));
    }
    list
}

fn macos_app(home: Option<&Path>) -> Option<InstalledCodexApp> {
    macos_app_candidates(home)
        .into_iter()
        .find(|p| p.join("Contents").join("Info.plist").is_file())
        .map(|p| {
            let plist =
                std::fs::read_to_string(p.join("Contents").join("Info.plist")).unwrap_or_default();
            InstalledCodexApp {
                path: p.to_string_lossy().into_owned(),
                version: super::cc_switch::plist_short_version(&plist),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CheckStatus;

    fn spec() -> CodexAppSpec {
        CodexAppSpec {
            store_product_id: "9PLM9XGG6VKS".into(),
            download_page: "https://chatgpt.com/download".into(),
            ..CodexAppSpec::default()
        }
    }

    #[test]
    fn found_app_passes_with_details() {
        let app = InstalledCodexApp {
            path: r"C:\Program Files\WindowsApps\OpenAI.Codex_26.8.0_x64__2p2nqsd0c76g0".into(),
            version: Some("26.8.0".into()),
        };
        let v = verdict_for(Some(&app), Platform::Windows, &spec());
        assert_eq!(v.status, CheckStatus::Pass);
        assert_eq!(v.code, "codex_app.ok");
        assert_eq!(v.params.get("version").map(String::as_str), Some("26.8.0"));
        assert_eq!(v.details, vec![app.path]);
    }

    #[test]
    fn missing_app_warns_with_platform_fixes() {
        let win = verdict_for(None, Platform::Windows, &spec());
        assert_eq!(win.status, CheckStatus::Warn);
        assert!(win.fixes.contains(&FixAction::Install {
            tool: InstallTarget::CodexApp
        }));
        assert!(win.fixes.contains(&FixAction::OpenSystemUri {
            uri: SystemUri::MsStoreCodexApp
        }));
        assert!(win.fixes.contains(&FixAction::OpenSystemUri {
            uri: SystemUri::WindowsRegionSettings
        }));
        let mac = verdict_for(None, Platform::Macos, &spec());
        assert!(!mac
            .fixes
            .iter()
            .any(|f| matches!(f, FixAction::OpenSystemUri { .. })));
        assert!(mac
            .fixes
            .iter()
            .any(|f| matches!(f, FixAction::OpenUrl { .. })));
        // No store id → no store button.
        let no_store = missing_fixes(Platform::Windows, &CodexAppSpec::default());
        assert!(!no_store.contains(&FixAction::OpenSystemUri {
            uri: SystemUri::MsStoreCodexApp
        }));
    }

    #[test]
    fn appx_output_parses() {
        let parsed = parse_appx_output(
            "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.8.0_x64__2p2nqsd0c76g0\r\n26.8.0\r\n",
        )
        .expect("parsed");
        assert!(parsed.path.ends_with("2p2nqsd0c76g0"));
        assert_eq!(parsed.version.as_deref(), Some("26.8.0"));
        assert!(parse_appx_output("\n").is_none());
    }

    #[test]
    fn macos_candidates_cover_both_names() {
        let list = macos_app_candidates(Some(Path::new("/Users/me")));
        assert!(list.contains(&PathBuf::from("/Applications/ChatGPT.app")));
        assert!(list.contains(&PathBuf::from("/Users/me/Applications/Codex.app")));
    }
}

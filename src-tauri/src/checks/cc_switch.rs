//! CC Switch check: is the desktop app installed, and does its data directory exist?
//!
//! - Windows: `HKCU`/`HKLM` `Software\Microsoft\Windows\CurrentVersion\Uninstall\*` (plus the
//!   `WOW6432Node` view) with a `DisplayName` containing "cc switch" / "cc-switch" →
//!   `DisplayVersion`, `InstallLocation`; also `%LOCALAPPDATA%\Programs\{cc-switch,CC Switch}\*.exe`.
//! - macOS: `/Applications/CC Switch.app`, `~/Applications/CC Switch.app` (and the
//!   `cc-switch.app` spelling); version from `Contents/Info.plist` `CFBundleShortVersionString`
//!   (tiny text scan, no plist dependency).
//! - Data dir: `expand_tilde(cc_switch.data_dir)` (read-only existence test — ADR-0003).
//!
//! Codes: `cc_switch.ok` (Pass) · `cc_switch.data_only` (Warn — data dir exists but the app
//! was not found) · `cc_switch.missing` (Fail — install + download page).

use std::path::{Path, PathBuf};

use crate::models::{CcSwitchSpec, FixAction, InstallTarget, Platform};
use crate::platform;

use super::Verdict;

/// Label code of the CC Switch download `OpenUrl` fix.
pub const CC_SWITCH_DOWNLOAD_LABEL: &str = "cc_switch_download";

/// An installed CC Switch application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledApp {
    pub version: Option<String>,
    /// Install location or bundle path, when known.
    pub location: Option<String>,
    /// Where the app was found (`registry`, `local_app_data`, `app_bundle`).
    pub source: &'static str,
}

/// Runs the CC Switch check.
pub fn check(spec: &CcSwitchSpec) -> Verdict {
    let app = find_installed_app();
    let data_dir = platform::expand_tilde(&spec.data_dir);
    let data_dir_present = data_dir.is_dir();
    evaluate(spec, app.as_ref(), &data_dir, data_dir_present)
}

/// Pure mapping of the detection results to a verdict.
pub fn evaluate(
    spec: &CcSwitchSpec,
    app: Option<&InstalledApp>,
    data_dir: &Path,
    data_dir_present: bool,
) -> Verdict {
    let data_dir_str = data_dir.to_string_lossy().into_owned();
    let verdict = match app {
        Some(app) => {
            let v = Verdict::pass("cc_switch.ok")
                .param_opt("version", app.version.clone())
                .param_opt("path", app.location.clone())
                .param("source", app.source);
            match &app.location {
                Some(location) => v.detail(location.clone()),
                None => v,
            }
        }
        None if data_dir_present => Verdict::warn("cc_switch.data_only")
            .fixes(install_fixes(spec))
            .fix(FixAction::Rerun),
        None => Verdict::fail("cc_switch.missing").fixes(install_fixes(spec)),
    };
    let verdict = verdict.param("dataDir", data_dir_str.clone());
    if data_dir_present {
        verdict.detail(data_dir_str)
    } else {
        verdict
    }
}

/// Install fix + download page (when configured).
pub fn install_fixes(spec: &CcSwitchSpec) -> Vec<FixAction> {
    let mut fixes = vec![FixAction::Install {
        tool: InstallTarget::CcSwitch,
    }];
    let page = if spec.intranet_mirror.trim().is_empty() {
        spec.download_page.trim()
    } else {
        spec.intranet_mirror.trim()
    };
    if !page.is_empty() {
        fixes.push(FixAction::OpenUrl {
            url: page.to_owned(),
            label_code: CC_SWITCH_DOWNLOAD_LABEL.to_owned(),
        });
    }
    fixes
}

/// `true` when an uninstall entry / app name denotes CC Switch (case-insensitive).
pub fn is_cc_switch_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["cc switch", "cc-switch", "ccswitch", "cc_switch"]
        .iter()
        .any(|needle| lower.contains(needle))
}

/// `CFBundleShortVersionString` from Info.plist text (`<key>…</key>` followed by
/// `<string>…</string>`), tolerant of whitespace/newlines between the two.
pub fn plist_short_version(plist: &str) -> Option<String> {
    let key_pos = plist.find("<key>CFBundleShortVersionString</key>")?;
    let rest = &plist[key_pos..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")? + start;
    let value = rest[start..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Locates the installed app on the current platform (`None` when not found).
pub fn find_installed_app() -> Option<InstalledApp> {
    match platform::platform() {
        Platform::Windows => windows_registry_app().or_else(|| {
            let local = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from)?;
            local_app_data_app(&local)
        }),
        Platform::Macos => macos_app_candidates(platform::home_dir().as_deref())
            .into_iter()
            .find_map(|bundle| app_bundle(&bundle)),
        Platform::Linux | Platform::Unknown => None,
    }
}

/// `%LOCALAPPDATA%\Programs\{cc-switch,CC Switch}` containing at least one `.exe`.
pub fn local_app_data_app(local_app_data: &Path) -> Option<InstalledApp> {
    ["cc-switch", "CC Switch"]
        .iter()
        .map(|name| local_app_data.join("Programs").join(name))
        .find(|dir| dir_has_exe(dir))
        .map(|dir| InstalledApp {
            version: None,
            location: Some(dir.to_string_lossy().into_owned()),
            source: "local_app_data",
        })
}

fn dir_has_exe(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        })
    })
}

/// macOS bundle paths to probe (pure).
pub fn macos_app_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let names = ["CC Switch.app", "cc-switch.app"];
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = home {
        roots.push(home.join("Applications"));
    }
    roots
        .iter()
        .flat_map(|root| names.iter().map(move |n| root.join(n)))
        .collect()
}

/// Reads an app bundle at `bundle` (must contain `Contents/Info.plist`).
pub fn app_bundle(bundle: &Path) -> Option<InstalledApp> {
    let plist = bundle.join("Contents").join("Info.plist");
    if !bundle.is_dir() || !plist.is_file() {
        return None;
    }
    let version = std::fs::read_to_string(&plist)
        .ok()
        .and_then(|text| plist_short_version(&text));
    Some(InstalledApp {
        version,
        location: Some(bundle.to_string_lossy().into_owned()),
        source: "app_bundle",
    })
}

/// Windows uninstall registry scan (read-only). `None` on other platforms.
fn windows_registry_app() -> Option<InstalledApp> {
    #[cfg(windows)]
    {
        windows_registry::find_cc_switch()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows_registry {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    use super::{is_cc_switch_name, InstalledApp};

    const UNINSTALL_KEYS: [&str; 2] = [
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];

    /// First uninstall entry whose `DisplayName` denotes CC Switch (HKCU first, then HKLM).
    pub(super) fn find_cc_switch() -> Option<InstalledApp> {
        [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
            .into_iter()
            .flat_map(|root| UNINSTALL_KEYS.iter().map(move |path| (root, *path)))
            .find_map(|(root, path)| scan_uninstall_key(root, path))
    }

    fn scan_uninstall_key(root: winreg::HKEY, path: &str) -> Option<InstalledApp> {
        let key = RegKey::predef(root)
            .open_subkey_with_flags(path, KEY_READ)
            .ok()?;
        key.enum_keys().filter_map(Result::ok).find_map(|name| {
            let sub = key.open_subkey_with_flags(&name, KEY_READ).ok()?;
            let display: String = sub.get_value("DisplayName").ok()?;
            if !is_cc_switch_name(&display) {
                return None;
            }
            let version: Option<String> = sub.get_value("DisplayVersion").ok();
            let location: Option<String> = sub.get_value("InstallLocation").ok();
            Some(InstalledApp {
                version: version
                    .map(|v| v.trim().to_owned())
                    .filter(|v| !v.is_empty()),
                location: location
                    .map(|l| l.trim().to_owned())
                    .filter(|l| !l.is_empty()),
                source: "registry",
            })
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Enumerating the uninstall keys must never panic, whatever is installed.
        #[test]
        fn uninstall_scan_does_not_panic() {
            let _ = find_cc_switch();
            for path in UNINSTALL_KEYS {
                let _ = scan_uninstall_key(HKEY_CURRENT_USER, path);
                let _ = scan_uninstall_key(HKEY_LOCAL_MACHINE, path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CheckStatus;

    fn spec() -> CcSwitchSpec {
        CcSwitchSpec {
            github_repo: "farion1231/cc-switch".into(),
            releases_api: "https://api.github.com/repos/farion1231/cc-switch/releases/latest"
                .into(),
            download_page: "https://github.com/farion1231/cc-switch/releases/latest".into(),
            intranet_mirror: String::new(),
            data_dir: "~/.cc-switch".into(),
        }
    }

    fn app() -> InstalledApp {
        InstalledApp {
            version: Some("3.2.1".into()),
            location: Some(r"C:\Users\me\AppData\Local\Programs\cc-switch".into()),
            source: "registry",
        }
    }

    #[test]
    fn evaluate_table() {
        let data_dir = Path::new("/Users/me/.cc-switch");
        let cases: &[(&str, Option<InstalledApp>, bool, CheckStatus, &str)] = &[
            (
                "app + data",
                Some(app()),
                true,
                CheckStatus::Pass,
                "cc_switch.ok",
            ),
            (
                "app only",
                Some(app()),
                false,
                CheckStatus::Pass,
                "cc_switch.ok",
            ),
            (
                "data only",
                None,
                true,
                CheckStatus::Warn,
                "cc_switch.data_only",
            ),
            (
                "nothing",
                None,
                false,
                CheckStatus::Fail,
                "cc_switch.missing",
            ),
        ];
        for (name, app, data, status, code) in cases {
            let v = evaluate(&spec(), app.as_ref(), data_dir, *data);
            assert_eq!(v.status, *status, "case {name}");
            assert_eq!(v.code, *code, "case {name}");
            assert_eq!(
                v.params.get("dataDir").map(String::as_str),
                Some("/Users/me/.cc-switch"),
                "case {name}"
            );
        }
    }

    #[test]
    fn ok_carries_version_and_missing_offers_install_and_download() {
        let ok = evaluate(&spec(), Some(&app()), Path::new("/x"), false);
        assert_eq!(ok.params.get("version").map(String::as_str), Some("3.2.1"));
        assert!(ok.fixes.is_empty());

        let missing = evaluate(&spec(), None, Path::new("/x"), false);
        assert_eq!(
            missing.fixes,
            vec![
                FixAction::Install {
                    tool: InstallTarget::CcSwitch
                },
                FixAction::OpenUrl {
                    url: "https://github.com/farion1231/cc-switch/releases/latest".into(),
                    label_code: "cc_switch_download".into(),
                },
            ]
        );

        let mut mirror = spec();
        mirror.intranet_mirror = "https://intranet.example/cc-switch".into();
        assert!(matches!(
            &install_fixes(&mirror)[1],
            FixAction::OpenUrl { url, .. } if url == "https://intranet.example/cc-switch"
        ));
        let mut none = spec();
        none.download_page.clear();
        assert_eq!(install_fixes(&none).len(), 1);
    }

    #[test]
    fn name_matcher_is_case_insensitive() {
        assert!(is_cc_switch_name("CC Switch"));
        assert!(is_cc_switch_name("cc-switch 3.0.0"));
        assert!(is_cc_switch_name("CCSwitch"));
        assert!(!is_cc_switch_name("Visual Studio Code"));
        assert!(!is_cc_switch_name(""));
    }

    #[test]
    fn plist_scan_extracts_short_version() {
        let plist = "<?xml version=\"1.0\"?>\n<plist><dict>\n<key>CFBundleName</key><string>CC Switch</string>\n<key>CFBundleShortVersionString</key>\n\t<string> 3.4.0 </string>\n<key>CFBundleVersion</key><string>340</string>\n</dict></plist>";
        assert_eq!(plist_short_version(plist).as_deref(), Some("3.4.0"));
        assert_eq!(plist_short_version("<plist></plist>"), None);
        assert_eq!(
            plist_short_version("<key>CFBundleShortVersionString</key><string></string>"),
            None
        );
    }

    #[test]
    fn app_bundle_reads_version_from_temp_bundle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bundle = dir.path().join("CC Switch.app");
        std::fs::create_dir_all(bundle.join("Contents")).expect("mkdir");
        std::fs::write(
            bundle.join("Contents").join("Info.plist"),
            "<key>CFBundleShortVersionString</key><string>1.2.3</string>",
        )
        .expect("write");
        let found = app_bundle(&bundle).expect("bundle");
        assert_eq!(found.version.as_deref(), Some("1.2.3"));
        assert_eq!(found.source, "app_bundle");
        assert!(app_bundle(&dir.path().join("Missing.app")).is_none());
    }

    #[test]
    fn local_app_data_probe_needs_an_exe() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(local_app_data_app(dir.path()).is_none());
        let programs = dir.path().join("Programs").join("cc-switch");
        std::fs::create_dir_all(&programs).expect("mkdir");
        std::fs::write(programs.join("readme.txt"), b"").expect("write");
        assert!(local_app_data_app(dir.path()).is_none());
        std::fs::write(programs.join("CC Switch.exe"), b"").expect("write");
        let found = local_app_data_app(dir.path()).expect("found");
        assert_eq!(found.source, "local_app_data");
        assert_eq!(
            found.location.as_deref(),
            Some(programs.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn macos_candidates_cover_system_and_user_applications() {
        let list = macos_app_candidates(Some(Path::new("/Users/me")));
        assert!(list.contains(&PathBuf::from("/Applications/CC Switch.app")));
        assert!(list.contains(&PathBuf::from("/Users/me/Applications/CC Switch.app")));
        assert_eq!(macos_app_candidates(None).len(), 2);
    }

    #[test]
    fn check_on_this_machine_yields_a_known_code() {
        let v = check(&spec());
        assert!(v.code.starts_with("cc_switch."), "{v:?}");
        assert!(v.params.contains_key("dataDir"));
    }
}

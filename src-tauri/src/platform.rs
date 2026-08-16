//! OS / filesystem / PATH introspection. Pure read-only helpers — nothing here writes to the
//! user's machine or modifies environment variables (PRD #17).
//!
//! Platform-dependent behaviour is isolated in small functions; the pure decision logic
//! (`*_for(platform, …)`) is separated from the current-platform wrappers so it can be
//! unit-tested on any host.

use std::path::{Path, PathBuf};

use crate::models::{OsInfo, Platform};
use crate::process::{self, CommandSpec};

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

// ---------------------------------------------------------------------------
// OS description
// ---------------------------------------------------------------------------

/// Best-effort OS description (version, build number on Windows, arch, default shell, admin?).
///
/// `is_admin` limitations: Windows answers `Some(true)` only when a directory readable solely
/// by administrators can be listed (elevated process); `Some(false)` when access is denied;
/// `None` when the probe itself fails. macOS/Linux use the `USER == root` heuristic — a
/// member of the `admin` group therefore reports `Some(false)` (which is the answer we care
/// about: nothing is elevated).
pub fn os_info() -> OsInfo {
    let info = os_info::get();
    OsInfo {
        platform: platform(),
        version: info.version().to_string(),
        build: windows_build_number(),
        arch: std::env::consts::ARCH.to_owned(),
        shell: default_shell(),
        home_dir: home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        is_admin: is_admin(),
    }
}

/// Basename of the default shell: `ComSpec` on Windows (`cmd.exe`), `$SHELL` elsewhere.
fn default_shell() -> String {
    let var = if cfg!(windows) { "ComSpec" } else { "SHELL" };
    std::env::var(var)
        .ok()
        .and_then(|s| basename(&s))
        .unwrap_or_default()
}

/// Last path component of `path` as a string (`None` for empty input).
pub fn basename(path: &str) -> Option<String> {
    Path::new(path.trim())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
}

/// Windows build number from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`
/// (`CurrentBuildNumber`); `None` on other platforms or when unreadable.
pub fn windows_build_number() -> Option<u64> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
        use winreg::RegKey;
        let key = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
            .ok()?;
        let raw: String = key.get_value("CurrentBuildNumber").ok()?;
        raw.trim().parse().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Maps a Windows LCID to one of our supported language tags.
///
/// Only the primary language (low 10 bits) is looked at, so every Chinese sublanguage
/// (`zh-CN` 2052, `zh-TW` 1028, `zh-HK` 3076, …) and every English one collapses correctly.
/// Anything else yields `None` so the caller falls back to the OS locale.
fn lang_tag_for_lcid(lcid: u32) -> Option<&'static str> {
    match lcid & 0x3ff {
        0x04 => Some("zh-CN"),
        0x09 => Some("en"),
        _ => None,
    }
}

/// The language the user picked in the NSIS installer's language selector.
///
/// The bundled installer persists the choice as a decimal LCID under
/// `HKCU\Software\<manufacturer>\<product>\Installer Language` (MUI's `MUI_LANGDLL_SAVELANGUAGE`,
/// reached via the `MUI_PAGE_INSTFILES` page). Reading it lets the app open in the same language
/// the user just chose, instead of guessing from the OS UI language. `None` off Windows, when the
/// app was not installed by the installer (dev runs, portable copies), or when the value is absent
/// or unparsable.
pub fn installer_language(manufacturer: &str, product: &str) -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;
        if manufacturer.is_empty() || product.is_empty() {
            return None;
        }
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(format!(r"Software\{manufacturer}\{product}"), KEY_READ)
            .ok()?;
        let raw: String = key.get_value("Installer Language").ok()?;
        let lcid: u32 = raw.trim().parse().ok()?;
        lang_tag_for_lcid(lcid).map(ToOwned::to_owned)
    }
    #[cfg(not(windows))]
    {
        let _ = (manufacturer, product);
        None
    }
}

/// See [`os_info`] for the semantics and limitations.
pub fn is_admin() -> Option<bool> {
    #[cfg(windows)]
    {
        // Readable only with administrative rights; a plain user gets "access denied".
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let probe = Path::new(&system_root).join(r"System32\config\systemprofile");
        match std::fs::read_dir(&probe) {
            Ok(_) => Some(true),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Some(false),
            Err(_) => None,
        }
    }
    #[cfg(not(windows))]
    {
        std::env::var("USER").ok().map(|u| u == "root")
    }
}

// ---------------------------------------------------------------------------
// Executables and npm layout
// ---------------------------------------------------------------------------

/// Finds an executable by name on the current `PATH` (`which`), including `.cmd`/`.exe`
/// shims on Windows.
pub fn find_on_path(binary: &str) -> Option<PathBuf> {
    which::which_global(binary).ok()
}

/// File names to try for `binary` inside a directory that is *not* on `PATH`.
pub fn candidate_names_for(platform: Platform, binary: &str) -> Vec<String> {
    match platform {
        Platform::Windows => ["cmd", "exe", "bat"]
            .iter()
            .map(|ext| format!("{binary}.{ext}"))
            .chain(std::iter::once(binary.to_owned()))
            .collect(),
        _ => vec![binary.to_owned()],
    }
}

/// Locates `binary`: first on `PATH` (`(path, true)`), then inside `extra_dirs` such as the
/// npm global bin directory (`(path, false)` — installed but not reachable from a terminal,
/// guide fault E). `None` when not found anywhere.
pub fn find_binary(binary: &str, extra_dirs: &[PathBuf]) -> Option<(PathBuf, bool)> {
    if let Some(p) = find_on_path(binary) {
        return Some((p, true));
    }
    let names = candidate_names_for(platform(), binary);
    extra_dirs
        .iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|candidate| candidate.is_file())
        .map(|p| (p, false))
}

/// npm global prefix: `npm prefix -g` (10 s timeout) with [`npm_global_prefix_guess`] as
/// fallback when npm is missing or fails.
pub async fn npm_global_prefix() -> Option<PathBuf> {
    let spec =
        CommandSpec::new("npm", ["prefix", "-g"]).with_timeout(std::time::Duration::from_secs(10));
    match process::run(&spec).await {
        Ok(out) if out.success() => {
            let line = out.stdout.lines().map(str::trim).find(|l| !l.is_empty());
            match line {
                Some(p) => Some(PathBuf::from(p)),
                None => npm_global_prefix_guess(),
            }
        }
        _ => npm_global_prefix_guess(),
    }
}

/// Synchronous best-effort guess of the npm global prefix without running npm:
/// `NPM_CONFIG_PREFIX` / `npm_config_prefix` → `prefix=` in `~/.npmrc` → platform default
/// (Windows `%APPDATA%\npm`; Unix `~/.npm-global` if present, `/opt/homebrew` when its npm
/// exists, else `/usr/local`).
pub fn npm_global_prefix_guess() -> Option<PathBuf> {
    let env_prefix = ["NPM_CONFIG_PREFIX", "npm_config_prefix"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(|v| v.trim().to_owned())
        .find(|v| !v.is_empty());
    if let Some(p) = env_prefix {
        return Some(expand_tilde(&p));
    }
    if let Some(p) = npmrc_prefix() {
        return Some(p);
    }
    default_npm_prefix_for(platform(), home_dir().as_deref())
}

/// `prefix=` from `~/.npmrc` (read-only), if any.
fn npmrc_prefix() -> Option<PathBuf> {
    let content = std::fs::read_to_string(home_dir()?.join(".npmrc")).ok()?;
    parse_npmrc_prefix(&content).map(|p| expand_tilde(&p))
}

/// Extracts the `prefix` value from npmrc-style content (`key=value`, `;`/`#` comments).
pub fn parse_npmrc_prefix(content: &str) -> Option<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with(';') && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == "prefix")
        .map(|(_, v)| v.trim().trim_matches('"').to_owned())
        .filter(|v| !v.is_empty())
}

/// Platform default of the npm global prefix (pure; `home` injected for tests).
pub fn default_npm_prefix_for(platform: Platform, home: Option<&Path>) -> Option<PathBuf> {
    match platform {
        Platform::Windows => std::env::var("APPDATA")
            .ok()
            .map(|a| PathBuf::from(a).join("npm"))
            .or_else(|| home.map(|h| h.join("AppData").join("Roaming").join("npm"))),
        Platform::Macos | Platform::Linux | Platform::Unknown => {
            if let Some(user_prefix) = home.map(|h| h.join(".npm-global")) {
                if user_prefix.is_dir() {
                    return Some(user_prefix);
                }
            }
            let brew = Path::new("/opt/homebrew");
            if brew.join("bin").join("npm").exists() {
                return Some(brew.to_path_buf());
            }
            Some(PathBuf::from("/usr/local"))
        }
    }
}

/// Directory that holds npm's global executables for a given prefix (pure).
pub fn npm_global_bin_dir_for(platform: Platform, prefix: &Path) -> PathBuf {
    match platform {
        Platform::Windows => prefix.to_path_buf(),
        _ => prefix.join("bin"),
    }
}

/// [`npm_global_bin_dir_for`] on the current platform.
pub fn npm_global_bin_dir(prefix: &Path) -> PathBuf {
    npm_global_bin_dir_for(platform(), prefix)
}

/// `true` when Homebrew is usable (macOS only: `brew` on `PATH` or in a standard location).
pub fn homebrew_available() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    find_on_path("brew").is_some()
        || Path::new("/opt/homebrew/bin/brew").exists()
        || Path::new("/usr/local/bin/brew").exists()
}

/// Shell start-up files that may define `PATH`/env vars, in the order a login shell reads them.
/// Pure: returns candidates under `home` (plus system-wide files) without touching the disk.
pub fn shell_rc_candidates(home: &Path) -> Vec<PathBuf> {
    let user = [
        ".zshenv",
        ".zprofile",
        ".zshrc",
        ".zlogin",
        ".bash_profile",
        ".bash_login",
        ".bashrc",
        ".profile",
        ".config/fish/config.fish",
    ];
    let system = [
        "/etc/zshenv",
        "/etc/zprofile",
        "/etc/zshrc",
        "/etc/profile",
        "/etc/paths",
    ];
    user.iter()
        .map(|f| home.join(f))
        .chain(system.iter().map(PathBuf::from))
        .collect()
}

/// PowerShell profile files (`$PROFILE` variants) that may set `$env:NAME` for every new
/// PowerShell window: `Documents\PowerShell\*profile.ps1` (PowerShell 7) and
/// `Documents\WindowsPowerShell\*profile.ps1` (Windows PowerShell 5), under the user profile
/// and — when Documents is redirected — under `onedrive`. Pure.
pub fn powershell_profile_candidates(home: &Path, onedrive: Option<&Path>) -> Vec<PathBuf> {
    let roots = std::iter::once(home).chain(onedrive);
    roots
        .flat_map(|root| {
            ["PowerShell", "WindowsPowerShell"]
                .into_iter()
                .flat_map(move |edition| {
                    ["profile.ps1", "Microsoft.PowerShell_profile.ps1"]
                        .into_iter()
                        .map(move |file| root.join("Documents").join(edition).join(file))
                })
        })
        .collect()
}

/// Existing shell start-up files for the current user: shell rc files on macOS/Linux,
/// PowerShell profiles on Windows.
pub fn shell_rc_files() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let candidates = if cfg!(windows) {
        let onedrive = std::env::var_os("OneDrive").map(PathBuf::from);
        powershell_profile_candidates(&home, onedrive.as_deref())
    } else {
        shell_rc_candidates(&home)
    };
    candidates.into_iter().filter(|p| p.is_file()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcid_maps_every_chinese_and_english_sublanguage() {
        // zh-CN, zh-TW, zh-HK, zh-SG, zh-MO — the installer only offers SimpChinese (2052),
        // but a pre-existing registry value may carry any of them.
        for lcid in [2052, 1028, 3076, 4100, 5124] {
            assert_eq!(lang_tag_for_lcid(lcid), Some("zh-CN"), "lcid {lcid}");
        }
        // en-US, en-GB, en-AU, en-CA
        for lcid in [1033, 2057, 3081, 4105] {
            assert_eq!(lang_tag_for_lcid(lcid), Some("en"), "lcid {lcid}");
        }
        // Unsupported languages fall through to the OS locale rather than guessing.
        assert_eq!(lang_tag_for_lcid(1041), None, "ja-JP");
        assert_eq!(lang_tag_for_lcid(1031), None, "de-DE");
    }

    #[test]
    fn installer_language_is_none_without_manufacturer_or_product() {
        assert_eq!(installer_language("", "SeedRouter Onboarding"), None);
        assert_eq!(installer_language("company", ""), None);
    }

    /// Round-trips the real registry read against a value written exactly the way MUI's
    /// `MUI_LANGDLL_SAVELANGUAGE` writes it (decimal LCID, REG_SZ), under a test-scoped key.
    #[cfg(windows)]
    #[test]
    fn installer_language_reads_a_saved_selection() {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};
        use winreg::RegKey;

        let (vendor, product) = ("seedrouter-onboarding-test", "lang-roundtrip");
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = format!(r"Software\{vendor}\{product}");
        let (key, _) = hkcu.create_subkey(&path).expect("create test key");

        key.set_value("Installer Language", &"2052".to_owned())
            .expect("write SimpChinese");
        let zh = installer_language(vendor, product);

        key.set_value("Installer Language", &"1033".to_owned())
            .expect("write English");
        let en = installer_language(vendor, product);

        key.set_value("Installer Language", &"not-a-number".to_owned())
            .expect("write garbage");
        let broken = installer_language(vendor, product);

        drop(key);
        let root = hkcu
            .open_subkey_with_flags(r"Software", KEY_ALL_ACCESS)
            .expect("open Software");
        let _ = root.delete_subkey_all(vendor);

        assert_eq!(zh.as_deref(), Some("zh-CN"));
        assert_eq!(en.as_deref(), Some("en"));
        assert_eq!(
            broken, None,
            "unparsable value must fall back to the OS locale"
        );
    }

    #[test]
    fn installer_language_is_none_when_not_installed() {
        // Nothing ever writes this key, on any platform.
        assert_eq!(
            installer_language("no-such-vendor-42", "no-such-product-42"),
            None
        );
    }

    #[test]
    fn powershell_profile_candidates_cover_both_editions_and_onedrive() {
        let home = Path::new(r"C:\Users\alice");
        let list = powershell_profile_candidates(home, None);
        assert_eq!(list.len(), 4);
        assert!(list.contains(
            &home
                .join("Documents")
                .join("PowerShell")
                .join("profile.ps1")
        ));
        assert!(list.contains(
            &home
                .join("Documents")
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        ));
        let onedrive = Path::new(r"C:\Users\alice\OneDrive");
        let with_onedrive = powershell_profile_candidates(home, Some(onedrive));
        assert_eq!(with_onedrive.len(), 8);
        assert!(with_onedrive.contains(
            &onedrive
                .join("Documents")
                .join("PowerShell")
                .join("profile.ps1")
        ));
    }

    #[test]
    fn expand_tilde_uses_home() {
        let home = home_dir().expect("home dir");
        assert_eq!(expand_tilde("~/x/y"), home.join("x").join("y"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_tilde("relative"), PathBuf::from("relative"));
    }

    #[test]
    fn npm_global_bin_dir_mapping() {
        let prefix = Path::new("/usr/local");
        assert_eq!(
            npm_global_bin_dir_for(Platform::Macos, prefix),
            PathBuf::from("/usr/local/bin")
        );
        assert_eq!(
            npm_global_bin_dir_for(Platform::Linux, prefix),
            PathBuf::from("/usr/local/bin")
        );
        let win = Path::new(r"C:\Users\me\AppData\Roaming\npm");
        assert_eq!(npm_global_bin_dir_for(Platform::Windows, win), win);
    }

    #[test]
    fn candidate_names_per_platform() {
        assert_eq!(
            candidate_names_for(Platform::Windows, "codex"),
            vec!["codex.cmd", "codex.exe", "codex.bat", "codex"]
        );
        assert_eq!(candidate_names_for(Platform::Macos, "codex"), vec!["codex"]);
    }

    #[test]
    fn find_binary_falls_back_to_extra_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let name = "seedrouter-onboarding-fake-binary";
        assert!(find_binary(name, &[]).is_none());
        let file_name = candidate_names_for(platform(), name)
            .into_iter()
            .next()
            .expect("candidate");
        std::fs::write(dir.path().join(&file_name), b"").expect("write");
        let (path, on_path) = find_binary(name, &[dir.path().to_path_buf()]).expect("found");
        assert!(!on_path);
        assert_eq!(path, dir.path().join(file_name));
    }

    #[test]
    fn parse_npmrc_prefix_reads_value() {
        let content = "; comment\nregistry=https://r/\n# other\nprefix = \"~/.npm-global\" \n";
        assert_eq!(
            parse_npmrc_prefix(content).as_deref(),
            Some("~/.npm-global")
        );
        assert_eq!(parse_npmrc_prefix("prefix="), None);
        assert_eq!(parse_npmrc_prefix("registry=x"), None);
        assert_eq!(parse_npmrc_prefix("#prefix=/x"), None);
    }

    #[test]
    fn default_npm_prefix_unix_prefers_user_dir() {
        let home = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(home.path().join(".npm-global")).expect("mkdir");
        assert_eq!(
            default_npm_prefix_for(Platform::Macos, Some(home.path())),
            Some(home.path().join(".npm-global"))
        );
        let empty = tempfile::tempdir().expect("tempdir");
        let fallback = default_npm_prefix_for(Platform::Linux, Some(empty.path())).expect("some");
        assert!(
            fallback == Path::new("/usr/local") || fallback == Path::new("/opt/homebrew"),
            "{fallback:?}"
        );
    }

    #[test]
    fn default_npm_prefix_windows_uses_appdata_layout() {
        let p = default_npm_prefix_for(Platform::Windows, Some(Path::new(r"C:\Users\me")))
            .expect("some");
        assert!(p.ends_with("npm"), "{p:?}");
    }

    #[test]
    fn shell_rc_candidates_are_under_home_or_etc() {
        let home = Path::new("/Users/me");
        let list = shell_rc_candidates(home);
        assert!(list.contains(&home.join(".zshrc")));
        assert!(list.contains(&home.join(".bash_profile")));
        assert!(list
            .iter()
            .all(|p| p.starts_with(home) || p.starts_with("/etc")));
        if cfg!(windows) {
            assert!(shell_rc_files().is_empty());
        }
    }

    #[test]
    fn basename_extracts_last_component() {
        // `basename` only ever sees the current OS's own shell path (`ComSpec` / `SHELL`), so
        // the backslash form is meaningful on Windows only (`\` is a plain char on Unix).
        if cfg!(windows) {
            assert_eq!(
                basename(r"C:\Windows\system32\cmd.exe").as_deref(),
                Some("cmd.exe")
            );
        }
        assert_eq!(basename("/bin/zsh").as_deref(), Some("zsh"));
        assert_eq!(basename(""), None);
    }

    #[test]
    fn os_info_is_populated() {
        let info = os_info();
        assert_eq!(info.platform, platform());
        assert!(!info.version.is_empty());
        assert!(!info.arch.is_empty());
        assert!(!info.home_dir.is_empty());
        if cfg!(windows) {
            assert!(info.build.is_some_and(|b| b > 10_000), "{info:?}");
            assert_eq!(info.shell.to_ascii_lowercase(), "cmd.exe");
        }
    }

    #[test]
    fn homebrew_is_never_reported_off_macos() {
        if !cfg!(target_os = "macos") {
            assert!(!homebrew_available());
        }
    }

    #[tokio::test]
    async fn npm_global_prefix_returns_something_when_npm_exists() {
        if find_on_path("npm").is_none() {
            return;
        }
        let prefix = npm_global_prefix().await.expect("prefix");
        assert!(prefix.is_absolute(), "{prefix:?}");
    }
}

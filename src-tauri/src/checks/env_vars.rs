//! Environment-variable conflict check (guide fault D). The check only *reports* (masked
//! values); removal is a separate, previewed and confirmed one-click action
//! (`remediate::plan_env_cleanup` / `apply_env_cleanup`, ADR-0008) or manual instructions.
//!
//! For every name in `env_vars_to_inspect` the finding lists where the variable is defined:
//! - `Process`         — set in this process (`std::env::var`), i.e. inherited at launch;
//! - `UserRegistry`    — Windows `HKCU\Environment`;
//! - `MachineRegistry` — Windows `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment`;
//! - `ShellRc`         — shell start-up files with `file:line`: macOS/Linux rc files
//!                       (`export NAME=` / `NAME=` / `setenv NAME` / fish `set -x NAME`),
//!                       Windows PowerShell profiles (`$env:NAME = …`,
//!                       `[Environment]::SetEnvironmentVariable("NAME", …)`);
//! - `Launchctl`       — macOS `launchctl getenv NAME` (3 s timeout).
//!
//! Status: any `*_API_KEY` / `*_BASE_URL` / `*_AUTH_TOKEN` / `*_API_BASE` present anywhere →
//! Warn `env_vars.conflicts` (params `names`; details `NAME ← source`; `Instructions` fix
//! `checks:env_vars.instructions.<windows|macos>`). Proxy variables (`HTTP(S)_PROXY`,
//! `ALL_PROXY`, `NO_PROXY`) are informational only. Otherwise Pass `env_vars.clean`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use futures_util::future::join_all;

use crate::models::{
    AppConfig, EnvVarFinding, EnvVarSource, EnvVarSourceKind, FixAction, Params, Platform,
};
use crate::platform;
use crate::process::{self, CommandSpec};
use crate::redact::mask_value;

use super::Verdict;

/// Timeout for `launchctl getenv`.
const LAUNCHCTL_TIMEOUT: Duration = Duration::from_secs(3);

/// Location label of the process source.
pub const PROCESS_LOCATION: &str = "process";
/// Location label of the user registry source (Windows).
pub const USER_REGISTRY_LOCATION: &str = r"HKCU\Environment";
/// Location label of the machine registry source (Windows).
pub const MACHINE_REGISTRY_LOCATION: &str =
    r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
/// Location label of the launchctl source (macOS).
pub const LAUNCHCTL_LOCATION: &str = "launchctl";

/// Suffixes that make a variable a *conflict* (it overrides the CC Switch configuration).
const CONFLICT_SUFFIXES: [&str; 4] = ["_API_KEY", "_BASE_URL", "_AUTH_TOKEN", "_API_BASE"];

/// Runs the env-var check: verdict + the findings for the snapshot.
pub async fn check(cfg: &AppConfig) -> (Verdict, Vec<EnvVarFinding>) {
    let findings = inspect_env_vars(cfg).await;
    let verdict = evaluate(&findings, platform::platform());
    (verdict, findings)
}

/// Inspects `cfg.env_vars_to_inspect` (see module docs).
pub async fn inspect_env_vars(cfg: &AppConfig) -> Vec<EnvVarFinding> {
    inspect_env_var_names(&cfg.env_vars_to_inspect).await
}

/// Inspects the given variable names (see module docs). Values are never returned raw.
pub async fn inspect_env_var_names(names: &[String]) -> Vec<EnvVarFinding> {
    let (user_reg, machine_reg) = registry_env_maps();
    let rc_hits = scan_rc_files(names);
    let launchctl = launchctl_values(names).await;
    names
        .iter()
        .map(|name| {
            let mut sources = Vec::new();
            let mut masked: Option<String> = None;
            let mut note = |kind: EnvVarSourceKind, location: String, value: Option<&str>| {
                sources.push(EnvVarSource { kind, location });
                if masked.is_none() {
                    masked = value.map(mask_value);
                }
            };
            let process_value = std::env::var(name).ok();
            if let Some(v) = &process_value {
                note(
                    EnvVarSourceKind::Process,
                    PROCESS_LOCATION.to_owned(),
                    Some(v),
                );
            }
            if let Some(v) = lookup(&user_reg, name) {
                note(
                    EnvVarSourceKind::UserRegistry,
                    USER_REGISTRY_LOCATION.to_owned(),
                    Some(v),
                );
            }
            if let Some(v) = lookup(&machine_reg, name) {
                note(
                    EnvVarSourceKind::MachineRegistry,
                    MACHINE_REGISTRY_LOCATION.to_owned(),
                    Some(v),
                );
            }
            for location in rc_hits.get(name).into_iter().flatten() {
                note(EnvVarSourceKind::ShellRc, location.clone(), None);
            }
            if let Some(v) = launchctl.get(name) {
                note(
                    EnvVarSourceKind::Launchctl,
                    LAUNCHCTL_LOCATION.to_owned(),
                    Some(v),
                );
            }
            EnvVarFinding {
                name: name.clone(),
                present_in_session: process_value.is_some(),
                value_masked: masked,
                sources,
            }
        })
        .collect()
}

/// Case-insensitive lookup (Windows registry value names are case-insensitive).
fn lookup<'a>(map: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    map.get(name).map(String::as_str).or_else(|| {
        map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    })
}

/// Pure verdict from findings (see module docs).
pub fn evaluate(findings: &[EnvVarFinding], platform: Platform) -> Verdict {
    let present: Vec<&EnvVarFinding> = findings.iter().filter(|f| is_present(f)).collect();
    let conflicts: Vec<&EnvVarFinding> = present
        .iter()
        .copied()
        .filter(|f| is_conflict_name(&f.name))
        .collect();
    let mut verdict = if conflicts.is_empty() {
        Verdict::pass("env_vars.clean")
    } else {
        let names = conflicts
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let mut params = Params::new();
        params.insert("names".to_owned(), names.clone());
        Verdict::warn("env_vars.conflicts")
            .param("names", names)
            .fix(FixAction::CleanEnvVars {
                names: conflicts.iter().map(|f| f.name.clone()).collect(),
            })
            .fix(FixAction::Instructions {
                code: instructions_code(platform).to_owned(),
                params,
            })
            .fix(FixAction::Rerun)
    };
    for finding in present {
        for source in &finding.sources {
            verdict = verdict.detail(format!("{} ← {}", finding.name, source.location));
        }
    }
    verdict
}

/// Fully-qualified instructions key for removing env vars on `platform`.
pub fn instructions_code(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "checks:env_vars.instructions.windows",
        Platform::Macos | Platform::Linux | Platform::Unknown => {
            "checks:env_vars.instructions.macos"
        }
    }
}

/// `true` when the variable is defined anywhere (process or a persistent source).
pub fn is_present(finding: &EnvVarFinding) -> bool {
    finding.present_in_session || !finding.sources.is_empty()
}

/// `true` for `*_API_KEY` / `*_BASE_URL` / `*_AUTH_TOKEN` / `*_API_BASE` (case-insensitive).
pub fn is_conflict_name(name: &str) -> bool {
    let upper = name.trim().to_ascii_uppercase();
    CONFLICT_SUFFIXES.iter().any(|s| upper.ends_with(s))
}

// ---------------------------------------------------------------------------
// Shell rc files (macOS / Linux)
// ---------------------------------------------------------------------------

/// Scans one file's content for definitions of `names`; returns `(name, line)` pairs with
/// 1-based line numbers, in file order. Recognises `export NAME=…`, `NAME=…` (also several
/// assignments on one line), `setenv NAME …`, fish `set -x|-gx|-Ux NAME …`, and the
/// PowerShell forms `$env:NAME = …` / `$env:NAME += …` and
/// `[Environment]::SetEnvironmentVariable("NAME", …)`. Comment lines are ignored; names are
/// matched case-insensitively (PowerShell / Windows treat them so).
pub fn scan_rc_content(content: &str, names: &[String]) -> Vec<(String, usize)> {
    let mut hits = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        for found in assignments_in_line(line) {
            if let Some(name) = names.iter().find(|n| n.eq_ignore_ascii_case(found)) {
                hits.push((name.clone(), idx + 1));
            }
        }
    }
    hits
}

/// Variable names assigned/exported on a single line (shell and PowerShell syntax).
fn assignments_in_line(line: &str) -> Vec<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return Vec::new();
    }
    let mut names = shell_assignments_in_line(trimmed);
    names.extend(powershell_assignments_in_line(trimmed));
    names
}

/// `$env:NAME =` / `$env:NAME +=` and `SetEnvironmentVariable("NAME"` occurrences.
fn powershell_assignments_in_line(line: &str) -> Vec<&str> {
    let lower = line.to_ascii_lowercase();
    let mut names = Vec::new();
    for (start, _) in lower.match_indices("$env:") {
        let name_start = start + "$env:".len();
        let name_end = line[name_start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(line.len(), |i| name_start + i);
        let name = &line[name_start..name_end];
        let rest = line[name_end..].trim_start();
        let assigns = rest.starts_with("+=") || (rest.starts_with('=') && !rest.starts_with("=="));
        if is_var_name(name) && assigns {
            names.push(name);
        }
    }
    for (start, _) in lower.match_indices("setenvironmentvariable(") {
        let after = &line[start + "setenvironmentvariable(".len()..];
        let after = after.trim_start();
        let Some(quote) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            continue;
        };
        let inner = &after[quote.len_utf8()..];
        if let Some(end) = inner.find(quote) {
            let name = &inner[..end];
            if is_var_name(name) {
                names.push(name);
            }
        }
    }
    names
}

/// Variable names assigned/exported on a single POSIX/fish shell line (already trimmed).
fn shell_assignments_in_line(trimmed: &str) -> Vec<&str> {
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    match tokens.as_slice() {
        ["setenv", name, ..] if is_var_name(name) => vec![name],
        ["set", flags, name, ..]
            if flags.starts_with('-') && flags.contains('x') && is_var_name(name) =>
        {
            vec![name]
        }
        _ => {
            let rest = if tokens.first() == Some(&"export") {
                &tokens[1..]
            } else {
                &tokens[..]
            };
            rest.iter().map_while(|t| assignment_name(t)).collect()
        }
    }
}

/// `NAME` from a `NAME=value` token (`None` when the token is not an assignment).
fn assignment_name(token: &str) -> Option<&str> {
    let (name, _) = token.split_once('=')?;
    is_var_name(name).then_some(name)
}

fn is_var_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Scans a file for `names`; unreadable files yield nothing.
pub fn scan_rc_file(path: &Path, names: &[String]) -> Vec<(String, usize)> {
    std::fs::read_to_string(path)
        .map(|content| scan_rc_content(&content, names))
        .unwrap_or_default()
}

/// `~/.zshrc:12`-style location: `home` prefix collapsed to `~`.
pub fn rc_location(path: &Path, home: Option<&Path>, line: usize) -> String {
    let display = match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rel) => format!("~/{}", rel.to_string_lossy().replace('\\', "/")),
        None => path.to_string_lossy().into_owned(),
    };
    format!("{display}:{line}")
}

/// Scans the current user's shell start-up files (`platform::shell_rc_files`: rc files on
/// macOS/Linux, PowerShell profiles on Windows): name → locations.
fn scan_rc_files(names: &[String]) -> BTreeMap<String, Vec<String>> {
    let home = platform::home_dir();
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in platform::shell_rc_files() {
        for (name, line) in scan_rc_file(&file, names) {
            map.entry(name)
                .or_default()
                .push(rc_location(&file, home.as_deref(), line));
        }
    }
    map
}

// ---------------------------------------------------------------------------
// launchctl (macOS)
// ---------------------------------------------------------------------------

/// `launchctl getenv NAME` for every name (macOS only; empty elsewhere): name → raw value.
/// Values are only ever masked before leaving this module.
async fn launchctl_values(names: &[String]) -> BTreeMap<String, String> {
    if platform::platform() != Platform::Macos {
        return BTreeMap::new();
    }
    let runs = names.iter().map(|name| async move {
        let spec = CommandSpec::new("launchctl", ["getenv", name.as_str()])
            .with_timeout(LAUNCHCTL_TIMEOUT);
        match process::run(&spec).await {
            Ok(out) if out.success() => {
                let value = out.stdout.trim();
                (!value.is_empty()).then(|| (name.clone(), value.to_owned()))
            }
            _ => None,
        }
    });
    join_all(runs).await.into_iter().flatten().collect()
}

// ---------------------------------------------------------------------------
// Windows registry (read-only)
// ---------------------------------------------------------------------------

/// `(HKCU\Environment, HKLM …\Session Manager\Environment)` string values; empty maps on
/// other platforms or when a key cannot be read.
pub fn registry_env_maps() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    #[cfg(windows)]
    {
        (
            windows_registry::read_env_key(
                winreg::enums::HKEY_CURRENT_USER,
                windows_registry::USER_KEY,
            ),
            windows_registry::read_env_key(
                winreg::enums::HKEY_LOCAL_MACHINE,
                windows_registry::MACHINE_KEY,
            ),
        )
    }
    #[cfg(not(windows))]
    {
        (BTreeMap::new(), BTreeMap::new())
    }
}

#[cfg(windows)]
mod windows_registry {
    use std::collections::BTreeMap;

    use winreg::enums::KEY_READ;
    use winreg::types::FromRegValue;
    use winreg::RegKey;

    pub(super) const MACHINE_KEY: &str =
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
    pub(super) const USER_KEY: &str = r"Environment";

    /// All string values of `root\path` (`REG_SZ` / `REG_EXPAND_SZ`); empty when unreadable.
    pub(super) fn read_env_key(root: winreg::HKEY, path: &str) -> BTreeMap<String, String> {
        let Ok(key) = RegKey::predef(root).open_subkey_with_flags(path, KEY_READ) else {
            return BTreeMap::new();
        };
        key.enum_values()
            .filter_map(Result::ok)
            .filter_map(|(name, value)| String::from_reg_value(&value).ok().map(|v| (name, v)))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

        #[test]
        fn reading_environment_keys_does_not_panic() {
            let _ = read_env_key(HKEY_CURRENT_USER, USER_KEY);
            let machine = read_env_key(HKEY_LOCAL_MACHINE, MACHINE_KEY);
            // Every Windows machine defines a machine-level Path.
            assert!(machine.keys().any(|k| k.eq_ignore_ascii_case("Path")));
            let (user, machine2) = super::super::registry_env_maps();
            assert_eq!(machine2.len(), machine.len());
            let _ = user;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        [
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "HTTPS_PROXY",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    fn finding(
        name: &str,
        in_session: bool,
        sources: &[(EnvVarSourceKind, &str)],
    ) -> EnvVarFinding {
        EnvVarFinding {
            name: name.into(),
            present_in_session: in_session,
            value_masked: in_session.then(|| "sk-****abcd".to_owned()),
            sources: sources
                .iter()
                .map(|(kind, loc)| EnvVarSource {
                    kind: *kind,
                    location: (*loc).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn conflict_name_table() {
        let cases = [
            ("OPENAI_API_KEY", true),
            ("ANTHROPIC_BASE_URL", true),
            ("ANTHROPIC_AUTH_TOKEN", true),
            ("OPENAI_API_BASE", true),
            ("openai_api_key", true),
            ("HTTP_PROXY", false),
            ("HTTPS_PROXY", false),
            ("ALL_PROXY", false),
            ("NO_PROXY", false),
            ("PATH", false),
            ("", false),
        ];
        for (name, expected) in cases {
            assert_eq!(is_conflict_name(name), expected, "{name}");
        }
    }

    #[test]
    fn evaluate_clean_when_only_proxies_present() {
        let findings = vec![
            finding("OPENAI_API_KEY", false, &[]),
            finding(
                "HTTPS_PROXY",
                true,
                &[(EnvVarSourceKind::Process, "process")],
            ),
        ];
        let v = evaluate(&findings, Platform::Windows);
        assert_eq!(v.code, "env_vars.clean");
        assert_eq!(v.status, crate::models::CheckStatus::Pass);
        assert_eq!(v.details, vec!["HTTPS_PROXY ← process"]);
        assert!(v.fixes.is_empty());
    }

    #[test]
    fn evaluate_warns_on_conflicts_with_platform_instructions() {
        let findings = vec![
            finding(
                "OPENAI_API_KEY",
                true,
                &[
                    (EnvVarSourceKind::Process, "process"),
                    (EnvVarSourceKind::UserRegistry, USER_REGISTRY_LOCATION),
                ],
            ),
            finding(
                "OPENAI_BASE_URL",
                false,
                &[(EnvVarSourceKind::ShellRc, "~/.zshrc:12")],
            ),
            finding("HTTPS_PROXY", false, &[]),
        ];
        let v = evaluate(&findings, Platform::Macos);
        assert_eq!(v.code, "env_vars.conflicts");
        assert_eq!(v.status, crate::models::CheckStatus::Warn);
        assert_eq!(
            v.params.get("names").map(String::as_str),
            Some("OPENAI_API_KEY, OPENAI_BASE_URL")
        );
        assert_eq!(
            v.details,
            vec![
                "OPENAI_API_KEY ← process",
                format!("OPENAI_API_KEY ← {USER_REGISTRY_LOCATION}").as_str(),
                "OPENAI_BASE_URL ← ~/.zshrc:12",
            ]
        );
        assert_eq!(
            v.fixes[0],
            FixAction::CleanEnvVars {
                names: vec!["OPENAI_API_KEY".into(), "OPENAI_BASE_URL".into()]
            }
        );
        assert!(matches!(
            &v.fixes[1],
            FixAction::Instructions { code, params }
                if code == "checks:env_vars.instructions.macos"
                    && params.get("names").map(String::as_str) == Some("OPENAI_API_KEY, OPENAI_BASE_URL")
        ));
        assert_eq!(v.fixes[2], FixAction::Rerun);
        let win = evaluate(&findings, Platform::Windows);
        assert!(matches!(
            &win.fixes[1],
            FixAction::Instructions { code, .. } if code == "checks:env_vars.instructions.windows"
        ));
    }

    #[test]
    fn rc_scanner_recognises_shell_forms() {
        let content = "\
# export OPENAI_API_KEY=commented
export OPENAI_API_KEY=sk-abc
OPENAI_BASE_URL=\"https://x\" HTTPS_PROXY=http://p
  export PATH=$PATH:/x ANTHROPIC_AUTH_TOKEN=abc
setenv OPENAI_API_KEY value
set -gx OPENAI_BASE_URL https://y
set -l NOT_EXPORTED_API_KEY x
echo OPENAI_API_KEY=notanassignment
export OPENAI_API_KEY
";
        let hits = scan_rc_content(content, &names());
        assert_eq!(
            hits,
            vec![
                ("OPENAI_API_KEY".to_owned(), 2),
                ("OPENAI_BASE_URL".to_owned(), 3),
                ("HTTPS_PROXY".to_owned(), 3),
                ("ANTHROPIC_AUTH_TOKEN".to_owned(), 4),
                ("OPENAI_API_KEY".to_owned(), 5),
                ("OPENAI_BASE_URL".to_owned(), 6),
            ]
        );
        assert!(scan_rc_content("", &names()).is_empty());
    }

    #[test]
    fn rc_scanner_recognises_powershell_forms() {
        let content = "\
# $env:OPENAI_API_KEY = 'commented'
$env:OPENAI_API_KEY = 'sk-abc'
$Env:openai_base_url=\"https://x\"
$env:HTTPS_PROXY += ';x'
if ($env:OPENAI_API_KEY -eq 'x') { Write-Host $env:ANTHROPIC_AUTH_TOKEN }
[Environment]::SetEnvironmentVariable('ANTHROPIC_AUTH_TOKEN', 'abc', 'User')
[System.Environment]::SetEnvironmentVariable( \"OPENAI_BASE_URL\", $v )
Write-Output \"OPENAI_API_KEY=notanassignment\"
$env:PATH = \"$env:PATH;C:\\x\"
";
        let hits = scan_rc_content(content, &names());
        assert_eq!(
            hits,
            vec![
                ("OPENAI_API_KEY".to_owned(), 2),
                ("OPENAI_BASE_URL".to_owned(), 3),
                ("HTTPS_PROXY".to_owned(), 4),
                ("ANTHROPIC_AUTH_TOKEN".to_owned(), 6),
                ("OPENAI_BASE_URL".to_owned(), 7),
            ]
        );
    }

    #[test]
    fn rc_scanner_reads_powershell_profile_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir
            .path()
            .join("Documents")
            .join("PowerShell")
            .join("profile.ps1");
        std::fs::create_dir_all(profile.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &profile,
            "Set-Alias ll ls\n$env:OPENAI_API_KEY = \"sk-1\"\n",
        )
        .expect("write");
        assert_eq!(
            scan_rc_file(&profile, &names()),
            vec![("OPENAI_API_KEY".to_owned(), 2)]
        );
        assert_eq!(
            rc_location(&profile, Some(dir.path()), 2),
            "~/Documents/PowerShell/profile.ps1:2"
        );
    }

    #[test]
    fn rc_scanner_reads_temp_files_and_formats_locations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(".zshrc");
        std::fs::write(&file, "alias ll=ls\nexport OPENAI_API_KEY=sk-1\n").expect("write");
        let hits = scan_rc_file(&file, &names());
        assert_eq!(hits, vec![("OPENAI_API_KEY".to_owned(), 2)]);
        assert!(scan_rc_file(&dir.path().join("missing"), &names()).is_empty());
        assert_eq!(rc_location(&file, Some(dir.path()), 2), "~/.zshrc:2");
        assert_eq!(
            rc_location(Path::new("/etc/zshrc"), Some(dir.path()), 7),
            "/etc/zshrc:7"
        );
    }

    #[test]
    fn registry_lookup_is_case_insensitive() {
        let map: BTreeMap<String, String> = [("Openai_Api_Key".to_owned(), "v".to_owned())].into();
        assert_eq!(lookup(&map, "OPENAI_API_KEY"), Some("v"));
        assert_eq!(lookup(&map, "OTHER"), None);
    }

    #[tokio::test]
    async fn inspect_reports_process_variables_masked() {
        // Only look at a name that is guaranteed present in this process.
        let key = if cfg!(windows) { "PATH" } else { "HOME" };
        let findings =
            inspect_env_var_names(&[key.to_owned(), "CODEX_ONBOARDING_NOPE_42".to_owned()]).await;
        assert_eq!(findings.len(), 2);
        assert!(findings[0].present_in_session);
        assert!(findings[0]
            .sources
            .iter()
            .any(|s| s.kind == EnvVarSourceKind::Process));
        let masked = findings[0].value_masked.clone().unwrap_or_default();
        assert!(masked.contains('*'), "{masked}");
        assert!(!findings[1].present_in_session);
        assert!(findings[1].sources.is_empty());
        assert!(findings[1].value_masked.is_none());
    }

    #[tokio::test]
    async fn check_with_embedded_config_returns_a_known_code() {
        let cfg = crate::config::embedded().expect("config");
        let (v, findings) = check(&cfg).await;
        assert_eq!(findings.len(), cfg.env_vars_to_inspect.len());
        assert!(v.code == "env_vars.clean" || v.code == "env_vars.conflicts");
    }
}

//! One-click remediation (ADR-0008): the few machine changes this app performs itself, each
//! as a *plan → confirm → apply* pair so the UI can show exactly what will change first.
//!
//! - **PATH repair** (`plan_path_repair` / `apply_path_repair`): adds one directory to the
//!   user's persistent `PATH` — Windows `HKCU\Environment\Path` (kept `REG_EXPAND_SZ`, then a
//!   `WM_SETTINGCHANGE` broadcast so new terminals see it), macOS/Linux an `export PATH=…`
//!   line appended to the login shell's rc file (backup first). Never touches the machine
//!   `PATH`, never removes anything.
//! - **Env-var cleanup** (`plan_env_cleanup` / `apply_env_cleanup`): removes conflicting
//!   variables (`*_API_KEY`, `*_BASE_URL`, …) from their persistent sources. The plan carries
//!   the **full** current values because the user explicitly asked to see them before they
//!   are deleted; they are returned over IPC once and never logged. Sources: user registry
//!   (direct delete), machine registry (elevated `reg delete`, UAC prompt), shell rc files (the
//!   defining line is commented out, backup first), `launchctl unsetenv`. A value that only
//!   exists in this process (inherited from the launcher) cannot be removed and is reported as
//!   such.
//! - **Codex `config.toml`** (`codex_config_status` / `apply_codex_config` /
//!   `restore_codex_config`): writes the confirmed template to `~/.codex/config.toml` after
//!   copying the current file to `config.toml.seedrouter-<timestamp>.bak`; restore puts the
//!   newest backup back. Supersedes the "never write `~/.codex`" part of ADR-0003 for this one
//!   file, on explicit user action only.
//!
//! Everything that decides *what* to do is pure and unit-tested (`path_line`, `comment_out`,
//! `path_contains_dir`, …); the functions that touch the machine are thin.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::checks::env_vars::{
    self, LAUNCHCTL_LOCATION, MACHINE_REGISTRY_LOCATION, PROCESS_LOCATION, USER_REGISTRY_LOCATION,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    AppConfig, CodexConfigApplyResult, CodexConfigStatus, EnvCleanupAction, EnvCleanupItem,
    EnvCleanupPlan, EnvCleanupResult, EnvVarSource, EnvVarSourceKind, PathRepairPlan,
    PathRepairResult, Platform, ToolId,
};
use crate::platform;
use crate::process::{self, CommandSpec};
use crate::redact::redact_secrets;

/// Marker appended to every line this app writes into a user file, so a reader (and `restore`)
/// can tell them apart.
pub const MARKER: &str = "SeedRouter Onboarding";

/// Timeout for the small helper processes (`launchctl`, `reg`, the broadcast).
const HELPER_TIMEOUT: Duration = Duration::from_secs(20);
/// An elevated `reg delete` waits for the UAC prompt.
const ELEVATED_TIMEOUT: Duration = Duration::from_secs(120);

/// Backup suffix: `<file>.seedrouter-<UTC timestamp>.bak`.
fn backup_path_for(file: &Path) -> PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let name = file
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let first = file.with_file_name(format!("{name}.seedrouter-{stamp}.bak"));
    if !first.exists() {
        return first;
    }
    // Same second as a previous backup: number the file so nothing is overwritten.
    (2..1000)
        .map(|n| file.with_file_name(format!("{name}.seedrouter-{stamp}-{n}.bak")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Copies `file` to a timestamped sibling and returns the backup path (`None` when `file`
/// does not exist yet).
fn backup_file(file: &Path) -> AppResult<Option<PathBuf>> {
    if !file.is_file() {
        return Ok(None);
    }
    let backup = backup_path_for(file);
    std::fs::copy(file, &backup)?;
    Ok(Some(backup))
}

/// `~`-relative display of a path under the home directory.
fn display_path(path: &Path) -> String {
    match platform::home_dir().and_then(|h| path.strip_prefix(&h).ok().map(Path::to_path_buf)) {
        Some(rel) => format!("~/{}", rel.to_string_lossy().replace('\\', "/")),
        None => path.to_string_lossy().into_owned(),
    }
}

// ---------------------------------------------------------------------------
// PATH repair
// ---------------------------------------------------------------------------

/// Registry location label of the user PATH (Windows).
pub const USER_PATH_LOCATION: &str = r"HKCU\Environment\Path";

/// Normalises a PATH entry for comparison: trimmed, surrounding quotes and trailing
/// separators removed, backslashes unified, case-folded on Windows.
pub fn normalize_path_entry(entry: &str, platform: Platform) -> String {
    let trimmed = entry.trim().trim_matches('"').trim_end_matches(['/', '\\']);
    let unified = trimmed.replace('\\', "/");
    if platform == Platform::Windows {
        unified.to_ascii_lowercase()
    } else {
        unified
    }
}

/// `true` when `dir` is one of the entries of `path_value` (`;` on Windows, `:` elsewhere).
pub fn path_contains_dir(path_value: &str, dir: &str, platform: Platform) -> bool {
    let sep = if platform == Platform::Windows {
        ';'
    } else {
        ':'
    };
    let wanted = normalize_path_entry(dir, platform);
    if wanted.is_empty() {
        return true;
    }
    path_value
        .split(sep)
        .any(|e| normalize_path_entry(e, platform) == wanted)
}

/// The Windows user PATH with `dir` appended (`;`-joined, no duplicate separator).
pub fn appended_windows_path(current: &str, dir: &str) -> String {
    let trimmed = current.trim_end_matches(';');
    if trimmed.is_empty() {
        dir.to_owned()
    } else {
        format!("{trimmed};{dir}")
    }
}

/// The line appended to a shell rc file for `dir` (POSIX shells; fish gets `fish_add_path`).
pub fn path_line(dir: &str, rc_file: &Path) -> String {
    let is_fish = rc_file
        .file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".fish"));
    let escaped = dir.replace('"', "\\\"");
    if is_fish {
        format!("fish_add_path \"{escaped}\"  # added by {MARKER}")
    } else {
        format!("export PATH=\"{escaped}:$PATH\"  # added by {MARKER}")
    }
}

/// Which rc file the login shell reads (so the fresh-session re-check and every new terminal
/// see the change): zsh → `~/.zshrc`, bash → `~/.bash_profile` (macOS) / `~/.bashrc` (Linux),
/// fish → `~/.config/fish/config.fish`, anything else → `~/.profile`. Pure.
pub fn rc_file_for_shell(shell: &str, home: &Path, platform: Platform) -> PathBuf {
    let name = shell.rsplit('/').next().unwrap_or(shell).trim();
    match name {
        "zsh" | "" => home.join(".zshrc"),
        "bash" => {
            if platform == Platform::Macos {
                home.join(".bash_profile")
            } else {
                home.join(".bashrc")
            }
        }
        "fish" => home.join(".config").join("fish").join("config.fish"),
        _ => home.join(".profile"),
    }
}

/// Validates the directory argument of a PATH repair: absolute, existing directory.
fn validate_dir(dir: &str) -> AppResult<PathBuf> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("directory is empty".into()));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(AppError::InvalidInput(
            "directory must be an absolute path".into(),
        ));
    }
    if !path.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "directory does not exist: {trimmed}"
        )));
    }
    Ok(path)
}

/// Builds the PATH repair plan for `dir` on this machine (pure decision over live reads).
pub async fn plan_path_repair(dir: &str) -> AppResult<PathRepairPlan> {
    let path = validate_dir(dir)?;
    let dir = path.to_string_lossy().into_owned();
    let platform = platform::platform();
    match platform {
        Platform::Windows => {
            let current = read_user_path().unwrap_or_default();
            let already = path_contains_dir(&current, &dir, platform)
                || fresh_path_contains(&dir, platform).await;
            Ok(PathRepairPlan {
                display_command: format!(
                    "reg add \"HKCU\\Environment\" /v Path /t REG_EXPAND_SZ /d \"{}\"",
                    appended_windows_path(&current, &dir)
                ),
                dir,
                platform,
                location: USER_PATH_LOCATION.to_owned(),
                already_present: already,
                creates_backup: false,
            })
        }
        Platform::Macos | Platform::Linux | Platform::Unknown => {
            let home = platform::home_dir()
                .ok_or_else(|| AppError::Other("home directory unavailable".into()))?;
            let shell = std::env::var("SHELL").unwrap_or_default();
            let rc = rc_file_for_shell(&shell, &home, platform);
            let line = path_line(&dir, &rc);
            let already = rc_mentions_dir(&rc, &dir) || fresh_path_contains(&dir, platform).await;
            Ok(PathRepairPlan {
                display_command: format!("echo '{line}' >> {}", display_path(&rc)),
                dir,
                platform,
                location: display_path(&rc),
                already_present: already,
                creates_backup: rc.is_file(),
            })
        }
    }
}

/// Applies a previously shown plan. The plan is re-derived for the same directory and the
/// two renderings must match, so a tampered DTO cannot change what runs.
pub async fn apply_path_repair(confirmed: &PathRepairPlan) -> AppResult<PathRepairResult> {
    let plan = plan_path_repair(&confirmed.dir).await?;
    if plan.display_command != confirmed.display_command {
        return Err(AppError::InvalidInput(
            "PATH repair plan does not match the current machine state".into(),
        ));
    }
    if plan.already_present {
        return Ok(PathRepairResult {
            changed: false,
            location: plan.location,
            backup_path: None,
        });
    }
    match plan.platform {
        Platform::Windows => {
            let current = read_user_path().unwrap_or_default();
            write_user_path(&appended_windows_path(&current, &plan.dir))?;
            broadcast_environment_change().await;
            log::info!("PATH repair: appended {} to {USER_PATH_LOCATION}", plan.dir);
            Ok(PathRepairResult {
                changed: true,
                location: plan.location,
                backup_path: None,
            })
        }
        Platform::Macos | Platform::Linux | Platform::Unknown => {
            let home = platform::home_dir()
                .ok_or_else(|| AppError::Other("home directory unavailable".into()))?;
            let shell = std::env::var("SHELL").unwrap_or_default();
            let rc = rc_file_for_shell(&shell, &home, plan.platform);
            let backup = backup_file(&rc)?;
            if let Some(parent) = rc.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut content = std::fs::read_to_string(&rc).unwrap_or_default();
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&path_line(&plan.dir, &rc));
            content.push('\n');
            std::fs::write(&rc, content)?;
            log::info!("PATH repair: appended {} to {}", plan.dir, rc.display());
            Ok(PathRepairResult {
                changed: true,
                location: plan.location,
                backup_path: backup.map(|b| b.to_string_lossy().into_owned()),
            })
        }
    }
}

/// `true` when the fresh-session PATH (what a new terminal gets) already contains `dir`.
async fn fresh_path_contains(dir: &str, platform: Platform) -> bool {
    match process::fresh_session_env().await {
        Ok(env) => {
            process::get_env_var(&env, "PATH").is_some_and(|p| path_contains_dir(p, dir, platform))
        }
        Err(_) => false,
    }
}

/// `true` when `rc` already has a (non-comment) line mentioning `dir`.
fn rc_mentions_dir(rc: &Path, dir: &str) -> bool {
    std::fs::read_to_string(rc).is_ok_and(|c| {
        c.lines()
            .map(str::trim)
            .any(|l| !l.starts_with('#') && l.contains(dir))
    })
}

#[cfg(windows)]
fn read_user_path() -> Option<String> {
    windows_registry::read_user_value("Path")
}

#[cfg(not(windows))]
fn read_user_path() -> Option<String> {
    None
}

#[cfg(windows)]
fn write_user_path(value: &str) -> AppResult<()> {
    windows_registry::write_user_expand_value("Path", value)
}

#[cfg(not(windows))]
fn write_user_path(_value: &str) -> AppResult<()> {
    Err(AppError::Unsupported(
        "the user PATH registry value exists on Windows only".into(),
    ))
}

/// Broadcasts `WM_SETTINGCHANGE("Environment")` so Explorer and new terminals reload the
/// user environment. Best effort: failures are logged, never surfaced (the registry change
/// itself already happened and the fresh-session re-check reads the registry directly).
async fn broadcast_environment_change() {
    if platform::platform() != Platform::Windows {
        return;
    }
    let spec = CommandSpec::new(
        "powershell.exe",
        [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            BROADCAST_SCRIPT,
        ],
    )
    .with_timeout(HELPER_TIMEOUT);
    match process::run(&spec).await {
        Ok(out) if out.success() => {}
        Ok(out) => log::warn!(
            "WM_SETTINGCHANGE broadcast exited with {:?}: {}",
            out.exit_code,
            redact_secrets(out.stderr.trim())
        ),
        Err(e) => log::warn!("WM_SETTINGCHANGE broadcast failed: {e}"),
    }
}

/// Fixed PowerShell one-liner (no interpolation) that P/Invokes `SendMessageTimeout` with
/// `HWND_BROADCAST` / `WM_SETTINGCHANGE` / `"Environment"`.
const BROADCAST_SCRIPT: &str = concat!(
    "$sig = '[DllImport(\"user32.dll\", SetLastError = true, CharSet = CharSet.Auto)] ",
    "public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, ",
    "string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'; ",
    "$t = Add-Type -MemberDefinition $sig -Name NativeMethods -Namespace SeedRouter -PassThru; ",
    "[UIntPtr]$r = [UIntPtr]::Zero; ",
    "[void]$t::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$r)"
);

// ---------------------------------------------------------------------------
// Env-var cleanup
// ---------------------------------------------------------------------------

/// Comments a line out: `# removed by SeedRouter Onboarding: <line>`. Pure.
pub fn comment_out(line: &str) -> String {
    format!("# removed by {MARKER}: {line}")
}

/// `content` with the 1-based `line_no` commented out; `None` when the line does not exist.
/// Preserves every other byte (line endings included). Pure.
pub fn comment_out_line(content: &str, line_no: usize) -> Option<String> {
    let mut out = String::with_capacity(content.len() + 48);
    let mut found = false;
    for (idx, piece) in content.split_inclusive('\n').enumerate() {
        if idx + 1 == line_no {
            found = true;
            let (body, ending) = match piece.strip_suffix("\r\n") {
                Some(b) => (b, "\r\n"),
                None => match piece.strip_suffix('\n') {
                    Some(b) => (b, "\n"),
                    None => (piece, ""),
                },
            };
            out.push_str(&comment_out(body));
            out.push_str(ending);
        } else {
            out.push_str(piece);
        }
    }
    found.then_some(out)
}

/// Parses `~/.zshrc:12`-style locations back into `(path, line)`; pure.
pub fn parse_rc_location(location: &str, home: Option<&Path>) -> Option<(PathBuf, usize)> {
    let (file, line) = location.rsplit_once(':')?;
    let line: usize = line.parse().ok()?;
    let path = match file.strip_prefix("~/") {
        Some(rel) => home?.join(rel),
        None => PathBuf::from(file),
    };
    Some((path, line))
}

/// Builds the cleanup plan for `names` (conflict names only; proxies are never touched).
pub async fn plan_env_cleanup(names: &[String], config: &AppConfig) -> AppResult<EnvCleanupPlan> {
    let platform = platform::platform();
    let wanted: Vec<String> = names
        .iter()
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .filter(|n| env_vars::is_conflict_name(n))
        .filter(|n| {
            config
                .env_vars_to_inspect
                .iter()
                .any(|k| k.eq_ignore_ascii_case(n))
        })
        .collect();
    if wanted.is_empty() {
        return Err(AppError::InvalidInput(
            "no removable variable names given".into(),
        ));
    }
    let (user_reg, machine_reg) = env_vars::registry_env_maps();
    let home = platform::home_dir();
    let mut items = Vec::new();
    for name in &wanted {
        if let Some(value) = lookup(&user_reg, name) {
            items.push(EnvCleanupItem {
                name: name.clone(),
                source: EnvVarSource {
                    kind: EnvVarSourceKind::UserRegistry,
                    location: USER_REGISTRY_LOCATION.to_owned(),
                },
                value: Some(value.to_owned()),
                line: None,
                action: EnvCleanupAction::DeleteUserRegistry,
                display_command: format!("reg delete \"HKCU\\Environment\" /v {name} /f"),
            });
        }
        if let Some(value) = lookup(&machine_reg, name) {
            items.push(EnvCleanupItem {
                name: name.clone(),
                source: EnvVarSource {
                    kind: EnvVarSourceKind::MachineRegistry,
                    location: MACHINE_REGISTRY_LOCATION.to_owned(),
                },
                value: Some(value.to_owned()),
                line: None,
                action: EnvCleanupAction::DeleteMachineRegistry,
                display_command: format!(
                    "reg delete \"{MACHINE_REGISTRY_LOCATION}\" /v {name} /f  (as administrator)"
                ),
            });
        }
        for file in platform::shell_rc_files() {
            for (hit_name, line_no) in env_vars::scan_rc_file(&file, std::slice::from_ref(name)) {
                if hit_name != *name {
                    continue;
                }
                let line = std::fs::read_to_string(&file)
                    .ok()
                    .and_then(|c| c.lines().nth(line_no.saturating_sub(1)).map(str::to_owned));
                let location = env_vars::rc_location(&file, home.as_deref(), line_no);
                items.push(EnvCleanupItem {
                    name: name.clone(),
                    source: EnvVarSource {
                        kind: EnvVarSourceKind::ShellRc,
                        location: location.clone(),
                    },
                    value: None,
                    line,
                    action: EnvCleanupAction::CommentOutRcLine,
                    display_command: format!("# comment out {location} (backup first)"),
                });
            }
        }
        if platform == Platform::Macos {
            if let Some(value) = launchctl_value(name).await {
                items.push(EnvCleanupItem {
                    name: name.clone(),
                    source: EnvVarSource {
                        kind: EnvVarSourceKind::Launchctl,
                        location: LAUNCHCTL_LOCATION.to_owned(),
                    },
                    value: Some(value),
                    line: None,
                    action: EnvCleanupAction::LaunchctlUnsetenv,
                    display_command: format!("launchctl unsetenv {name}"),
                });
            }
        }
        if let Ok(value) = std::env::var(name) {
            // Only report the process value when no persistent source explains it.
            let explained = items.iter().any(|i| &i.name == name);
            if !explained {
                items.push(EnvCleanupItem {
                    name: name.clone(),
                    source: EnvVarSource {
                        kind: EnvVarSourceKind::Process,
                        location: PROCESS_LOCATION.to_owned(),
                    },
                    value: Some(value),
                    line: None,
                    action: EnvCleanupAction::None,
                    display_command: String::new(),
                });
            }
        }
    }
    let requires_admin = items
        .iter()
        .any(|i| i.action == EnvCleanupAction::DeleteMachineRegistry);
    Ok(EnvCleanupPlan {
        platform,
        items,
        requires_admin,
    })
}

/// Applies a confirmed cleanup plan. Re-derives the plan for the same names; items that are no
/// longer present are skipped, items whose value changed since the preview are reported as
/// failed (the user confirmed a different value).
pub async fn apply_env_cleanup(
    confirmed: &EnvCleanupPlan,
    config: &AppConfig,
) -> AppResult<EnvCleanupResult> {
    let names: Vec<String> = confirmed
        .items
        .iter()
        .map(|i| i.name.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let current = plan_env_cleanup(&names, config).await?;
    let mut result = EnvCleanupResult {
        removed: Vec::new(),
        failed: Vec::new(),
        backups: Vec::new(),
    };
    let home = platform::home_dir();
    let mut registry_changed = false;
    // Rc files: apply per file so several lines in one file keep their numbering consistent
    // (lines are commented out, never removed — numbers do not shift).
    for item in &confirmed.items {
        let label = format!("{} ← {}", item.name, item.source.location);
        let Some(live) = current
            .items
            .iter()
            .find(|c| c.name == item.name && c.source == item.source && c.action == item.action)
        else {
            // Already gone (or was process-only): nothing to do.
            if item.action != EnvCleanupAction::None {
                result.removed.push(format!("{label} (already removed)"));
            }
            continue;
        };
        if live.value != item.value || live.line != item.line {
            result
                .failed
                .push(format!("{label}: value changed since the preview"));
            continue;
        }
        let outcome = match item.action {
            EnvCleanupAction::DeleteUserRegistry => delete_user_value(&item.name).map(|()| {
                registry_changed = true;
            }),
            EnvCleanupAction::DeleteMachineRegistry => {
                delete_machine_value_elevated(&item.name).await.map(|()| {
                    registry_changed = true;
                })
            }
            EnvCleanupAction::CommentOutRcLine => {
                comment_out_rc(&item.source.location, home.as_deref(), &mut result.backups)
            }
            EnvCleanupAction::LaunchctlUnsetenv => launchctl_unsetenv(&item.name).await,
            EnvCleanupAction::None => Err(AppError::Unsupported(
                "set only in this process; restart the launcher instead".into(),
            )),
        };
        match outcome {
            Ok(()) => {
                log::info!("env cleanup: removed {}", redact_secrets(&label));
                result.removed.push(label);
            }
            Err(e) => result
                .failed
                .push(format!("{label}: {}", redact_secrets(&e.to_string()))),
        }
    }
    if registry_changed {
        broadcast_environment_change().await;
    }
    Ok(result)
}

/// Case-insensitive lookup (Windows registry value names are case-insensitive).
fn lookup<'a>(map: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    map.get(name).map(String::as_str).or_else(|| {
        map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    })
}

async fn launchctl_value(name: &str) -> Option<String> {
    let spec = CommandSpec::new("launchctl", ["getenv", name]).with_timeout(HELPER_TIMEOUT);
    match process::run(&spec).await {
        Ok(out) if out.success() => {
            let v = out.stdout.trim();
            (!v.is_empty()).then(|| v.to_owned())
        }
        _ => None,
    }
}

async fn launchctl_unsetenv(name: &str) -> AppResult<()> {
    let spec = CommandSpec::new("launchctl", ["unsetenv", name]).with_timeout(HELPER_TIMEOUT);
    let out = process::run(&spec).await?;
    if out.success() {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            program: "launchctl".into(),
            code: out.exit_code,
            stderr_tail: redact_secrets(out.stderr.trim()),
        })
    }
}

fn comment_out_rc(location: &str, home: Option<&Path>, backups: &mut Vec<String>) -> AppResult<()> {
    let (file, line_no) = parse_rc_location(location, home)
        .ok_or_else(|| AppError::InvalidInput(format!("unrecognised location {location}")))?;
    let content = std::fs::read_to_string(&file)?;
    let updated = comment_out_line(&content, line_no)
        .ok_or_else(|| AppError::InvalidInput(format!("{location}: line not found")))?;
    if let Some(backup) = backup_file(&file)? {
        backups.push(backup.to_string_lossy().into_owned());
    }
    std::fs::write(&file, updated)?;
    Ok(())
}

#[cfg(windows)]
fn delete_user_value(name: &str) -> AppResult<()> {
    windows_registry::delete_user_value(name)
}

#[cfg(not(windows))]
fn delete_user_value(_name: &str) -> AppResult<()> {
    Err(AppError::Unsupported(
        "user registry exists on Windows only".into(),
    ))
}

/// `reg delete HKLM\…\Environment /v NAME /f` through an elevated PowerShell `Start-Process
/// -Verb RunAs` (UAC prompt). The variable name is validated (identifier characters only) so
/// it can be embedded in the argument list safely.
async fn delete_machine_value_elevated(name: &str) -> AppResult<()> {
    if platform::platform() != Platform::Windows {
        return Err(AppError::Unsupported(
            "machine registry exists on Windows only".into(),
        ));
    }
    if !is_identifier(name) {
        return Err(AppError::InvalidInput(format!(
            "invalid variable name {name}"
        )));
    }
    let script = format!(
        "$p = Start-Process -FilePath reg.exe -ArgumentList @('delete','{MACHINE_REGISTRY_LOCATION}','/v','{name}','/f') -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
    );
    let spec = CommandSpec::new(
        "powershell.exe",
        [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script.as_str(),
        ],
    )
    .with_timeout(ELEVATED_TIMEOUT);
    let out = process::run(&spec).await?;
    if out.success() {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            program: "reg.exe".into(),
            code: out.exit_code,
            stderr_tail: redact_secrets(out.stderr.trim()),
        })
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------------------------------------------------------------------------
// Codex config.toml
// ---------------------------------------------------------------------------

/// Name of the file this app writes inside the Codex config dir.
pub const CODEX_CONFIG_FILE: &str = "config.toml";

/// `~/.codex/config.toml` from the preset's `configDir` of the codex tool.
pub fn codex_config_path(config: &AppConfig) -> AppResult<PathBuf> {
    let dir = config
        .tools
        .iter()
        .find(|t| t.id == ToolId::Codex)
        .map(|t| t.config_dir.as_str())
        .filter(|d| !d.trim().is_empty())
        .unwrap_or("~/.codex");
    let dir = platform::expand_tilde(dir);
    if dir.as_os_str().is_empty() {
        return Err(AppError::Other("codex config dir unavailable".into()));
    }
    Ok(dir.join(CODEX_CONFIG_FILE))
}

/// Backups next to `file` written by this app, newest first. Pure over a directory listing.
pub fn list_backups(file: &Path) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else {
        return Vec::new();
    };
    let Some(stem) = file.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Vec::new();
    };
    let prefix = format!("{stem}.seedrouter-");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy())
                        .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".bak"))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found.reverse();
    found
}

/// `true` when two config texts are equal ignoring trailing whitespace and line endings.
pub fn same_config(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace("\r\n", "\n").trim_end().to_owned();
    norm(a) == norm(b)
}

/// Current state of the live file; `template` (optional) enables `matches_template`.
pub fn codex_config_status(
    config: &AppConfig,
    template: Option<&str>,
) -> AppResult<CodexConfigStatus> {
    let path = codex_config_path(config)?;
    let current = std::fs::read_to_string(&path).ok();
    let matches = match (&current, template) {
        (Some(cur), Some(tpl)) if !tpl.trim().is_empty() => Some(same_config(cur, tpl)),
        _ => None,
    };
    Ok(CodexConfigStatus {
        path: path.to_string_lossy().into_owned(),
        exists: current.is_some(),
        current_redacted: current.as_deref().map(redact_secrets),
        backups: list_backups(&path)
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        matches_template: matches,
    })
}

/// Writes `content` to `~/.codex/config.toml` (backup of the existing file first). The content
/// must be valid TOML; it is never logged (it may contain the key).
pub fn apply_codex_config(config: &AppConfig, content: &str) -> AppResult<CodexConfigApplyResult> {
    if content.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "config.toml content is empty".into(),
        ));
    }
    toml::from_str::<toml::Value>(content)
        .map_err(|e| AppError::InvalidInput(format!("config.toml is not valid TOML: {e}")))?;
    let path = codex_config_path(config)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let backup = backup_file(&path)?;
    let mut text = content.replace("\r\n", "\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(&path, &text)?;
    log::info!(
        "codex config.toml written to {} ({} bytes, backup: {})",
        path.display(),
        text.len(),
        backup
            .as_ref()
            .map_or("none".to_owned(), |b| b.display().to_string())
    );
    Ok(CodexConfigApplyResult {
        path: path.to_string_lossy().into_owned(),
        backup_path: backup.map(|b| b.to_string_lossy().into_owned()),
        bytes: text.len() as u64,
    })
}

/// Restores the newest backup over the live file (the current file is backed up first, so the
/// operation is itself reversible). Errors when there is no backup.
pub fn restore_codex_config(config: &AppConfig) -> AppResult<CodexConfigApplyResult> {
    let path = codex_config_path(config)?;
    let backups = list_backups(&path);
    let Some(newest) = backups.first() else {
        return Err(AppError::InvalidInput("no backup to restore".into()));
    };
    let content = std::fs::read_to_string(newest)?;
    let backup = backup_file(&path)?;
    std::fs::write(&path, &content)?;
    std::fs::remove_file(newest)?;
    log::info!(
        "codex config.toml restored from {} (previous live file kept as {})",
        newest.display(),
        backup
            .as_ref()
            .map_or("none".to_owned(), |b| b.display().to_string())
    );
    Ok(CodexConfigApplyResult {
        path: path.to_string_lossy().into_owned(),
        backup_path: backup.map(|b| b.to_string_lossy().into_owned()),
        bytes: content.len() as u64,
    })
}

// ---------------------------------------------------------------------------
// Windows registry (user hive only — the machine hive is only ever touched elevated)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod windows_registry {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_EXPAND_SZ};
    use winreg::{RegKey, RegValue};

    use crate::error::{AppError, AppResult};

    const USER_KEY: &str = "Environment";

    /// Raw (unexpanded) string value of `HKCU\Environment\<name>`.
    pub(super) fn read_user_value(name: &str) -> Option<String> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(USER_KEY, KEY_READ)
            .ok()?;
        let raw = key.get_raw_value(name).ok()?;
        Some(decode_utf16(&raw.bytes))
    }

    /// Writes `HKCU\Environment\<name>` as `REG_EXPAND_SZ` (so `%VAR%` entries keep working).
    pub(super) fn write_user_expand_value(name: &str, value: &str) -> AppResult<()> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(USER_KEY, KEY_SET_VALUE)
            .map_err(|e| AppError::Other(format!("cannot open HKCU\\Environment: {e}")))?;
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        key.set_raw_value(
            name,
            &RegValue {
                bytes: std::borrow::Cow::Owned(bytes),
                vtype: REG_EXPAND_SZ,
            },
        )
        .map_err(|e| AppError::Other(format!("cannot write HKCU\\Environment\\{name}: {e}")))
    }

    pub(super) fn delete_user_value(name: &str) -> AppResult<()> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(USER_KEY, KEY_SET_VALUE)
            .map_err(|e| AppError::Other(format!("cannot open HKCU\\Environment: {e}")))?;
        key.delete_value(name)
            .map_err(|e| AppError::Other(format!("cannot delete HKCU\\Environment\\{name}: {e}")))
    }

    fn decode_utf16(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|u| *u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entry_normalisation_and_lookup() {
        assert!(path_contains_dir(
            r"C:\Windows;C:\Program Files\nodejs\;D:\x",
            r"c:/program files/nodejs",
            Platform::Windows
        ));
        assert!(!path_contains_dir(
            r"C:\Windows;C:\Program Files\nodejs",
            r"C:\Users\me\AppData\Roaming\npm",
            Platform::Windows
        ));
        assert!(path_contains_dir(
            "/usr/bin:/opt/homebrew/bin/:/usr/local/bin",
            "/opt/homebrew/bin",
            Platform::Macos
        ));
        // Case matters outside Windows.
        assert!(!path_contains_dir(
            "/usr/bin:/Opt/Homebrew/bin",
            "/opt/homebrew/bin",
            Platform::Macos
        ));
        assert!(path_contains_dir("", "", Platform::Macos));
    }

    #[test]
    fn windows_path_append_handles_separators() {
        assert_eq!(appended_windows_path("", r"C:\x"), r"C:\x");
        assert_eq!(appended_windows_path(r"C:\a;", r"C:\x"), r"C:\a;C:\x");
        assert_eq!(
            appended_windows_path(r"C:\a;%B%", r"C:\x"),
            r"C:\a;%B%;C:\x"
        );
    }

    #[test]
    fn rc_line_and_shell_mapping() {
        let home = Path::new("/Users/me");
        assert_eq!(
            rc_file_for_shell("/bin/zsh", home, Platform::Macos),
            home.join(".zshrc")
        );
        assert_eq!(
            rc_file_for_shell("/bin/bash", home, Platform::Macos),
            home.join(".bash_profile")
        );
        assert_eq!(
            rc_file_for_shell("bash", home, Platform::Linux),
            home.join(".bashrc")
        );
        assert_eq!(
            rc_file_for_shell("/opt/homebrew/bin/fish", home, Platform::Macos),
            home.join(".config/fish/config.fish")
        );
        assert_eq!(
            rc_file_for_shell("/bin/tcsh", home, Platform::Macos),
            home.join(".profile")
        );
        let line = path_line("/opt/homebrew/bin", &home.join(".zshrc"));
        assert_eq!(
            line,
            "export PATH=\"/opt/homebrew/bin:$PATH\"  # added by SeedRouter Onboarding"
        );
        let fish = path_line("/a b", &home.join(".config/fish/config.fish"));
        assert!(fish.starts_with("fish_add_path \"/a b\""));
    }

    #[test]
    fn commenting_out_keeps_every_other_byte() {
        let content = "a=1\r\nexport OPENAI_API_KEY=sk-x\r\nb=2";
        let out = comment_out_line(content, 2).expect("line exists");
        assert_eq!(
            out,
            "a=1\r\n# removed by SeedRouter Onboarding: export OPENAI_API_KEY=sk-x\r\nb=2"
        );
        let last = comment_out_line(content, 3).expect("last line");
        assert!(last.ends_with("# removed by SeedRouter Onboarding: b=2"));
        assert!(comment_out_line(content, 4).is_none());
        assert!(comment_out_line("", 1).is_none());
    }

    #[test]
    fn rc_location_round_trip() {
        let home = Path::new("/Users/me");
        assert_eq!(
            parse_rc_location("~/.zshrc:12", Some(home)),
            Some((home.join(".zshrc"), 12))
        );
        assert_eq!(
            parse_rc_location("/etc/zshrc:3", None),
            Some((PathBuf::from("/etc/zshrc"), 3))
        );
        assert_eq!(parse_rc_location("~/.zshrc", Some(home)), None);
        assert_eq!(parse_rc_location("~/.zshrc:x", Some(home)), None);
    }

    #[test]
    fn identifiers() {
        assert!(is_identifier("OPENAI_API_KEY"));
        assert!(is_identifier("_x1"));
        assert!(!is_identifier("1abc"));
        assert!(!is_identifier("A B"));
        assert!(!is_identifier("A;rm"));
        assert!(!is_identifier(""));
    }

    #[test]
    fn config_comparison_ignores_line_endings_and_trailing_space() {
        assert!(same_config("a = 1\r\nb = 2\r\n", "a = 1\nb = 2"));
        assert!(!same_config("a = 1", "a = 2"));
    }

    #[test]
    fn backups_are_listed_newest_first_and_restored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "model = \"a\"\n").expect("write");
        let b1 = backup_file(&file).expect("backup").expect("exists");
        assert!(b1
            .file_name()
            .expect("name")
            .to_string_lossy()
            .ends_with(".bak"));
        assert!(list_backups(&file).contains(&b1));
        assert!(backup_file(&dir.path().join("missing.toml"))
            .expect("no error")
            .is_none());
        // Unrelated files never count as backups.
        std::fs::write(dir.path().join("config.toml.bak"), "x").expect("write");
        assert_eq!(list_backups(&file).len(), 1);
    }

    #[test]
    fn apply_and_restore_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cfg = crate::config::embedded().expect("config");
        for t in &mut cfg.tools {
            if t.id == ToolId::Codex {
                t.config_dir = dir.path().join(".codex").to_string_lossy().into_owned();
            }
        }
        let path = codex_config_path(&cfg).expect("path");
        assert!(path.ends_with(Path::new(".codex").join("config.toml")));
        let status = codex_config_status(&cfg, Some("model = \"x\"")).expect("status");
        assert!(!status.exists);
        assert_eq!(status.matches_template, None);

        assert!(apply_codex_config(&cfg, "not = valid = toml").is_err());
        assert!(apply_codex_config(&cfg, "   ").is_err());
        let first = apply_codex_config(&cfg, "model = \"x\"").expect("apply");
        assert!(first.backup_path.is_none());
        let status = codex_config_status(&cfg, Some("model = \"x\"\n")).expect("status");
        assert_eq!(status.matches_template, Some(true));
        assert_eq!(status.current_redacted.as_deref(), Some("model = \"x\"\n"));

        let second = apply_codex_config(&cfg, "model = \"y\"\n[t]\nk = \"sk-abcdefghijklmnop\"")
            .expect("apply");
        assert!(second.backup_path.is_some());
        let status = codex_config_status(&cfg, Some("model = \"x\"")).expect("status");
        assert_eq!(status.matches_template, Some(false));
        assert_eq!(status.backups.len(), 1);
        assert!(!status
            .current_redacted
            .expect("content")
            .contains("sk-abcdefghijklmnop"));

        let restored = restore_codex_config(&cfg).expect("restore");
        assert!(restored.backup_path.is_some());
        let live = std::fs::read_to_string(&path).expect("read");
        assert_eq!(live, "model = \"x\"\n");
        // The restore consumed the backup and left the replaced file as a new one.
        assert_eq!(list_backups(&path).len(), 1);
    }

    #[test]
    fn display_paths_collapse_home() {
        if let Some(home) = platform::home_dir() {
            assert_eq!(display_path(&home.join(".zshrc")), "~/.zshrc");
        }
        assert_eq!(display_path(Path::new("/etc/zshrc")), "/etc/zshrc");
    }

    #[tokio::test]
    async fn plan_path_repair_rejects_bad_dirs() {
        assert!(plan_path_repair("").await.is_err());
        assert!(plan_path_repair("relative/dir").await.is_err());
        let missing = if cfg!(windows) {
            r"C:\seedrouter-does-not-exist-123"
        } else {
            "/seedrouter-does-not-exist-123"
        };
        assert!(plan_path_repair(missing).await.is_err());
    }

    #[tokio::test]
    async fn plan_path_repair_on_existing_dir_is_consistent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = plan_path_repair(&dir.path().to_string_lossy())
            .await
            .expect("plan");
        assert_eq!(plan.dir, dir.path().to_string_lossy());
        assert!(!plan.display_command.is_empty());
        assert!(!plan.already_present);
        // A plan whose rendering was tampered with is refused.
        let mut tampered = plan.clone();
        tampered.display_command.push_str(" && evil");
        assert!(apply_path_repair(&tampered).await.is_err());
    }

    #[tokio::test]
    async fn env_cleanup_plan_filters_names() {
        let cfg = crate::config::embedded().expect("config");
        assert!(plan_env_cleanup(&["HTTPS_PROXY".to_owned()], &cfg)
            .await
            .is_err());
        assert!(
            plan_env_cleanup(&["NOT_IN_PRESET_API_KEY".to_owned()], &cfg)
                .await
                .is_err()
        );
        let plan = plan_env_cleanup(&["OPENAI_API_KEY".to_owned()], &cfg)
            .await
            .expect("plan");
        assert_eq!(plan.platform, platform::platform());
        for item in &plan.items {
            assert_eq!(item.name, "OPENAI_API_KEY");
        }
    }
}

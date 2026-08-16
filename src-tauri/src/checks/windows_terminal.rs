//! Windows Terminal recommendation (Windows only). Not a requirement — the wizard works from
//! any console — but Windows Terminal gives users a far better copy/paste and rendering
//! experience, so a missing install is a `Warn` with a download fix, never a blocker.
//!
//! Detection: `wt` resolved the way a fresh terminal would (`locate_binary`), plus the
//! per-user app-execution-alias directory `%LOCALAPPDATA%\Microsoft\WindowsApps` (where the
//! Store install puts `wt.exe` even before a new terminal is opened). On macOS/Linux the
//! check is `Skipped` (`windows_terminal.not_applicable`) and the UI hides the row.
//!
//! Codes: `windows_terminal.ok` (Pass, param `path`) · `windows_terminal.missing` (Warn —
//! download page + install instructions + rerun) · `windows_terminal.not_applicable` (Skipped).

use std::path::PathBuf;

use crate::models::{CheckStatus, FixAction, Params, Platform};
use crate::platform;

use super::Verdict;

/// Label code of the Windows Terminal download `OpenUrl` fix.
pub const WINDOWS_TERMINAL_DOWNLOAD_LABEL: &str = "windows_terminal_download";

/// Microsoft's stable short link to the Windows Terminal store page.
const DOWNLOAD_URL: &str = "https://aka.ms/terminal";

/// Instructions fix (fully qualified i18n code).
const INSTALL_INSTRUCTIONS: &str = "checks:windows_terminal.instructions.install";

/// Runs the Windows Terminal recommendation check.
pub async fn check() -> Verdict {
    if platform::platform() != Platform::Windows {
        return not_applicable();
    }
    evaluate(find_windows_terminal().await)
}

/// Skipped verdict for platforms where Windows Terminal does not apply.
pub fn not_applicable() -> Verdict {
    Verdict::new(CheckStatus::Skipped, "windows_terminal.not_applicable")
}

/// Pure mapping of the detection result to a verdict.
pub fn evaluate(path: Option<PathBuf>) -> Verdict {
    match path {
        Some(path) => {
            let path = path.to_string_lossy().into_owned();
            Verdict::pass("windows_terminal.ok")
                .param("path", path.clone())
                .detail(path)
        }
        None => Verdict::warn("windows_terminal.missing")
            .fix(FixAction::OpenUrl {
                url: DOWNLOAD_URL.to_owned(),
                label_code: WINDOWS_TERMINAL_DOWNLOAD_LABEL.to_owned(),
            })
            .fix(FixAction::Instructions {
                code: INSTALL_INSTRUCTIONS.to_owned(),
                params: Params::new(),
            })
            .fix(FixAction::Rerun),
    }
}

/// `wt` as a fresh terminal would resolve it, falling back to the Store alias directory.
async fn find_windows_terminal() -> Option<PathBuf> {
    let extra: Vec<PathBuf> = std::env::var("LOCALAPPDATA")
        .ok()
        .map(|local| PathBuf::from(local).join("Microsoft").join("WindowsApps"))
        .into_iter()
        .collect();
    super::locate_binary("wt", &extra).await.map(|(path, _)| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CheckStatus;

    #[test]
    fn found_is_a_pass_with_the_path() {
        let v = evaluate(Some(PathBuf::from(r"C:\Users\me\WindowsApps\wt.exe")));
        assert_eq!(v.status, CheckStatus::Pass);
        assert_eq!(v.code, "windows_terminal.ok");
        assert_eq!(
            v.params.get("path").map(String::as_str),
            Some(r"C:\Users\me\WindowsApps\wt.exe")
        );
        assert!(v.fixes.is_empty());
    }

    #[test]
    fn missing_is_a_warn_with_download_instructions_and_rerun() {
        let v = evaluate(None);
        assert_eq!(v.status, CheckStatus::Warn);
        assert_eq!(v.code, "windows_terminal.missing");
        assert_eq!(
            v.fixes,
            vec![
                FixAction::OpenUrl {
                    url: "https://aka.ms/terminal".into(),
                    label_code: "windows_terminal_download".into(),
                },
                FixAction::Instructions {
                    code: "checks:windows_terminal.instructions.install".into(),
                    params: Params::new(),
                },
                FixAction::Rerun,
            ]
        );
    }

    #[test]
    fn not_applicable_is_skipped() {
        let v = not_applicable();
        assert_eq!(v.status, CheckStatus::Skipped);
        assert_eq!(v.code, "windows_terminal.not_applicable");
    }

    #[tokio::test]
    async fn check_yields_a_known_code_on_every_platform() {
        let v = check().await;
        assert!(v.code.starts_with("windows_terminal."), "{v:?}");
        if platform::platform() != Platform::Windows {
            assert_eq!(v.status, CheckStatus::Skipped);
        }
    }
}

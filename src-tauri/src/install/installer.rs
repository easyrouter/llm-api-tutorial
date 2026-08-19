//! Running a *downloaded* installer (Node.js MSI / pkg, Codex desktop client MSIX / DMG) as a
//! regular install job, and the static Codex-app release. Everything here is pure; the
//! downloads-dir containment check happens in [`super::start`] where the app handle is known.
//!
//! | target      | platform | command                                                        | admin |
//! | ----------- | -------- | -------------------------------------------------------------- | ----- |
//! | Node        | Windows  | `msiexec.exe /i <file> /passive /norestart` (UAC by MSI)        | yes   |
//! | Node        | macOS    | `osascript -e 'do shell script "installer -pkg <f> -target /" with administrator privileges'` | yes |
//! | CodexApp    | Windows  | `powershell Add-AppxPackage -Path <file>` (per-user, Store-signed MSIX) | no |
//! | CodexApp    | macOS    | `/bin/sh -c <mount dmg, ditto the .app into /Applications, detach> sh <file>` | no (admin group) |
//!
//! Explanation codes (UI: `install:plan.<code>`): `node.msi`, `node.pkg`, `codex_app.msix`,
//! `codex_app.dmg`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::models::{CodexAppSpec, InstallPlan, InstallTarget, InstallerRelease, Platform};
use crate::process::CommandSpec;

pub const CODE_NODE_MSI: &str = "node.msi";
pub const CODE_NODE_PKG: &str = "node.pkg";
pub const CODE_CODEX_APP_MSIX: &str = "codex_app.msix";
pub const CODE_CODEX_APP_DMG: &str = "codex_app.dmg";

/// Exit code msiexec returns when the install succeeded but a reboot is pending.
pub const MSIEXEC_REBOOT_REQUIRED: i32 = 3010;

/// Shell body that installs a `.app` from a DMG into `/Applications` (`$1` = dmg path).
pub const DMG_INSTALL_SCRIPT: &str = concat!(
    "set -e; ",
    "MNT=$(mktemp -d /tmp/seedrouter-dmg.XXXXXX); ",
    "hdiutil attach -nobrowse -readonly -mountpoint \"$MNT\" \"$1\" >/dev/null; ",
    "APP=$(find \"$MNT\" -maxdepth 1 -name '*.app' | head -n 1); ",
    "if [ -z \"$APP\" ]; then hdiutil detach \"$MNT\" >/dev/null; echo 'no .app in image' >&2; exit 2; fi; ",
    "echo \"Installing $(basename \"$APP\") into /Applications\"; ",
    "ditto \"$APP\" \"/Applications/$(basename \"$APP\")\"; ",
    "hdiutil detach \"$MNT\" >/dev/null; ",
    "echo done"
);

/// The command that runs the downloaded installer at `path` for `target` on `platform`.
pub fn run_plan(target: InstallTarget, path: &Path, platform: Platform) -> AppResult<InstallPlan> {
    let file = path.to_string_lossy().into_owned();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let (spec, code, requires_admin) = match (target, platform, ext.as_str()) {
        (InstallTarget::Node, Platform::Windows, "msi") => (
            CommandSpec::new(
                "msiexec.exe",
                ["/i", file.as_str(), "/passive", "/norestart"],
            ),
            CODE_NODE_MSI,
            true,
        ),
        (InstallTarget::Node, Platform::Macos, "pkg") => (
            CommandSpec::new(
                "osascript",
                [
                    "-e",
                    &format!(
                        "do shell script \"/usr/sbin/installer -pkg {} -target /\" with administrator privileges",
                        applescript_quote(&file)
                    ),
                ],
            ),
            CODE_NODE_PKG,
            true,
        ),
        (InstallTarget::CodexApp, Platform::Windows, "msix" | "msixbundle" | "appx") => (
            CommandSpec::new(
                "powershell.exe",
                [
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    "Add-AppxPackage",
                    "-Path",
                    file.as_str(),
                ],
            ),
            CODE_CODEX_APP_MSIX,
            false,
        ),
        (InstallTarget::CodexApp, Platform::Macos, "dmg") => (
            CommandSpec::new("/bin/sh", ["-c", DMG_INSTALL_SCRIPT, "sh", file.as_str()]),
            CODE_CODEX_APP_DMG,
            false,
        ),
        _ => {
            return Err(AppError::Unsupported(format!(
                "no installer command for {target:?} / {platform:?} / .{ext}"
            )))
        }
    };
    Ok(InstallPlan {
        target,
        display_command: spec.display(),
        program: spec.program,
        args: spec.args,
        env: BTreeMap::new(),
        registry: None,
        requires_admin,
        explanation_code: code.to_owned(),
        download_url: None,
        installer_path: Some(file),
    })
}

/// Quotes a path for embedding inside an AppleScript `do shell script` string: the shell sees
/// it single-quoted, and AppleScript sees the double quotes/backslashes escaped.
pub fn applescript_quote(path: &str) -> String {
    let shell = format!("'{}'", path.replace('\'', "'\\''"));
    shell.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Validates that `plan` is exactly what [`run_plan`] renders for its own `installer_path`
/// (pure; the caller checks that the path lives in the downloads dir). Accepts msiexec 3010.
pub fn validate_run_plan(plan: &InstallPlan, platform: Platform) -> Result<(), &'static str> {
    let path = plan
        .installer_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .ok_or("installer path missing")?;
    let expected = run_plan(plan.target, Path::new(path), platform)
        .map_err(|_| "installer plan is not supported here")?;
    if expected.program != plan.program || expected.args != plan.args {
        return Err("installer command does not match the downloaded file");
    }
    if expected.explanation_code != plan.explanation_code {
        return Err("installer plan code mismatch");
    }
    Ok(())
}

/// `true` when `exit_code` means success for this plan (msiexec's "reboot required" counts).
pub fn exit_code_is_success(plan_code: &str, exit_code: Option<i32>) -> bool {
    match exit_code {
        Some(0) => true,
        Some(MSIEXEC_REBOOT_REQUIRED) => plan_code == CODE_NODE_MSI,
        _ => false,
    }
}

/// The static Codex desktop client download for `platform` / `arch` from the preset.
pub fn codex_app_release(
    spec: &CodexAppSpec,
    platform: Platform,
    arch: &str,
) -> AppResult<InstallerRelease> {
    let (url, name) = match platform {
        Platform::Windows => {
            let url = match arch {
                "aarch64" | "arm64" => spec.windows_msix_arm64.trim(),
                _ => spec.windows_msix_x64.trim(),
            };
            (url, "ChatGPT (Codex).msix")
        }
        Platform::Macos => (spec.macos_dmg.trim(), "ChatGPT (Codex).dmg"),
        Platform::Linux | Platform::Unknown => {
            return Err(AppError::Unsupported(
                "the Codex desktop client ships for Windows and macOS only".into(),
            ))
        }
    };
    if url.is_empty() {
        return Err(AppError::Config(
            "codexApp download url is not configured for this platform".into(),
        ));
    }
    let file_name = url
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty() && !n.contains('?'))
        .unwrap_or(name)
        .to_owned();
    Ok(InstallerRelease {
        target: InstallTarget::CodexApp,
        version: "latest".to_owned(),
        asset_name: file_name,
        download_url: url.to_owned(),
        sha256: None,
        source: "static".to_owned(),
        requires_admin: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn node_msi_plan_is_passive_and_admin() {
        let plan = run_plan(
            InstallTarget::Node,
            &p(r"C:\cache\downloads\node-v24.19.0-x64.msi"),
            Platform::Windows,
        )
        .expect("plan");
        assert_eq!(plan.program, "msiexec.exe");
        assert_eq!(
            plan.args,
            vec![
                "/i",
                r"C:\cache\downloads\node-v24.19.0-x64.msi",
                "/passive",
                "/norestart"
            ]
        );
        assert!(plan.requires_admin);
        assert_eq!(plan.explanation_code, CODE_NODE_MSI);
        assert!(plan.display_command.contains("msiexec.exe /i"));
        assert!(validate_run_plan(&plan, Platform::Windows).is_ok());
        let mut tampered = plan.clone();
        tampered.args[1] = r"C:\evil.msi".into();
        assert!(validate_run_plan(&tampered, Platform::Windows).is_err());
        assert!(exit_code_is_success(CODE_NODE_MSI, Some(3010)));
        assert!(!exit_code_is_success(CODE_CODEX_APP_MSIX, Some(3010)));
        assert!(exit_code_is_success(CODE_NODE_PKG, Some(0)));
        assert!(!exit_code_is_success(CODE_NODE_MSI, None));
    }

    #[test]
    fn node_pkg_plan_uses_osascript_admin_prompt() {
        let plan = run_plan(
            InstallTarget::Node,
            &p("/Users/me/Library/Caches/app/downloads/node-v24.19.0.pkg"),
            Platform::Macos,
        )
        .expect("plan");
        assert_eq!(plan.program, "osascript");
        assert!(plan.args[1].contains("with administrator privileges"));
        assert!(plan.args[1].contains("'/Users/me/Library/Caches/app/downloads/node-v24.19.0.pkg'"));
        assert!(plan.requires_admin);
        assert_eq!(applescript_quote("/a b/it's.pkg"), "'/a b/it'\\\\''s.pkg'");
    }

    #[test]
    fn codex_app_plans() {
        let win = run_plan(
            InstallTarget::CodexApp,
            &p(r"C:\cache\downloads\ChatGPT-x64.msix"),
            Platform::Windows,
        )
        .expect("plan");
        assert_eq!(win.program, "powershell.exe");
        assert!(win.args.contains(&"Add-AppxPackage".to_owned()));
        assert!(!win.requires_admin);
        let mac = run_plan(
            InstallTarget::CodexApp,
            &p("/tmp/ChatGPT.dmg"),
            Platform::Macos,
        )
        .expect("plan");
        assert_eq!(mac.program, "/bin/sh");
        assert_eq!(mac.args[3], "/tmp/ChatGPT.dmg");
        assert!(validate_run_plan(&mac, Platform::Macos).is_ok());
        assert!(run_plan(InstallTarget::CodexApp, &p("/tmp/x.exe"), Platform::Macos).is_err());
        assert!(run_plan(InstallTarget::Codex, &p("/tmp/x.msi"), Platform::Windows).is_err());
    }

    #[test]
    fn codex_app_release_from_spec() {
        let spec = CodexAppSpec {
            store_product_id: "9PLM9XGG6VKS".into(),
            windows_msix_x64: "https://persistent.oaistatic.com/codex-app-prod/ChatGPT-x64.msix"
                .into(),
            windows_msix_arm64:
                "https://persistent.oaistatic.com/codex-app-prod/ChatGPT-arm64.msix".into(),
            macos_dmg: "https://persistent.oaistatic.com/codex-app-prod/ChatGPT.dmg".into(),
            download_page: String::new(),
        };
        let x64 = codex_app_release(&spec, Platform::Windows, "x86_64").expect("release");
        assert_eq!(x64.asset_name, "ChatGPT-x64.msix");
        assert_eq!(x64.target, InstallTarget::CodexApp);
        assert!(x64.sha256.is_none());
        let arm = codex_app_release(&spec, Platform::Windows, "aarch64").expect("release");
        assert!(arm.download_url.ends_with("arm64.msix"));
        let mac = codex_app_release(&spec, Platform::Macos, "aarch64").expect("release");
        assert_eq!(mac.asset_name, "ChatGPT.dmg");
        assert!(codex_app_release(&spec, Platform::Linux, "x86_64").is_err());
        assert!(codex_app_release(&CodexAppSpec::default(), Platform::Windows, "x86_64").is_err());
    }
}

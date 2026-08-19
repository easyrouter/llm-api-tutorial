//! CLI tool checks (Codex CLI, Claude Code) driven by the preset's `ToolSpec`s.
//!
//! [`detect_tool`] is the shared detector (also used by verify / diagnose): the binary is
//! looked up like a new terminal would (`checks::locate_binary`: process `PATH`, then the
//! fresh-session `PATH`), then in the npm global bin directory (installed but the terminal
//! has not picked the new `PATH` up yet — guide fault E). When found, it is executed with
//! `version_args` (12 s) to obtain the version.
//!
//! Codes: `tool.ok` (Pass) · `tool.not_on_path` (Fail, instructions + rerun) · `tool.broken`
//! (Fail, the binary exists but fails to run) · `tool.missing` (Fail, install).

use std::path::PathBuf;
use std::time::Duration;

use crate::models::{FixAction, InstallTarget, Params, ToolId, ToolInfo, ToolSpec};
use crate::platform;

use super::{locate_binary, run_version, tool_id_str, Verdict, VersionRun};

/// Timeout for `<binary> --version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(12);

/// Instructions key (fully qualified) shown when a tool is only in the npm global bin dir.
pub const TOOL_NOT_ON_PATH_INSTRUCTIONS: &str = "checks:tool.instructions.not_on_path";

/// Detection outcome: the wire-level [`ToolInfo`] plus the raw version run (when executed).
#[derive(Debug, Clone)]
pub struct Detection {
    pub info: ToolInfo,
    /// `None` when the binary was not found anywhere.
    pub run: Option<VersionRun>,
}

/// npm global bin directory (`npm prefix -g`, with a synchronous guess as fallback).
pub async fn npm_global_bin() -> Option<PathBuf> {
    platform::npm_global_prefix()
        .await
        .map(|prefix| platform::npm_global_bin_dir(&prefix))
}

/// Detects `spec` (see module docs). Runs `npm prefix -g` to locate the global bin dir.
pub async fn detect_tool(spec: &ToolSpec) -> ToolInfo {
    detect(spec, npm_global_bin().await).await.info
}

/// Detection with an already-known npm global bin directory.
pub async fn detect(spec: &ToolSpec, npm_global_bin: Option<PathBuf>) -> Detection {
    let extra_dirs: Vec<PathBuf> = npm_global_bin.iter().cloned().collect();
    let npm_global_bin_str = npm_global_bin.map(|p| p.to_string_lossy().into_owned());
    let Some((path, on_path)) = locate_binary(&spec.binary, &extra_dirs).await else {
        return Detection {
            info: ToolInfo {
                id: spec.id,
                installed: false,
                version: None,
                path: None,
                on_path: false,
                npm_global_bin: npm_global_bin_str,
            },
            run: None,
        };
    };
    // Run through the resolved path so a binary that is not on PATH still reports a version.
    let run = run_version(&path.to_string_lossy(), &spec.version_args, VERSION_TIMEOUT).await;
    Detection {
        info: ToolInfo {
            id: spec.id,
            installed: true,
            version: run.display_version(),
            path: Some(path.to_string_lossy().into_owned()),
            on_path,
            npm_global_bin: npm_global_bin_str,
        },
        run: Some(run),
    }
}

/// Runs the check for one tool: verdict + snapshot [`ToolInfo`].
pub async fn check(spec: &ToolSpec) -> (Verdict, ToolInfo) {
    let detection = detect(spec, npm_global_bin().await).await;
    let verdict = verdict_for(spec, &detection);
    (verdict, detection.info)
}

/// Pure mapping from a [`Detection`] to the check verdict.
pub fn verdict_for(spec: &ToolSpec, detection: &Detection) -> Verdict {
    let info = &detection.info;
    let base = |v: Verdict| {
        v.param("tool", spec.display_name.clone())
            .param("toolId", tool_id_str(spec.id))
            .param_opt("path", info.path.clone())
            .param_opt("version", info.version.clone())
    };
    let verdict = match (&detection.run, info.on_path) {
        (None, _) => Verdict::fail("tool.missing").fix(install_fix(spec.id)),
        (Some(_), false) => not_on_path(spec, info),
        (Some(VersionRun::Ok { raw, .. }), true) => Verdict::pass("tool.ok").detail(format!(
            "{} {}: {raw}",
            spec.binary,
            spec.version_args.join(" ")
        )),
        (
            Some(VersionRun::Failed {
                exit_code,
                timed_out,
                tail,
            }),
            true,
        ) => broken(spec, *exit_code, *timed_out, tail),
        // Found by `find_binary` but the spawn reported "not found" — treat as missing.
        (Some(VersionRun::NotFound), true) => {
            Verdict::fail("tool.missing").fix(install_fix(spec.id))
        }
    };
    base(verdict)
}

fn not_on_path(spec: &ToolSpec, info: &ToolInfo) -> Verdict {
    let dir = info
        .npm_global_bin
        .clone()
        .or_else(|| {
            info.path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).parent())
                .map(|d| d.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let mut params = Params::new();
    params.insert("tool".to_owned(), spec.display_name.clone());
    params.insert("dir".to_owned(), dir.clone());
    let mut v = Verdict::fail("tool.not_on_path")
        .param("dir", dir.clone())
        .detail(info.path.clone().unwrap_or_default());
    if !dir.is_empty() {
        v = v.fix(FixAction::RepairPath { dir });
    }
    v.fix(FixAction::Instructions {
        code: TOOL_NOT_ON_PATH_INSTRUCTIONS.to_owned(),
        params,
    })
    .fix(FixAction::Rerun)
}

fn broken(spec: &ToolSpec, exit_code: Option<i32>, timed_out: bool, tail: &str) -> Verdict {
    broken_with_node(spec, exit_code, timed_out, tail, node_dir_for_repair())
}

/// Like [`broken`], with the Node directory to offer when the failure is "node not found"
/// (injected so the decision is testable).
pub fn broken_with_node(
    spec: &ToolSpec,
    exit_code: Option<i32>,
    timed_out: bool,
    tail: &str,
    node_dir: Option<String>,
) -> Verdict {
    let mut v = Verdict::fail("tool.broken");
    if let Some(c) = exit_code {
        v = v.param("exitCode", c.to_string());
    }
    if timed_out {
        v = v.param("timedOut", "true");
    }
    for line in tail.lines().filter(|l| !l.trim().is_empty()) {
        v = v.detail(line.trim());
    }
    // The npm shim (`codex.cmd` / `codex` shell script) could not find `node`: the tool itself
    // is fine, Node is installed somewhere that is not on PATH — offer the PATH repair first.
    if tail_says_node_missing(tail) {
        v = v.param("nodeMissing", "true");
        if let Some(dir) = node_dir {
            v = v
                .param("nodeDir", dir.clone())
                .fix(FixAction::RepairPath { dir });
        }
    }
    v.fix(install_fix(spec.id)).fix(FixAction::Rerun)
}

/// `'"node"' is not recognized…` (cmd), `node: command not found` (sh), `node: not found`.
pub fn tail_says_node_missing(tail: &str) -> bool {
    let lower = tail.to_ascii_lowercase();
    (lower.contains("\"node\"") || lower.contains("'node'") || lower.contains("node:"))
        && (lower.contains("not recognized")
            || lower.contains("not found")
            || lower.contains("no such file"))
}

/// Directory of an installed Node that a fresh terminal cannot see (well-known locations),
/// `None` when Node is on PATH already or not installed at all.
fn node_dir_for_repair() -> Option<String> {
    if crate::platform::find_on_path("node").is_some() {
        return None;
    }
    super::node::existing_node_candidates()
        .into_iter()
        .find_map(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
}

/// Install fix for a tool id.
pub fn install_fix(id: ToolId) -> FixAction {
    FixAction::Install {
        tool: install_target(id),
    }
}

/// [`InstallTarget`] for a [`ToolId`].
pub fn install_target(id: ToolId) -> InstallTarget {
    match id {
        ToolId::Codex => InstallTarget::Codex,
        ToolId::ClaudeCode => InstallTarget::ClaudeCode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CheckStatus;

    fn spec(id: ToolId, binary: &str) -> ToolSpec {
        ToolSpec {
            id,
            display_name: match id {
                ToolId::Codex => "Codex CLI".into(),
                ToolId::ClaudeCode => "Claude Code".into(),
            },
            npm_package: "pkg".into(),
            binary: binary.into(),
            version_args: vec!["--version".into()],
            config_dir: "~/.x".into(),
        }
    }

    fn info(id: ToolId, installed: bool, on_path: bool) -> ToolInfo {
        ToolInfo {
            id,
            installed,
            version: installed.then(|| "0.5.1".to_owned()),
            path: installed.then(|| "/usr/local/bin/codex".to_owned()),
            on_path,
            npm_global_bin: Some("/usr/local/bin".to_owned()),
        }
    }

    #[test]
    fn verdict_table() {
        let ok_run = VersionRun::Ok {
            raw: "codex-cli 0.5.1".into(),
            version: super::super::parse_version("0.5.1"),
        };
        let failed_run = VersionRun::Failed {
            exit_code: Some(1),
            timed_out: false,
            tail: "SyntaxError: Unexpected token".into(),
        };
        let cases: Vec<(&str, Detection, CheckStatus, &str)> = vec![
            (
                "missing",
                Detection {
                    info: info(ToolId::Codex, false, false),
                    run: None,
                },
                CheckStatus::Fail,
                "tool.missing",
            ),
            (
                "ok",
                Detection {
                    info: info(ToolId::Codex, true, true),
                    run: Some(ok_run.clone()),
                },
                CheckStatus::Pass,
                "tool.ok",
            ),
            (
                "not on path",
                Detection {
                    info: info(ToolId::Codex, true, false),
                    run: Some(ok_run),
                },
                CheckStatus::Fail,
                "tool.not_on_path",
            ),
            (
                "broken",
                Detection {
                    info: info(ToolId::Codex, true, true),
                    run: Some(failed_run),
                },
                CheckStatus::Fail,
                "tool.broken",
            ),
            (
                "spawn not found",
                Detection {
                    info: info(ToolId::Codex, true, true),
                    run: Some(VersionRun::NotFound),
                },
                CheckStatus::Fail,
                "tool.missing",
            ),
        ];
        let s = spec(ToolId::Codex, "codex");
        for (name, detection, status, code) in cases {
            let v = verdict_for(&s, &detection);
            assert_eq!(v.status, status, "case {name}");
            assert_eq!(v.code, code, "case {name}");
            assert_eq!(v.params.get("tool").map(String::as_str), Some("Codex CLI"));
            assert_eq!(v.params.get("toolId").map(String::as_str), Some("codex"));
        }
    }

    #[test]
    fn missing_offers_install_and_not_on_path_offers_instructions() {
        let s = spec(ToolId::ClaudeCode, "claude");
        let missing = verdict_for(
            &s,
            &Detection {
                info: info(ToolId::ClaudeCode, false, false),
                run: None,
            },
        );
        assert_eq!(
            missing.fixes,
            vec![FixAction::Install {
                tool: InstallTarget::ClaudeCode
            }]
        );
        let not_on_path = verdict_for(
            &s,
            &Detection {
                info: info(ToolId::ClaudeCode, true, false),
                run: Some(VersionRun::Ok {
                    raw: "1.0.0 (Claude Code)".into(),
                    version: None,
                }),
            },
        );
        assert_eq!(
            not_on_path.params.get("dir").map(String::as_str),
            Some("/usr/local/bin")
        );
        assert_eq!(
            not_on_path.fixes[0],
            FixAction::RepairPath {
                dir: "/usr/local/bin".into()
            }
        );
        assert!(matches!(
            &not_on_path.fixes[1],
            FixAction::Instructions { code, params }
                if code == TOOL_NOT_ON_PATH_INSTRUCTIONS
                    && params.get("dir").map(String::as_str) == Some("/usr/local/bin")
                    && params.get("tool").map(String::as_str) == Some("Claude Code")
        ));
        assert_eq!(not_on_path.fixes[2], FixAction::Rerun);
    }

    #[test]
    fn broken_shim_missing_node_offers_path_repair_first() {
        let spec = spec(ToolId::Codex, "codex");
        let tail = "'\"node\"' is not recognized as an internal or external command,
operable program or batch file.";
        let v = broken_with_node(
            &spec,
            Some(1),
            false,
            tail,
            Some(
                r"C:\Program Files
odejs"
                    .into(),
            ),
        );
        assert_eq!(v.code, "tool.broken");
        assert_eq!(
            v.params.get("nodeMissing").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            v.fixes[0],
            FixAction::RepairPath {
                dir: r"C:\Program Files
odejs"
                    .into()
            }
        );
        assert!(matches!(v.fixes[1], FixAction::Install { .. }));
        // Node missing but not installed anywhere: no repair offered, install stays first.
        let v = broken_with_node(&spec, Some(1), false, tail, None);
        assert!(matches!(v.fixes[0], FixAction::Install { .. }));
        // Unrelated failure: no nodeMissing flag.
        let v = broken_with_node(&spec, Some(2), false, "segfault", Some("/x".into()));
        assert!(!v.params.contains_key("nodeMissing"));
        assert!(tail_says_node_missing("sh: node: command not found"));
        assert!(tail_says_node_missing(
            "env: node: No such file or directory"
        ));
        assert!(!tail_says_node_missing("TypeError: x is not a function"));
    }

    #[test]
    fn broken_carries_exit_code_and_redacted_tail() {
        let s = spec(ToolId::Codex, "codex");
        let v = verdict_for(
            &s,
            &Detection {
                info: info(ToolId::Codex, true, true),
                run: Some(VersionRun::Failed {
                    exit_code: Some(2),
                    timed_out: true,
                    tail: "line one\n\nline two".into(),
                }),
            },
        );
        assert_eq!(v.params.get("exitCode").map(String::as_str), Some("2"));
        assert_eq!(v.params.get("timedOut").map(String::as_str), Some("true"));
        assert_eq!(v.details, vec!["line one", "line two"]);
        assert!(v.fixes.contains(&FixAction::Install {
            tool: InstallTarget::Codex
        }));
    }

    #[test]
    fn install_targets_map_one_to_one() {
        assert_eq!(install_target(ToolId::Codex), InstallTarget::Codex);
        assert_eq!(
            install_target(ToolId::ClaudeCode),
            InstallTarget::ClaudeCode
        );
    }

    #[tokio::test]
    async fn detect_unknown_binary_is_not_installed() {
        let s = spec(ToolId::Codex, "seedrouter-onboarding-no-such-tool-42");
        let d = detect(&s, Some(PathBuf::from("/nonexistent/dir"))).await;
        assert!(!d.info.installed);
        assert!(!d.info.on_path);
        assert!(d.run.is_none());
        assert_eq!(d.info.npm_global_bin.as_deref(), Some("/nonexistent/dir"));
        let (v, info) = check(&s).await;
        assert_eq!(v.code, "tool.missing");
        assert!(!info.installed);
    }

    #[tokio::test]
    async fn detect_binary_only_in_extra_dir_reports_not_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let name = "seedrouter-onboarding-fake-tool";
        let file_name = platform::candidate_names_for(platform::platform(), name)
            .into_iter()
            .next()
            .expect("candidate");
        std::fs::write(dir.path().join(&file_name), b"").expect("write");
        let s = spec(ToolId::Codex, name);
        let d = detect(&s, Some(dir.path().to_path_buf())).await;
        assert!(d.info.installed);
        assert!(!d.info.on_path);
        assert!(d.run.is_some());
        let v = verdict_for(&s, &d);
        assert_eq!(v.code, "tool.not_on_path");
        assert_eq!(
            v.params.get("dir").map(String::as_str),
            Some(dir.path().to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn detect_tool_public_api_runs() {
        let s = spec(ToolId::Codex, "seedrouter-onboarding-no-such-tool-42");
        let info = detect_tool(&s).await;
        assert!(!info.installed);
        assert_eq!(info.id, ToolId::Codex);
    }
}

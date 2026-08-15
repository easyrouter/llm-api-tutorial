//! Node.js / npm checks.
//!
//! Node is located like a new terminal would find it (`checks::locate_binary`: process
//! `PATH`, then fresh-session `PATH`), then `node --version` (8 s) → semver compared to
//! `Requirements.node_min_version`:
//! - `node.ok` (Pass) / `node.too_old` (Fail — Codex needs >= 18) / `node.broken` (Fail —
//!   the binary exists but fails to run);
//! - not on `PATH` but present in a well-known install directory → `node.not_on_path` (Fail,
//!   the terminal simply has not picked it up yet — instructions + rerun);
//! - not found anywhere → `node.missing` (Fail, install + download page).
//!
//! `npm --version` (8 s) → `npm.ok` / `npm.missing` / `npm.broken` with the same fixes as
//! Node (npm ships with Node).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::{FixAction, InstallTarget, Mirrors, Params, Platform, Requirements};
use crate::platform;

use super::{locate_binary, parse_version, run_version, Verdict, VersionRun};

/// Timeout for `node --version` / `npm --version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(8);

/// Node download page used when the mirror list has none.
const DEFAULT_NODE_DOWNLOAD_PAGE: &str = "https://nodejs.org/en/download";

/// Instructions key (fully qualified) shown when Node is installed but not on `PATH`.
pub const NODE_NOT_ON_PATH_INSTRUCTIONS: &str = "checks:node.instructions.not_on_path";

/// Label code of the Node download `OpenUrl` fix.
pub const NODE_DOWNLOAD_LABEL: &str = "node_download";

/// Runs the Node.js check. Node is looked up like a new terminal would (`locate_binary`)
/// and executed through its resolved path.
pub async fn check_node(req: &Requirements, mirrors: &Mirrors) -> Verdict {
    match locate_binary("node", &[]).await {
        Some((path, _)) => {
            let run = version_of(&path).await;
            verdict_for_run(&run, req, mirrors).param("path", path_str(&path))
        }
        None => match find_installed_node().await {
            Some((path, run)) => not_on_path(&path, &run),
            None => Verdict::fail("node.missing").fixes(install_fixes(mirrors)),
        },
    }
}

/// Runs the npm check (same lookup rules as Node).
pub async fn check_npm(mirrors: &Mirrors) -> Verdict {
    let Some((path, _)) = locate_binary("npm", &[]).await else {
        return Verdict::fail("npm.missing").fixes(install_fixes(mirrors));
    };
    let verdict = match version_of(&path).await {
        VersionRun::Ok { raw, version } => {
            Verdict::pass("npm.ok").param("version", version.map_or(raw, |v| v.to_string()))
        }
        VersionRun::NotFound => Verdict::fail("npm.missing").fixes(install_fixes(mirrors)),
        VersionRun::Failed {
            exit_code,
            timed_out,
            tail,
        } => broken("npm.broken", exit_code, timed_out, &tail).fixes(install_fixes(mirrors)),
    };
    verdict.param("path", path_str(&path))
}

/// `<path> --version` with the Node/npm timeout.
async fn version_of(path: &Path) -> VersionRun {
    let args = vec!["--version".to_owned()];
    run_version(&path.to_string_lossy(), &args, VERSION_TIMEOUT).await
}

/// Verdict for a Node that is on `PATH` and was executed.
fn verdict_for_run(run: &VersionRun, req: &Requirements, mirrors: &Mirrors) -> Verdict {
    match run {
        VersionRun::Ok { raw, version } => {
            let display = version
                .as_ref()
                .map_or_else(|| raw.clone(), ToString::to_string);
            let verdict = match (version, parse_version(&req.node_min_version)) {
                (Some(actual), Some(min)) if *actual < min => {
                    Verdict::fail("node.too_old").fixes(install_fixes(mirrors))
                }
                (None, _) => {
                    log::warn!("node --version output not understood: {raw}");
                    Verdict::pass("node.ok")
                }
                (Some(_), None) => {
                    log::warn!(
                        "nodeMinVersion {:?} is not a version; skipping comparison",
                        req.node_min_version
                    );
                    Verdict::pass("node.ok")
                }
                (Some(_), Some(_)) => Verdict::pass("node.ok"),
            };
            verdict
                .param("version", display)
                .param("minVersion", req.node_min_version.clone())
                .param("recommended", req.node_recommended_lts.clone())
                .detail(format!("node --version: {raw}"))
        }
        VersionRun::Failed {
            exit_code,
            timed_out,
            tail,
        } => broken("node.broken", *exit_code, *timed_out, tail).fixes(install_fixes(mirrors)),
        VersionRun::NotFound => Verdict::fail("node.missing").fixes(install_fixes(mirrors)),
    }
}

/// Fail verdict for a binary that exists but does not run.
fn broken(code: &str, exit_code: Option<i32>, timed_out: bool, tail: &str) -> Verdict {
    let mut v = Verdict::fail(code);
    if let Some(c) = exit_code {
        v = v.param("exitCode", c.to_string());
    }
    if timed_out {
        v = v.param("timedOut", "true");
    }
    for line in tail.lines().filter(|l| !l.trim().is_empty()) {
        v = v.detail(line.trim());
    }
    v.fix(FixAction::Rerun)
}

/// `node.not_on_path` verdict for a Node found at `path` (version from running it directly).
/// Params / instruction params: `path` = executable, `dir` = its directory (to add to PATH).
fn not_on_path(path: &Path, run: &VersionRun) -> Verdict {
    let full = path.to_string_lossy().into_owned();
    let dir = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut params = Params::new();
    params.insert("path".to_owned(), full.clone());
    params.insert("dir".to_owned(), dir.clone());
    Verdict::fail("node.not_on_path")
        .param("path", full.clone())
        .param("dir", dir)
        .param_opt("version", run.display_version())
        .detail(full)
        .fix(FixAction::Instructions {
            code: NODE_NOT_ON_PATH_INSTRUCTIONS.to_owned(),
            params,
        })
        .fix(FixAction::Rerun)
}

/// Fixes offered when Node/npm are missing or too old: install + official download page.
pub fn install_fixes(mirrors: &Mirrors) -> Vec<FixAction> {
    vec![
        FixAction::Install {
            tool: InstallTarget::Node,
        },
        FixAction::OpenUrl {
            url: node_download_page(mirrors),
            label_code: NODE_DOWNLOAD_LABEL.to_owned(),
        },
    ]
}

/// Download page of the `official` Node dist mirror, else the first mirror with a page,
/// else the nodejs.org default.
pub fn node_download_page(mirrors: &Mirrors) -> String {
    let with_page = |id: Option<&str>| {
        mirrors
            .node_dist
            .iter()
            .filter(|m| id.is_none_or(|id| m.id == id))
            .map(|m| m.download_page.trim())
            .find(|p| !p.is_empty())
            .map(str::to_owned)
    };
    with_page(Some("official"))
        .or_else(|| with_page(None))
        .unwrap_or_else(|| DEFAULT_NODE_DOWNLOAD_PAGE.to_owned())
}

/// Well-known Node install locations for `platform` (pure; env/home injected for tests).
/// The `~/.nvm/versions/node/*/bin/node` glob is expanded by [`existing_node_candidates`].
pub fn node_candidates_for<F>(platform: Platform, home: Option<&Path>, env: F) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    match platform {
        Platform::Windows => ["ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|k| env(k))
            .map(|dir| PathBuf::from(dir).join("nodejs").join("node.exe"))
            .chain(env("LOCALAPPDATA").map(|dir| {
                PathBuf::from(dir)
                    .join("Programs")
                    .join("nodejs")
                    .join("node.exe")
            }))
            .collect(),
        Platform::Macos | Platform::Linux | Platform::Unknown => {
            let mut list = vec![
                PathBuf::from("/usr/local/bin/node"),
                PathBuf::from("/opt/homebrew/bin/node"),
            ];
            if let Some(home) = home {
                list.push(home.join(".nvm").join("versions").join("node"));
            }
            list
        }
    }
}

/// Existing Node executables from the well-known locations (nvm version dirs expanded,
/// newest version first).
pub fn existing_node_candidates() -> Vec<PathBuf> {
    let home = platform::home_dir();
    let candidates = node_candidates_for(platform::platform(), home.as_deref(), |k| {
        std::env::var(k).ok()
    });
    let mut found = Vec::new();
    for candidate in candidates {
        if candidate.is_file() {
            found.push(candidate);
        } else if candidate.is_dir() {
            found.extend(nvm_versions(&candidate));
        }
    }
    found
}

/// `<dir>/*/bin/node` for an nvm `versions/node` directory, newest version first.
fn nvm_versions(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut versions: Vec<(Option<semver::Version>, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|p| {
            let node = p.join("bin").join("node");
            node.is_file().then(|| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                (name.as_deref().and_then(parse_version), node)
            })
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, p)| p).collect()
}

/// First installed-but-not-on-PATH Node together with the outcome of running it directly.
async fn find_installed_node() -> Option<(PathBuf, VersionRun)> {
    let path = existing_node_candidates().into_iter().next()?;
    let run = version_of(&path).await;
    Some((path, run))
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CheckStatus, MacosRequirement, MirrorEntry, OsRequirements, WindowsRequirement,
    };

    fn mirrors() -> Mirrors {
        Mirrors {
            npm_registries: vec![],
            node_dist: vec![
                MirrorEntry {
                    id: "official".into(),
                    url: "https://nodejs.org/dist/".into(),
                    download_page: "https://nodejs.org/en/download".into(),
                },
                MirrorEntry {
                    id: "npmmirror".into(),
                    url: "https://npmmirror.com/mirrors/node/".into(),
                    download_page: "https://npmmirror.com/mirrors/node/".into(),
                },
            ],
            probe_timeout_ms: 1000,
        }
    }

    fn requirements(min: &str) -> Requirements {
        Requirements {
            node_min_version: min.into(),
            node_recommended_lts: "22".into(),
            os: OsRequirements {
                windows: WindowsRequirement {
                    min_build: 19041,
                    label: String::new(),
                },
                macos: MacosRequirement {
                    min_version: "12.0".into(),
                    label: String::new(),
                },
            },
        }
    }

    fn ok(raw: &str) -> VersionRun {
        VersionRun::Ok {
            raw: raw.into(),
            version: parse_version(raw),
        }
    }

    #[test]
    fn node_version_comparison_table() {
        let cases: &[(&str, &str, CheckStatus, &str)] = &[
            ("v22.1.0", "18.0.0", CheckStatus::Pass, "node.ok"),
            ("v18.0.0", "18.0.0", CheckStatus::Pass, "node.ok"),
            ("v16.20.2", "18.0.0", CheckStatus::Fail, "node.too_old"),
            ("v17.9.9", "18.0.0", CheckStatus::Fail, "node.too_old"),
            ("v20.0.0", "not-a-version", CheckStatus::Pass, "node.ok"),
            ("weird output", "18.0.0", CheckStatus::Pass, "node.ok"),
        ];
        for (raw, min, status, code) in cases {
            let v = verdict_for_run(&ok(raw), &requirements(min), &mirrors());
            assert_eq!(v.status, *status, "raw {raw} min {min}");
            assert_eq!(v.code, *code, "raw {raw} min {min}");
            assert_eq!(v.params.get("minVersion").map(String::as_str), Some(*min));
            assert_eq!(v.params.get("recommended").map(String::as_str), Some("22"));
        }
        let too_old = verdict_for_run(&ok("v16.20.2"), &requirements("18.0.0"), &mirrors());
        assert_eq!(
            too_old.params.get("version").map(String::as_str),
            Some("16.20.2")
        );
        assert!(too_old.fixes.contains(&FixAction::Install {
            tool: InstallTarget::Node
        }));
        assert!(too_old.fixes.iter().any(
            |f| matches!(f, FixAction::OpenUrl { label_code, .. } if label_code == "node_download")
        ));
    }

    #[test]
    fn broken_and_missing_runs() {
        let failed = VersionRun::Failed {
            exit_code: Some(3),
            timed_out: false,
            tail: "boom sk-abcdefghijklmnop".into(),
        };
        let v = verdict_for_run(&failed, &requirements("18.0.0"), &mirrors());
        assert_eq!(v.code, "node.broken");
        assert_eq!(v.status, CheckStatus::Fail);
        assert_eq!(v.params.get("exitCode").map(String::as_str), Some("3"));
        assert!(v.details.iter().any(|d| d.contains("boom")));
        assert!(v.fixes.contains(&FixAction::Rerun));

        let missing = verdict_for_run(&VersionRun::NotFound, &requirements("18.0.0"), &mirrors());
        assert_eq!(missing.code, "node.missing");
    }

    #[test]
    fn not_on_path_verdict_carries_instructions() {
        let path = Path::new("/usr/local/bin/node");
        let v = not_on_path(path, &ok("v20.1.0"));
        assert_eq!(v.code, "node.not_on_path");
        assert_eq!(v.status, CheckStatus::Fail);
        assert_eq!(v.params.get("version").map(String::as_str), Some("20.1.0"));
        assert_eq!(
            v.params.get("dir").map(String::as_str),
            Some("/usr/local/bin")
        );
        assert!(matches!(
            &v.fixes[0],
            FixAction::Instructions { code, params }
                if code == NODE_NOT_ON_PATH_INSTRUCTIONS
                    && params.get("dir").map(String::as_str) == Some("/usr/local/bin")
                    && params.get("path").map(String::as_str) == Some("/usr/local/bin/node")
        ));
        assert_eq!(v.fixes[1], FixAction::Rerun);
    }

    #[test]
    fn download_page_prefers_official() {
        assert_eq!(
            node_download_page(&mirrors()),
            "https://nodejs.org/en/download"
        );
        let mut m = mirrors();
        m.node_dist[0].download_page.clear();
        assert_eq!(
            node_download_page(&m),
            "https://npmmirror.com/mirrors/node/"
        );
        m.node_dist.clear();
        assert_eq!(node_download_page(&m), DEFAULT_NODE_DOWNLOAD_PAGE);
    }

    #[test]
    fn candidate_locations_per_platform() {
        let env = |k: &str| match k {
            "ProgramFiles" => Some(r"C:\Program Files".to_owned()),
            "LOCALAPPDATA" => Some(r"C:\Users\me\AppData\Local".to_owned()),
            _ => None,
        };
        // Compare separator-agnostically: `PathBuf::join` uses `/` on Unix hosts, so the
        // literal Windows spelling only matches when the test itself runs on Windows.
        let win: Vec<String> = node_candidates_for(Platform::Windows, None, env)
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            win,
            vec![
                "C:/Program Files/nodejs/node.exe".to_owned(),
                "C:/Users/me/AppData/Local/Programs/nodejs/node.exe".to_owned(),
            ]
        );
        let mac = node_candidates_for(Platform::Macos, Some(Path::new("/Users/me")), |_| None);
        assert!(mac.contains(&PathBuf::from("/opt/homebrew/bin/node")));
        assert!(mac.contains(&PathBuf::from("/Users/me/.nvm/versions/node")));
    }

    #[test]
    fn nvm_versions_are_sorted_newest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        for v in ["v18.20.0", "v22.1.0", "v20.11.1", "not-a-version"] {
            let bin = dir.path().join(v).join("bin");
            std::fs::create_dir_all(&bin).expect("mkdir");
            std::fs::write(bin.join("node"), b"").expect("write");
        }
        std::fs::create_dir_all(dir.path().join("v99.0.0")).expect("empty version dir");
        let found = nvm_versions(dir.path());
        let names: Vec<String> = found
            .iter()
            .filter_map(|p| p.parent()?.parent()?.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["v22.1.0", "v20.11.1", "v18.20.0", "not-a-version"]
        );
    }

    #[tokio::test]
    async fn node_check_on_this_machine() {
        let v = check_node(&requirements("0.1.0"), &mirrors()).await;
        if platform::find_on_path("node").is_some() {
            assert_eq!(v.code, "node.ok", "{v:?}");
            assert!(v.params.contains_key("path"));
        } else {
            assert!(
                v.code == "node.missing" || v.code == "node.not_on_path",
                "{v:?}"
            );
        }
    }

    #[tokio::test]
    async fn npm_check_on_this_machine() {
        let v = check_npm(&mirrors()).await;
        if platform::find_on_path("npm").is_some() {
            assert_eq!(v.code, "npm.ok", "{v:?}");
            assert!(v.params.contains_key("version"));
        } else {
            assert_eq!(v.code, "npm.missing", "{v:?}");
        }
    }
}

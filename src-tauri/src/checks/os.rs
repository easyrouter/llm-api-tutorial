//! OS check: platform + minimum version from `Requirements.os`.
//!
//! - Windows: `OsInfo.build` (registry `CurrentBuildNumber`) must be `>= min_build`.
//! - macOS: `OsInfo.version` (`major.minor[.patch]`) must be `>= min_version` (semver order).
//! - Anything that cannot be determined (missing build number, unparsable version, an
//!   unsupported platform) is `os.unknown` with status Warn — the wizard continues, the user
//!   sees what was detected.

use crate::models::{OsInfo, OsRequirements, Platform};

use super::{parse_version, Verdict};

/// Runs the OS check against the current machine.
pub fn check(req: &OsRequirements) -> Verdict {
    evaluate(&crate::platform::os_info(), req)
}

/// Pure comparison of an [`OsInfo`] against the requirements (see module docs).
pub fn evaluate(info: &OsInfo, req: &OsRequirements) -> Verdict {
    let verdict = match info.platform {
        Platform::Windows => evaluate_windows(info, req),
        Platform::Macos => evaluate_macos(info, req),
        Platform::Linux | Platform::Unknown => Verdict::warn("os.unknown"),
    };
    with_facts(verdict, info)
}

fn evaluate_windows(info: &OsInfo, req: &OsRequirements) -> Verdict {
    let min_build = u64::from(req.windows.min_build);
    let verdict = match info.build {
        Some(build) if build >= min_build => Verdict::pass("os.ok"),
        Some(_) => Verdict::fail("os.too_old"),
        None => Verdict::warn("os.unknown"),
    };
    verdict
        .param("minBuild", min_build.to_string())
        .param("label", req.windows.label.clone())
}

fn evaluate_macos(info: &OsInfo, req: &OsRequirements) -> Verdict {
    let verdict = match (
        parse_version(&info.version),
        parse_version(&req.macos.min_version),
    ) {
        (Some(actual), Some(min)) if actual >= min => Verdict::pass("os.ok"),
        (Some(_), Some(_)) => Verdict::fail("os.too_old"),
        // Unparsable requirement = misconfigured preset: do not block the user.
        (Some(_), None) => Verdict::pass("os.ok"),
        (None, _) => Verdict::warn("os.unknown"),
    };
    verdict
        .param("minVersion", req.macos.min_version.clone())
        .param("label", req.macos.label.clone())
}

/// Adds the detected facts as params and detail lines.
fn with_facts(verdict: Verdict, info: &OsInfo) -> Verdict {
    let platform = platform_name(info.platform);
    let build = info.build.map(|b| b.to_string());
    let summary = match &build {
        Some(b) => format!("{platform} {} (build {b})", info.version),
        None => format!("{platform} {}", info.version),
    };
    verdict
        .param("platform", platform)
        .param("version", info.version.clone())
        .param_opt("build", build)
        .param("arch", info.arch.clone())
        .param("shell", info.shell.clone())
        .detail(summary)
        .detail(format!("arch: {}", info.arch))
        .detail(format!("shell: {}", info.shell))
}

/// Wire name of a platform (`windows`, `macos`, `linux`, `unknown`).
pub fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::Macos => "macos",
        Platform::Linux => "linux",
        Platform::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CheckStatus, MacosRequirement, WindowsRequirement};

    fn requirements() -> OsRequirements {
        OsRequirements {
            windows: WindowsRequirement {
                min_build: 19041,
                label: "Windows 10 2004+".into(),
            },
            macos: MacosRequirement {
                min_version: "12.0".into(),
                label: "macOS 12 Monterey+".into(),
            },
        }
    }

    fn info(platform: Platform, version: &str, build: Option<u64>) -> OsInfo {
        OsInfo {
            platform,
            version: version.into(),
            build,
            arch: "x86_64".into(),
            shell: "zsh".into(),
            home_dir: "/Users/me".into(),
            is_admin: Some(false),
        }
    }

    #[test]
    fn os_requirement_table() {
        let cases: &[(&str, OsInfo, CheckStatus, &str)] = &[
            (
                "win build above min",
                info(Platform::Windows, "10.0.19045", Some(19045)),
                CheckStatus::Pass,
                "os.ok",
            ),
            (
                "win build equal min",
                info(Platform::Windows, "10.0.19041", Some(19041)),
                CheckStatus::Pass,
                "os.ok",
            ),
            (
                "win build below min",
                info(Platform::Windows, "10.0.18363", Some(18363)),
                CheckStatus::Fail,
                "os.too_old",
            ),
            (
                "win 11",
                info(Platform::Windows, "10.0.22631", Some(22631)),
                CheckStatus::Pass,
                "os.ok",
            ),
            (
                "win build unknown",
                info(Platform::Windows, "10.0", None),
                CheckStatus::Warn,
                "os.unknown",
            ),
            (
                "mac newer",
                info(Platform::Macos, "14.5.0", None),
                CheckStatus::Pass,
                "os.ok",
            ),
            (
                "mac equal",
                info(Platform::Macos, "12.0", None),
                CheckStatus::Pass,
                "os.ok",
            ),
            (
                "mac older",
                info(Platform::Macos, "11.7.10", None),
                CheckStatus::Fail,
                "os.too_old",
            ),
            (
                "mac unparsable",
                info(Platform::Macos, "Unknown", None),
                CheckStatus::Warn,
                "os.unknown",
            ),
            (
                "linux unsupported",
                info(Platform::Linux, "22.04", None),
                CheckStatus::Warn,
                "os.unknown",
            ),
        ];
        let req = requirements();
        for (name, info, status, code) in cases {
            let v = evaluate(info, &req);
            assert_eq!(v.status, *status, "case {name}");
            assert_eq!(v.code, *code, "case {name}");
        }
    }

    #[test]
    fn facts_are_exposed_as_params_and_details() {
        let v = evaluate(
            &info(Platform::Windows, "10.0.19045", Some(19045)),
            &requirements(),
        );
        assert_eq!(
            v.params.get("platform").map(String::as_str),
            Some("windows")
        );
        assert_eq!(v.params.get("build").map(String::as_str), Some("19045"));
        assert_eq!(v.params.get("minBuild").map(String::as_str), Some("19041"));
        assert_eq!(
            v.params.get("label").map(String::as_str),
            Some("Windows 10 2004+")
        );
        assert_eq!(v.details[0], "windows 10.0.19045 (build 19045)");
        assert!(v.details.iter().any(|d| d == "arch: x86_64"));
        assert!(v.details.iter().any(|d| d == "shell: zsh"));

        let mac = evaluate(&info(Platform::Macos, "14.5", None), &requirements());
        assert!(!mac.params.contains_key("build"));
        assert_eq!(
            mac.params.get("minVersion").map(String::as_str),
            Some("12.0")
        );
        assert_eq!(mac.details[0], "macos 14.5");
    }

    #[test]
    fn unparsable_macos_requirement_does_not_block() {
        let mut req = requirements();
        req.macos.min_version = "latest".into();
        let v = evaluate(&info(Platform::Macos, "13.1", None), &req);
        assert_eq!(v.status, CheckStatus::Pass);
    }

    #[test]
    fn check_on_this_machine_yields_a_known_code() {
        let v = check(&requirements());
        assert!(v.code.starts_with("os."), "{v:?}");
        if cfg!(windows) {
            assert_eq!(
                v.params.get("platform").map(String::as_str),
                Some("windows")
            );
            assert!(v.params.contains_key("build"));
        }
    }
}

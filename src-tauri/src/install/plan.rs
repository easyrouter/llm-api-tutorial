//! Pure `InstallPlan` builders. Everything that touches the machine (finding `npm`, probing
//! the npm prefix, detecting Homebrew) is gathered into a [`PlanContext`] by the caller so the
//! decision logic here can be unit-tested on any host.
//!
//! Explanation codes (resolved by the UI as `install:plan.<code>`):
//!
//! | target              | code                 | command                                        |
//! | ------------------- | -------------------- | ---------------------------------------------- |
//! | Node (default)      | `node.download_page` | none — `download_url` = mirror download page   |
//! | Node (macOS + brew) | `node.homebrew`      | `brew install node` (linked into `PATH`)       |
//! | Codex / Claude Code | `npm_global`         | `npm install -g <pkg> --registry <mirror url>` |
//! | CC Switch           | `cc_switch.download` | none — release fetched via `cc_switch`         |
//!
//! [`validate_plan`] is the server-side counterpart of "show before run": `start_install`
//! only executes a plan that this module could have produced for the current config (program
//! basename, exact argument shape, package from the preset, registry from the preset, no
//! environment overrides), so a tampered DTO from the webview cannot run anything else.

use std::collections::BTreeMap;

use crate::error::{AppError, AppResult};
use crate::models::{AppConfig, InstallPlan, InstallTarget, MirrorChoice, Platform, ToolId};
use crate::process::CommandSpec;

/// Explanation code for the "open the Node.js download page" plan.
pub const CODE_NODE_DOWNLOAD_PAGE: &str = "node.download_page";
/// Explanation code for the macOS Homebrew Node.js plan.
pub const CODE_NODE_HOMEBREW: &str = "node.homebrew";
/// Explanation code for `npm install -g` plans.
pub const CODE_NPM_GLOBAL: &str = "npm_global";
/// Explanation code for the CC Switch release download plan.
pub const CODE_CC_SWITCH_DOWNLOAD: &str = "cc_switch.download";

/// Machine facts needed to build a plan (collected once by [`super::plan`]).
#[derive(Debug, Clone)]
pub struct PlanContext {
    pub platform: Platform,
    /// Resolved `brew` executable when Homebrew is usable (macOS only), else `None`. A full
    /// path is used because GUI apps start with a minimal `PATH` that rarely contains it.
    pub homebrew: Option<String>,
    /// Resolved `npm` shim (full path) or the bare name `npm` when it was not found.
    pub npm_program: String,
    /// The npm global prefix accepts writes from the current user (no elevation needed).
    pub npm_prefix_writable: bool,
}

/// Builds the plan for `target`. Pure: never touches the machine.
pub fn build_plan(
    target: InstallTarget,
    config: &AppConfig,
    mirrors: &MirrorChoice,
    ctx: &PlanContext,
) -> AppResult<InstallPlan> {
    match target {
        InstallTarget::Node => Ok(node_plan(config, mirrors, ctx)),
        InstallTarget::Codex => npm_plan(target, ToolId::Codex, config, mirrors, ctx),
        InstallTarget::ClaudeCode => npm_plan(target, ToolId::ClaudeCode, config, mirrors, ctx),
        InstallTarget::CcSwitch => Ok(cc_switch_plan()),
    }
}

/// A plan without a command: the UI shows instructions / a download link instead.
fn manual_plan(target: InstallTarget, code: &str, download_url: Option<String>) -> InstallPlan {
    InstallPlan {
        target,
        program: String::new(),
        args: Vec::new(),
        env: BTreeMap::new(),
        display_command: String::new(),
        registry: None,
        requires_admin: false,
        explanation_code: code.to_owned(),
        download_url,
    }
}

/// Homebrew formula proposed for Node.js. The unversioned `node` formula is used on purpose:
/// versioned formulae (`node@22`) are keg-only — their binaries are not linked into `PATH`, so a
/// fresh terminal (and the re-check) would still report "node missing" until the user runs
/// `brew link --force --overwrite node@22`. Any current `node` satisfies the minimum version.
pub const HOMEBREW_NODE_FORMULA: &str = "node";

/// Node.js: manual download from the chosen mirror; on macOS with Homebrew the plan proposes
/// `brew install node` and still carries the download page as an alternative.
fn node_plan(_config: &AppConfig, mirrors: &MirrorChoice, ctx: &PlanContext) -> InstallPlan {
    let download_url = node_download_url(mirrors);
    let brew = ctx
        .homebrew
        .as_deref()
        .filter(|_| ctx.platform == Platform::Macos);
    if let Some(brew) = brew {
        let spec = CommandSpec::new(brew, ["install", HOMEBREW_NODE_FORMULA]);
        return InstallPlan {
            target: InstallTarget::Node,
            display_command: spec.display(),
            program: spec.program,
            args: spec.args,
            env: BTreeMap::new(),
            registry: None,
            requires_admin: false,
            explanation_code: CODE_NODE_HOMEBREW.to_owned(),
            download_url,
        };
    }
    manual_plan(InstallTarget::Node, CODE_NODE_DOWNLOAD_PAGE, download_url)
}

/// Download page of the chosen Node.js mirror; falls back to the dist URL when the entry has
/// no dedicated page. `None` only when both are empty (misconfiguration).
fn node_download_url(mirrors: &MirrorChoice) -> Option<String> {
    [&mirrors.node_dist.download_page, &mirrors.node_dist.url]
        .into_iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map(str::to_owned)
}

/// `npm install -g <package> --registry <chosen registry>` for a CLI tool.
fn npm_plan(
    target: InstallTarget,
    tool: ToolId,
    config: &AppConfig,
    mirrors: &MirrorChoice,
    ctx: &PlanContext,
) -> AppResult<InstallPlan> {
    let spec = config
        .tools
        .iter()
        .find(|t| t.id == tool)
        .ok_or_else(|| AppError::Config(format!("tool {tool:?} is not configured")))?;
    let registry = mirrors.npm_registry.clone();
    let command = CommandSpec::new(
        ctx.npm_program.clone(),
        [
            "install",
            "-g",
            spec.npm_package.as_str(),
            "--registry",
            registry.url.as_str(),
        ],
    );
    Ok(InstallPlan {
        target,
        display_command: command.display(),
        program: command.program,
        args: command.args,
        env: BTreeMap::new(),
        registry: Some(registry),
        requires_admin: !ctx.npm_prefix_writable,
        explanation_code: CODE_NPM_GLOBAL.to_owned(),
        download_url: None,
    })
}

/// CC Switch is a desktop app: no command, the UI fetches the release and downloads it.
fn cc_switch_plan() -> InstallPlan {
    manual_plan(InstallTarget::CcSwitch, CODE_CC_SWITCH_DOWNLOAD, None)
}

// ---------------------------------------------------------------------------
// Server-side plan validation (start_install)
// ---------------------------------------------------------------------------

/// File-name stems `program` may resolve to (case-insensitive, extension ignored).
const NPM_PROGRAM_STEMS: &[&str] = &["npm"];
const BREW_PROGRAM_STEMS: &[&str] = &["brew"];

/// Rejects a plan that this module could not have built for `config` (see module docs). Pure.
pub fn validate_plan(plan: &InstallPlan, config: &AppConfig) -> AppResult<()> {
    let invalid = |why: &str| AppError::InvalidInput(format!("install plan rejected: {why}"));
    if !plan.env.is_empty() {
        return Err(invalid("environment overrides are not allowed"));
    }
    match plan.target {
        InstallTarget::Codex | InstallTarget::ClaudeCode => {
            validate_npm_plan(plan, config).map_err(invalid)
        }
        InstallTarget::Node => validate_brew_plan(plan).map_err(invalid),
        InstallTarget::CcSwitch => Err(invalid("CC Switch has no command to run")),
    }
}

/// `npm install -g <package of the plan's tool> --registry <configured registry url>`.
fn validate_npm_plan(plan: &InstallPlan, config: &AppConfig) -> Result<(), &'static str> {
    if !program_matches(&plan.program, NPM_PROGRAM_STEMS) {
        return Err("program must be npm");
    }
    let tool = match plan.target {
        InstallTarget::Codex => ToolId::Codex,
        InstallTarget::ClaudeCode => ToolId::ClaudeCode,
        InstallTarget::Node | InstallTarget::CcSwitch => return Err("not an npm target"),
    };
    let package = config
        .tools
        .iter()
        .find(|t| t.id == tool)
        .map(|t| t.npm_package.as_str())
        .ok_or("tool is not configured")?;
    let [install, global, pkg, registry_flag, registry_url] = plan.args.as_slice() else {
        return Err("unexpected argument shape");
    };
    if install != "install" || global != "-g" || registry_flag != "--registry" {
        return Err("unexpected arguments");
    }
    if pkg != package {
        return Err("package does not match the preset");
    }
    let known_registry = config
        .mirrors
        .npm_registries
        .iter()
        .any(|m| &m.url == registry_url);
    if !known_registry {
        return Err("registry is not one of the configured mirrors");
    }
    Ok(())
}

/// `brew install node` (the only Node command plan).
fn validate_brew_plan(plan: &InstallPlan) -> Result<(), &'static str> {
    if !program_matches(&plan.program, BREW_PROGRAM_STEMS) {
        return Err("program must be brew");
    }
    if plan.args != ["install", HOMEBREW_NODE_FORMULA] {
        return Err("unexpected arguments");
    }
    Ok(())
}

/// `true` when the file-name stem of `program` (bare name or full path, extension ignored) is
/// one of `stems`, case-insensitively.
fn program_matches(program: &str, stems: &[&str]) -> bool {
    let trimmed = program.trim();
    let file_name = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name.as_str(), |(stem, _)| stem);
    !stem.is_empty() && stems.contains(&stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::models::MirrorEntry;

    fn entry(id: &str, url: &str, page: &str) -> MirrorEntry {
        MirrorEntry {
            id: id.to_owned(),
            url: url.to_owned(),
            download_page: page.to_owned(),
        }
    }

    fn mirrors() -> MirrorChoice {
        MirrorChoice {
            npm_registry: entry("npmmirror", "https://registry.npmmirror.com/", ""),
            node_dist: entry(
                "npmmirror",
                "https://npmmirror.com/mirrors/node/",
                "https://npmmirror.com/mirrors/node/",
            ),
            probes: Vec::new(),
        }
    }

    fn ctx(platform: Platform) -> PlanContext {
        PlanContext {
            platform,
            homebrew: None,
            npm_program: "npm".to_owned(),
            npm_prefix_writable: true,
        }
    }

    fn cfg() -> AppConfig {
        config::embedded().expect("embedded config parses")
    }

    #[test]
    fn node_plan_is_manual_with_mirror_download_page() {
        let plan = build_plan(
            InstallTarget::Node,
            &cfg(),
            &mirrors(),
            &ctx(Platform::Windows),
        )
        .expect("plan");
        assert_eq!(plan.target, InstallTarget::Node);
        assert!(plan.program.is_empty());
        assert!(plan.args.is_empty());
        assert!(plan.display_command.is_empty());
        assert!(plan.registry.is_none());
        assert!(!plan.requires_admin);
        assert_eq!(plan.explanation_code, CODE_NODE_DOWNLOAD_PAGE);
        assert_eq!(
            plan.download_url.as_deref(),
            Some("https://npmmirror.com/mirrors/node/")
        );
    }

    #[test]
    fn node_plan_falls_back_to_dist_url_without_download_page() {
        let mut m = mirrors();
        m.node_dist = entry("official", "https://nodejs.org/dist/", "  ");
        let plan =
            build_plan(InstallTarget::Node, &cfg(), &m, &ctx(Platform::Macos)).expect("plan");
        assert_eq!(
            plan.download_url.as_deref(),
            Some("https://nodejs.org/dist/")
        );
    }

    #[test]
    fn node_plan_uses_homebrew_on_macos_when_available() {
        let mut c = ctx(Platform::Macos);
        c.homebrew = Some("/opt/homebrew/bin/brew".to_owned());
        let plan = build_plan(InstallTarget::Node, &cfg(), &mirrors(), &c).expect("plan");
        assert_eq!(plan.program, "/opt/homebrew/bin/brew");
        assert_eq!(
            plan.args,
            vec!["install", "node"],
            "unversioned: linked into PATH"
        );
        assert_eq!(plan.display_command, "/opt/homebrew/bin/brew install node");
        assert_eq!(plan.explanation_code, CODE_NODE_HOMEBREW);
        assert!(plan.registry.is_none());
        assert!(!plan.requires_admin);
        assert!(
            plan.download_url.is_some(),
            "download page kept as alternative"
        );
    }

    #[test]
    fn homebrew_is_ignored_off_macos() {
        let mut c = ctx(Platform::Windows);
        c.homebrew = Some("brew".to_owned());
        let plan = build_plan(InstallTarget::Node, &cfg(), &mirrors(), &c).expect("plan");
        assert!(plan.program.is_empty());
        assert_eq!(plan.explanation_code, CODE_NODE_DOWNLOAD_PAGE);
    }

    #[test]
    fn npm_plan_for_codex_uses_chosen_registry() {
        let mut c = ctx(Platform::Windows);
        c.npm_program = r"C:\Program Files\nodejs\npm.cmd".to_owned();
        let plan = build_plan(InstallTarget::Codex, &cfg(), &mirrors(), &c).expect("plan");
        assert_eq!(plan.target, InstallTarget::Codex);
        assert_eq!(plan.program, r"C:\Program Files\nodejs\npm.cmd");
        assert_eq!(
            plan.args,
            vec![
                "install",
                "-g",
                "@openai/codex",
                "--registry",
                "https://registry.npmmirror.com/"
            ]
        );
        assert_eq!(
            plan.display_command,
            r#""C:\Program Files\nodejs\npm.cmd" install -g @openai/codex --registry https://registry.npmmirror.com/"#
        );
        assert_eq!(
            plan.registry.as_ref().map(|r| r.id.as_str()),
            Some("npmmirror")
        );
        assert!(
            plan.env.is_empty(),
            "inherited environment, nothing overridden"
        );
        assert!(!plan.requires_admin);
        assert_eq!(plan.explanation_code, CODE_NPM_GLOBAL);
        assert!(plan.download_url.is_none());
    }

    #[test]
    fn npm_plan_for_claude_code_marks_admin_when_prefix_not_writable() {
        let mut c = ctx(Platform::Macos);
        c.npm_prefix_writable = false;
        let plan = build_plan(InstallTarget::ClaudeCode, &cfg(), &mirrors(), &c).expect("plan");
        assert_eq!(plan.target, InstallTarget::ClaudeCode);
        assert_eq!(plan.program, "npm");
        assert_eq!(plan.args[2], "@anthropic-ai/claude-code");
        assert_eq!(
            plan.display_command,
            "npm install -g @anthropic-ai/claude-code --registry https://registry.npmmirror.com/"
        );
        assert!(plan.requires_admin);
    }

    #[test]
    fn npm_plan_fails_when_tool_missing_from_config() {
        let mut c = cfg();
        c.tools.retain(|t| t.id != ToolId::Codex);
        let err = build_plan(
            InstallTarget::Codex,
            &c,
            &mirrors(),
            &ctx(Platform::Windows),
        )
        .expect_err("missing tool");
        assert_eq!(err.code(), "config");
    }

    #[test]
    fn validate_plan_accepts_what_build_plan_produces() {
        let cfg = cfg();
        let m = mirrors();
        let mut c = ctx(Platform::Windows);
        c.npm_program = r"C:\Program Files\nodejs\npm.cmd".to_owned();
        for target in [InstallTarget::Codex, InstallTarget::ClaudeCode] {
            let plan = build_plan(target, &cfg, &m, &c).expect("plan");
            validate_plan(&plan, &cfg).expect("own npm plan is valid");
        }
        let mut mac = ctx(Platform::Macos);
        mac.homebrew = Some("/opt/homebrew/bin/brew".to_owned());
        let brew = build_plan(InstallTarget::Node, &cfg, &m, &mac).expect("plan");
        validate_plan(&brew, &cfg).expect("own brew plan is valid");
    }

    type Mutation = Box<dyn Fn(&mut InstallPlan)>;

    #[test]
    fn validate_plan_rejects_tampered_plans() {
        let cfg = cfg();
        let base = build_plan(
            InstallTarget::Codex,
            &cfg,
            &mirrors(),
            &ctx(Platform::Windows),
        )
        .expect("plan");
        let tampered: Vec<(&str, Mutation)> = vec![
            (
                "other program",
                Box::new(|p| p.program = "powershell".into()),
            ),
            (
                "path to other program",
                Box::new(|p| p.program = r"C:\x\cmd.exe".into()),
            ),
            (
                "extra arg",
                Box::new(|p| p.args.push("--ignore-scripts".into())),
            ),
            (
                "missing arg",
                Box::new(|p| {
                    p.args.pop();
                }),
            ),
            (
                "other package",
                Box::new(|p| p.args[2] = "evil-package".into()),
            ),
            (
                "other registry",
                Box::new(|p| p.args[4] = "https://evil.example/".into()),
            ),
            ("not install", Box::new(|p| p.args[0] = "exec".into())),
            (
                "env override",
                Box::new(|p| {
                    p.env.insert("NODE_OPTIONS".into(), "--require x".into());
                }),
            ),
            (
                "target swap",
                Box::new(|p| p.target = InstallTarget::ClaudeCode),
            ),
            (
                "manual target",
                Box::new(|p| p.target = InstallTarget::CcSwitch),
            ),
            (
                "node with npm",
                Box::new(|p| p.target = InstallTarget::Node),
            ),
        ];
        for (name, mutate) in tampered {
            let mut plan = base.clone();
            mutate(&mut plan);
            let err = validate_plan(&plan, &cfg).expect_err(name);
            assert_eq!(err.code(), "invalid_input", "{name}");
        }
        // brew plan with tampered args
        let mut mac = ctx(Platform::Macos);
        mac.homebrew = Some("/opt/homebrew/bin/brew".to_owned());
        let mut brew = build_plan(InstallTarget::Node, &cfg, &mirrors(), &mac).expect("plan");
        brew.args = vec!["install".into(), "--cask".into(), "something".into()];
        assert!(validate_plan(&brew, &cfg).is_err());
    }

    #[test]
    fn program_matches_by_stem_case_insensitively() {
        for ok in [
            "npm",
            "NPM.CMD",
            r"C:\Program Files\nodejs\npm.cmd",
            "/opt/homebrew/bin/npm",
            "/usr/local/bin/npm",
        ] {
            assert!(program_matches(ok, NPM_PROGRAM_STEMS), "{ok}");
        }
        for bad in ["npx", "npm-evil", "cmd.exe", "", "/bin/sh", "npm/", "brew"] {
            assert!(!program_matches(bad, NPM_PROGRAM_STEMS), "{bad}");
        }
        assert!(program_matches(
            "/opt/homebrew/bin/brew",
            BREW_PROGRAM_STEMS
        ));
    }

    #[test]
    fn cc_switch_plan_is_manual_without_url() {
        let plan = build_plan(
            InstallTarget::CcSwitch,
            &cfg(),
            &mirrors(),
            &ctx(Platform::Windows),
        )
        .expect("plan");
        assert_eq!(plan.target, InstallTarget::CcSwitch);
        assert!(plan.program.is_empty());
        assert!(plan.display_command.is_empty());
        assert!(plan.registry.is_none());
        assert!(!plan.requires_admin);
        assert_eq!(plan.explanation_code, CODE_CC_SWITCH_DOWNLOAD);
        assert!(plan.download_url.is_none());
    }
}

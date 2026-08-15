//! Pure `InstallPlan` builders. Everything that touches the machine (finding `npm`, probing
//! the npm prefix, detecting Homebrew) is gathered into a [`PlanContext`] by the caller so the
//! decision logic here can be unit-tested on any host.
//!
//! Explanation codes (resolved by the UI as `install:plan.<code>`):
//!
//! | target              | code                 | command                                        |
//! | ------------------- | -------------------- | ---------------------------------------------- |
//! | Node (default)      | `node.download_page` | none — `download_url` = mirror download page   |
//! | Node (macOS + brew) | `node.homebrew`      | `brew install node@<recommended LTS>`          |
//! | Codex / Claude Code | `npm_global`         | `npm install -g <pkg> --registry <mirror url>` |
//! | CC Switch           | `cc_switch.download` | none — release fetched via `cc_switch`         |

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

/// Node.js: manual download from the chosen mirror; on macOS with Homebrew the plan proposes
/// `brew install node@<lts>` and still carries the download page as an alternative.
fn node_plan(config: &AppConfig, mirrors: &MirrorChoice, ctx: &PlanContext) -> InstallPlan {
    let download_url = node_download_url(mirrors);
    let brew = ctx
        .homebrew
        .as_deref()
        .filter(|_| ctx.platform == Platform::Macos);
    if let Some(brew) = brew {
        let spec = CommandSpec::new(
            brew,
            [
                "install".to_owned(),
                homebrew_node_formula(&config.requirements.node_recommended_lts),
            ],
        );
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

/// `node@<major>` (Homebrew versioned formula); a bare `node` when the LTS hint is empty.
fn homebrew_node_formula(recommended_lts: &str) -> String {
    let major: String = recommended_lts
        .trim()
        .trim_start_matches('v')
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if major.is_empty() {
        "node".to_owned()
    } else {
        format!("node@{major}")
    }
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
        assert_eq!(plan.args, vec!["install", "node@22"]);
        assert_eq!(
            plan.display_command,
            "/opt/homebrew/bin/brew install node@22"
        );
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
    fn homebrew_formula_from_lts_hint() {
        let cases = [
            ("22", "node@22"),
            ("v20.11.0", "node@20"),
            (" 18 ", "node@18"),
            ("", "node"),
            ("lts", "node"),
        ];
        for (input, expected) in cases {
            assert_eq!(homebrew_node_formula(input), expected, "input {input:?}");
        }
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

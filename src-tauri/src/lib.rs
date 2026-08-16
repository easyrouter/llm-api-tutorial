//! SeedRouter Onboarding — Rust core.
//!
//! Module map (mirrors PRD §四 M1–M6; see docs/ARCHITECTURE.md):
//!
//! | module      | PRD | responsibility                                                     |
//! |-------------|-----|--------------------------------------------------------------------|
//! | `checks`    | M1  | environment detection: OS, Node/npm, Codex, Claude Code, CC Switch, env vars, network |
//! | `install`   | M2  | install plans (shown before run), npm global installs with live output, CC Switch download + hash |
//! | `guide`     | M3  | CC Switch configuration walkthrough data, API-key format validation, URL rule preview |
//! | `verify`    | M4  | fresh-session CLI check, gateway probe (key in memory only), running-terminal detection |
//! | `diagnose`  | M5  | rule engine for guide faults A–G, redacted diagnostic report        |
//! | `docs`      | M6  | fetch help docs from the designated site with cache + bundled fallback |
//! | `telemetry` | #16 | opt-in aggregate events to an intranet endpoint, no secrets        |
//!
//! Cross-cutting: `config` (company preset), `platform`, `process`, `net`, `redact`, `state`.
//!
//! Hard rules (ADR-0003 / PRD #4, #17): this crate never writes to `~/.codex`, `~/.claude`
//! or `~/.cc-switch`, never edits environment variables, and never persists an API key.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod checks;
pub mod commands;
pub mod config;
pub mod diagnose;
pub mod docs;
pub mod error;
pub mod events;
pub mod fast_ui;
pub mod guide;
pub mod install;
pub mod models;
pub mod net;
pub mod platform;
pub mod process;
pub mod redact;
pub mod state;
pub mod telemetry;
pub mod verify;

use tauri::Manager;

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_plugin = tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Info)
        .targets([
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
        ])
        .max_file_size(2 * 1024 * 1024)
        .build();

    let result = tauri::Builder::default()
        .plugin(log_plugin)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let state = state::AppState::init(app.handle())?;
            app.manage(state);
            start_telemetry_flush(app.handle());
            log::info!(
                "seedrouter-onboarding {} started",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // app / config
            commands::get_app_info,
            commands::get_app_config,
            commands::open_external,
            // M1
            commands::run_env_checks,
            commands::run_env_check,
            commands::probe_mirrors,
            // M2
            commands::plan_install,
            commands::start_install,
            commands::cancel_install,
            commands::fetch_cc_switch_release,
            commands::download_file,
            commands::open_downloaded_file,
            // M3
            commands::get_config_guide,
            commands::validate_api_key,
            commands::preview_effective_url,
            commands::test_connectivity,
            commands::list_gateway_models,
            commands::get_codex_config_template,
            commands::preview_cc_switch_import,
            commands::open_cc_switch_import,
            // optional Codex Fast UI toolkit (Windows, ADR-0007)
            commands::codex_fast_ui_status,
            commands::plan_codex_fast_ui,
            commands::start_codex_fast_ui,
            // M4
            commands::verify_setup,
            commands::list_running_terminals,
            // M5
            commands::diagnose,
            commands::build_diagnostic_report,
            commands::save_diagnostic_report,
            // M6
            commands::fetch_docs_index,
            commands::fetch_doc_page,
            // telemetry
            commands::track_event,
            commands::set_telemetry_enabled,
            commands::get_telemetry_status,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        log::error!("error while running tauri application: {e}");
    }
}

/// Starts the opt-in telemetry periodic flush loop (no-op when no endpoint is configured).
///
/// The loop is detached: it lives for the whole process and only posts already-queued,
/// secret-free aggregate events, so nothing needs to be joined on shutdown.
fn start_telemetry_flush(app: &tauri::AppHandle) {
    let state = app.state::<state::AppState>();
    let interval = state.telemetry.flush_interval();
    if state.telemetry.spawn_periodic_flush(interval).is_some() {
        log::info!("telemetry periodic flush every {}s", interval.as_secs());
    }
}

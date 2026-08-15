//! Redacted diagnostic report (Markdown). See parent module docs.

use crate::models::{AppConfig, AppInfo, Diagnosis, DiagnosticReport, EnvSnapshot};

pub struct ReportInput<'a> {
    pub app: &'a AppInfo,
    pub config: &'a AppConfig,
    pub snapshot: Option<&'a EnvSnapshot>,
    pub diagnoses: &'a [Diagnosis],
}

impl std::fmt::Debug for ReportInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReportInput")
            .field("app", &self.app.version)
            .finish_non_exhaustive()
    }
}

pub fn build(input: &ReportInput<'_>) -> DiagnosticReport {
    // TODO(impl): render sections; pass every free-text field through redact::redact_secrets
    let generated_at = chrono::Utc::now().to_rfc3339();
    let markdown = format!(
        "# Codex Onboarding diagnostic report\n\n- generated: {generated_at}\n- app: {}\n",
        input.app.version
    );
    DiagnosticReport {
        generated_at,
        app_version: input.app.version.clone(),
        markdown,
    }
}

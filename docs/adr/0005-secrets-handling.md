# ADR-0005: API keys stay in memory; everything else is redacted

- Status: accepted
- Date: 2026-08-15

## Context

PRD #14/#15 allow the tool to perform a live connectivity test with the user's key; PRD §七
requires that keys never leak via reports or telemetry.

## Decision

- The key travels UI → Rust once (`GatewayProbeRequest.apiKey`), is used for one HTTPS request
  and dropped. It is never logged, persisted, echoed back, or stored in any store.
- All strings that could contain secrets (process output, server bodies, env values, reports)
  pass through `redact::redact_secrets`; known secrets are masked with `redact::mask_value`.
- Telemetry carries only enumerated fields (`TelemetryEvent`), never free text.
- Diagnostic reports are built from redacted data and re-redacted on save.

## Consequences

- Slight loss of detail in error messages; acceptable for the audience.
- Any new output path must call the redaction helpers — enforced by the review checklist.

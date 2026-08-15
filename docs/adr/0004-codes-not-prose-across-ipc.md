# ADR-0004: Rust returns codes + params; the UI owns all prose (zh-CN + en)

- Status: accepted
- Date: 2026-08-15

## Context

Bilingual native UI (#9) and a non-developer audience (#8) mean every message must exist in
both languages and be reviewable by non-engineers.

## Decision

The Rust core never emits user-facing prose. Results carry machine codes (`CheckResult.code`,
`Diagnosis.code`, `AppError.code`) plus interpolation params. Locale files are split per
namespace (`common checks install guide verify diagnose help`) with `zh-CN` as source of truth;
`scripts/check-i18n.mjs` enforces key parity in the pre-commit hook and CI.

## Consequences

- Translators / IT can edit locale JSON without touching Rust.
- Adding a code requires adding keys in both locales (CI fails otherwise).
- Free-form strings from the machine (paths, versions, redacted server messages) are passed as
  `details` / `params` and shown verbatim.

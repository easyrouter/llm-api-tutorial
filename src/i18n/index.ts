/**
 * i18n bootstrap.
 *
 * Locale files live in `src/i18n/locales/<lang>/<namespace>.json`. zh-CN is the source locale;
 * every other locale must have identical keys per namespace (enforced by scripts/check-i18n.mjs).
 * Namespaces map to PRD modules so each feature owns its own files:
 *   common · checks (M1) · install (M2) · guide (M3) · verify (M4) · diagnose (M5) · help (M6)
 *
 * Language resolution: persisted user choice → hint from Rust (the NSIS installer's language
 * selection, else the OS UI language) → navigator.language → zh-CN.
 */
import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import enChecks from "./locales/en/checks.json";
import enCommon from "./locales/en/common.json";
import enDiagnose from "./locales/en/diagnose.json";
import enGuide from "./locales/en/guide.json";
import enHelp from "./locales/en/help.json";
import enInstall from "./locales/en/install.json";
import enVerify from "./locales/en/verify.json";
import zhChecks from "./locales/zh-CN/checks.json";
import zhCommon from "./locales/zh-CN/common.json";
import zhDiagnose from "./locales/zh-CN/diagnose.json";
import zhGuide from "./locales/zh-CN/guide.json";
import zhHelp from "./locales/zh-CN/help.json";
import zhInstall from "./locales/zh-CN/install.json";
import zhVerify from "./locales/zh-CN/verify.json";

export const SUPPORTED_LANGS = ["zh-CN", "en"] as const;
export type Lang = (typeof SUPPORTED_LANGS)[number];
export const DEFAULT_LANG: Lang = "zh-CN";
export const NAMESPACES = [
  "common",
  "checks",
  "install",
  "guide",
  "verify",
  "diagnose",
  "help",
] as const;
export type Namespace = (typeof NAMESPACES)[number];

const STORAGE_KEY = "seedrouter-onboarding.lang";

export const resources = {
  "zh-CN": {
    common: zhCommon,
    checks: zhChecks,
    install: zhInstall,
    guide: zhGuide,
    verify: zhVerify,
    diagnose: zhDiagnose,
    help: zhHelp,
  },
  en: {
    common: enCommon,
    checks: enChecks,
    install: enInstall,
    guide: enGuide,
    verify: enVerify,
    diagnose: enDiagnose,
    help: enHelp,
  },
} as const;

export function normalizeLang(input: string | null | undefined): Lang | null {
  if (!input) return null;
  const lower = input.toLowerCase();
  if (lower.startsWith("zh")) return "zh-CN";
  if (lower.startsWith("en")) return "en";
  return null;
}

export function detectInitialLang(osHint?: string): Lang {
  const stored = normalizeLang(localStorage.getItem(STORAGE_KEY));
  if (stored) return stored;
  const hinted = normalizeLang(osHint);
  if (hinted) return hinted;
  return normalizeLang(navigator.language) ?? DEFAULT_LANG;
}

export async function setLang(lang: Lang): Promise<void> {
  localStorage.setItem(STORAGE_KEY, lang);
  await i18n.changeLanguage(lang);
  document.documentElement.lang = lang;
}

void i18n.use(initReactI18next).init({
  resources,
  lng: DEFAULT_LANG,
  fallbackLng: DEFAULT_LANG,
  supportedLngs: [...SUPPORTED_LANGS],
  ns: [...NAMESPACES],
  defaultNS: "common",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;

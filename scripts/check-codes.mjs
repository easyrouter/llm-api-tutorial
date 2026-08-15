#!/usr/bin/env node
/**
 * Verifies that every i18n code the Rust core emits has a translation in the locale files.
 *
 * Rust returns codes + params, never prose (CLAUDE.md hard rule 5); the UI resolves them as
 * `t("<namespace>:<code>", params)`. This script extracts the code literals from
 * `src-tauri/src` (test modules and comments are stripped first) and fails when the resolved
 * key is missing in `src/i18n/locales/zh-CN` (parity with other locales is enforced separately
 * by `check-i18n.mjs`).
 *
 * Extraction rules (kept narrow to avoid false positives):
 *
 * | pattern (non-test Rust)                                   | expected key(s)                                  |
 * | --------------------------------------------------------- | ------------------------------------------------ |
 * | `Verdict::pass\|warn\|fail("X")`, `Verdict::new(_, "X")`  | `checks:X`                                       |
 * | `code: "X"` in `checks/`                                  | `checks:X`                                       |
 * | `code: "X"` in `diagnose/`                                | `diagnose:X.title`, `diagnose:X.explanation`     |
 * | `keys(&["a", …])` in the same fn as `code: "X"` (diagnose)| `diagnose:X.steps.a` or `diagnose:steps.a`       |
 * | `code: format!("ns:prefix.{}", …)`                        | at least one key starting with `ns:prefix.`      |
 * | `step("X", …)` in `guide.rs`                              | `guide:X.title`, `guide:X.body`                  |
 * | `const CODE_*: &str = "X"` in `install/`                  | `install:plan.X`                                 |
 * | `const *_LABEL: &str = "X"`, `label_code: "X"`            | `common:fixes.labels.X`                          |
 * | `const *_INSTRUCTIONS: &str = "ns:X"`                     | `ns:X`                                           |
 * | any literal `"ns:some.key"` (ns = known namespace)        | `ns:some.key`                                    |
 * | `AppError::code()` match arms `=> "x"` in `error.rs`      | `common:errors.x`                                |
 * | wire enums (`KeyIssue`, `UrlRule`, `UrlWarning`,          | `guide:key.issue.<snake>`, `guide:url.rule.…`,   |
 * |   `ErrorClass`) in `models.rs`                            | `guide:url.warning.…`, `verify:errorClass.…`     |
 *
 * Exceptions live in `scripts/check-codes.allowlist.json` (`ignore`: exact keys,
 * `ignorePrefixes`: key prefixes). Run: `node scripts/check-codes.mjs` (part of `npm run
 * i18n:check`).
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const rustDir = join(root, "src-tauri", "src");
const localesDir = join(root, "src", "i18n", "locales");
const allowlistPath = join(here, "check-codes.allowlist.json");
const SOURCE_LOCALE = "zh-CN";
const NAMESPACES = ["common", "checks", "install", "guide", "verify", "diagnose", "help"];

/** Wire enums (serde `rename_all = "snake_case"`) whose values the UI resolves as keys. */
const ENUM_KEY_PREFIX = {
  KeyIssue: "guide:key.issue.",
  UrlRule: "guide:url.rule.",
  UrlWarning: "guide:url.warning.",
  ErrorClass: "verify:errorClass.",
};

// ---------------------------------------------------------------------------
// File helpers
// ---------------------------------------------------------------------------

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (entry.endsWith(".rs")) out.push(path);
  }
  return out;
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    console.error(`x cannot parse ${path}: ${err.message}`);
    process.exit(1);
  }
}

function flatten(obj, prefix, out) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) flatten(v, key, out);
    else out.add(key);
  }
  return out;
}

/** All keys of one locale as `namespace:flattened.key`. */
function localeKeys(lang) {
  const keys = new Set();
  const dir = join(localesDir, lang);
  for (const file of readdirSync(dir).filter((f) => f.endsWith(".json"))) {
    const ns = file.slice(0, -".json".length);
    for (const k of flatten(readJson(join(dir, file)), "", new Set())) keys.add(`${ns}:${k}`);
  }
  return keys;
}

// ---------------------------------------------------------------------------
// Rust source cleaning: drop comments and `#[cfg(test)] mod … { … }` blocks, keep strings
// ---------------------------------------------------------------------------

/**
 * Returns the index just past the string literal starting at `i` (`"` or `r#*"`), or -1 when
 * `src[i]` does not start one. Handles escapes and raw strings.
 */
function skipString(src, i) {
  if (src[i] === '"') {
    let j = i + 1;
    while (j < src.length) {
      if (src[j] === "\\") j += 2;
      else if (src[j] === '"') return j + 1;
      else j += 1;
    }
    return src.length;
  }
  if (src[i] === "r" && (src[i + 1] === '"' || src[i + 1] === "#")) {
    let hashes = 0;
    let j = i + 1;
    while (src[j] === "#") {
      hashes += 1;
      j += 1;
    }
    if (src[j] !== '"') return -1;
    const close = `"${"#".repeat(hashes)}`;
    const end = src.indexOf(close, j + 1);
    return end === -1 ? src.length : end + close.length;
  }
  return -1;
}

/** Index just past a char literal starting at `i` (`'x'`, `'\n'`), or -1 for lifetimes. */
function skipChar(src, i) {
  if (src[i] !== "'") return -1;
  if (src[i + 1] === "\\") {
    const end = src.indexOf("'", i + 2);
    return end === -1 ? -1 : end + 1;
  }
  return src[i + 2] === "'" ? i + 3 : -1;
}

/** Removes line comments and (nested) block comments, preserving string literals and newlines. */
function stripComments(src) {
  let out = "";
  let i = 0;
  const isIdent = (c) => /[A-Za-z0-9_]/.test(c ?? "");
  while (i < src.length) {
    const rawStart = src[i] === "r" && !isIdent(src[i - 1]);
    const strEnd = src[i] === '"' || rawStart ? skipString(src, i) : -1;
    if (strEnd !== -1) {
      out += src.slice(i, strEnd);
      i = strEnd;
      continue;
    }
    const chEnd = skipChar(src, i);
    if (chEnd !== -1) {
      out += src.slice(i, chEnd);
      i = chEnd;
      continue;
    }
    if (src.startsWith("//", i)) {
      const nl = src.indexOf("\n", i);
      i = nl === -1 ? src.length : nl;
      continue;
    }
    if (src.startsWith("/*", i)) {
      let depth = 1;
      let j = i + 2;
      while (j < src.length && depth > 0) {
        if (src.startsWith("/*", j)) {
          depth += 1;
          j += 2;
        } else if (src.startsWith("*/", j)) {
          depth -= 1;
          j += 2;
        } else {
          if (src[j] === "\n") out += "\n";
          j += 1;
        }
      }
      i = j;
      continue;
    }
    out += src[i];
    i += 1;
  }
  return out;
}

/** Index of the `}` matching the `{` at `open` (string/char aware), or -1. */
function matchBrace(src, open) {
  let depth = 0;
  let i = open;
  while (i < src.length) {
    const strEnd = src[i] === '"' || src[i] === "r" ? skipString(src, i) : -1;
    if (strEnd !== -1) {
      i = strEnd;
      continue;
    }
    const chEnd = skipChar(src, i);
    if (chEnd !== -1) {
      i = chEnd;
      continue;
    }
    if (src[i] === "{") depth += 1;
    else if (src[i] === "}") {
      depth -= 1;
      if (depth === 0) return i;
    }
    i += 1;
  }
  return -1;
}

/** Removes every `#[cfg(test)] mod name { … }` block. */
function stripTestModules(src) {
  const re = /#\[cfg\(test\)\]\s*(?:pub(?:\([a-z]+\))?\s+)?mod\s+\w+\s*\{/g;
  let out = "";
  let last = 0;
  let m;
  while ((m = re.exec(src)) !== null) {
    const open = m.index + m[0].length - 1;
    const close = matchBrace(src, open);
    if (close === -1) break;
    out += src.slice(last, m.index);
    last = close + 1;
    re.lastIndex = last;
  }
  return out + src.slice(last);
}

function cleanRust(src) {
  return stripTestModules(stripComments(src));
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/** One expectation: `keys` (any of them satisfies) or a `prefix` (any key starting with it). */
function expectation(where, keys, prefix = null) {
  return { where, keys, prefix };
}

function toSnakeCase(name) {
  return name.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

/** Splits cleaned source into top-level `fn` bodies (used to pair codes with checklists). */
function topLevelFunctions(src) {
  const starts = [];
  const re = /^(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+\w+/gm;
  let m;
  while ((m = re.exec(src)) !== null) starts.push(m.index);
  return starts.map((s, i) => src.slice(s, starts[i + 1] ?? src.length));
}

function extract(file, cleaned) {
  const rel = relative(rustDir, file).split(sep).join("/");
  const at = (line) => `${rel}:${line}`;
  const lineOf = (index) => cleaned.slice(0, index).split("\n").length;
  const found = [];
  const each = (re, fn) => {
    let m;
    while ((m = re.exec(cleaned)) !== null) fn(m, at(lineOf(m.index)));
  };
  const inChecks = rel.startsWith("checks/");
  const inDiagnose = rel.startsWith("diagnose/");
  const inInstall = rel.startsWith("install/");

  // Verdict::pass("x") / warn / fail / new(status, "x")
  each(/Verdict::(?:pass|warn|fail)\(\s*"([^"]+)"\s*\)/g, (m, where) =>
    found.push(expectation(where, [`checks:${m[1]}`])),
  );
  each(/Verdict::new\(\s*[^,()]+,\s*"([^"]+)"\s*\)/g, (m, where) =>
    found.push(expectation(where, [`checks:${m[1]}`])),
  );

  // A code literal in module context: fully qualified codes checked as-is, otherwise by module.
  const pushCode = (code, where) => {
    if (code.includes(":")) found.push(expectation(where, [code]));
    else if (inChecks) found.push(expectation(where, [`checks:${code}`]));
    else if (inDiagnose) {
      found.push(expectation(where, [`diagnose:${code}.title`]));
      found.push(expectation(where, [`diagnose:${code}.explanation`]));
    }
  };

  // code: "x" (struct fields)
  each(/\bcode:\s*"([^"]+)"/g, (m, where) => pushCode(m[1], where));

  // helper("x", …) where the helper's first parameter is named `code` (e.g. `fn broken(code:`)
  const helpers = [...cleaned.matchAll(/\bfn\s+(\w+)\s*\(\s*code\s*:/g)].map((m) => m[1]);
  for (const helper of helpers) {
    each(new RegExp(`\\b${helper}\\(\\s*"([^"]+)"`, "g"), (m, where) => pushCode(m[1], where));
  }

  // code: format!("ns:prefix.{}", …) → prefix must exist
  each(/\bcode:\s*format!\(\s*"([a-z]+:[a-z0-9_.]+)\{\}"/g, (m, where) =>
    found.push(expectation(where, [], m[1])),
  );

  // label_code: "x"
  each(/\blabel_code:\s*"([^"]+)"/g, (m, where) =>
    found.push(expectation(where, [`common:fixes.labels.${m[1]}`])),
  );

  // Diagnose checklists: keys(&["a", "b"]) paired with the code: "x" of the same fn
  if (inDiagnose) {
    for (const body of topLevelFunctions(cleaned)) {
      const code = /\bcode:\s*"([^"]+)"/.exec(body)?.[1];
      if (!code) continue;
      const where = at(lineOf(cleaned.indexOf(body)));
      const lists = body.matchAll(/\bkeys\(\s*&\[([^\]]*)\]/g);
      for (const list of lists) {
        for (const [, step] of list[1].matchAll(/"([^"]+)"/g)) {
          found.push(
            expectation(where, [`diagnose:${code}.steps.${step}`, `diagnose:steps.${step}`]),
          );
        }
      }
    }
  }

  // Guide steps: step("x", …)
  if (rel === "guide.rs") {
    each(/\bstep\(\s*"([^"]+)"/g, (m, where) => {
      found.push(expectation(where, [`guide:${m[1]}.title`]));
      found.push(expectation(where, [`guide:${m[1]}.body`]));
    });
  }

  // Constants: CODE_* (install plans), *_LABEL (fix labels), *_INSTRUCTIONS (qualified)
  each(/\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"([^"]+)"/g, (m, where) => {
    const [, name, value] = m;
    if (inInstall && name.startsWith("CODE_")) {
      found.push(expectation(where, [`install:plan.${value}`]));
    } else if (name.endsWith("_LABEL")) {
      found.push(expectation(where, [`common:fixes.labels.${value}`]));
    } else if (name.endsWith("_INSTRUCTIONS")) {
      found.push(expectation(where, [value]));
    }
  });

  // Any fully qualified literal "ns:some.key"
  const nsAlt = NAMESPACES.join("|");
  each(new RegExp(`"((?:${nsAlt}):[a-z0-9_]+(?:\\.[a-zA-Z0-9_]+)*)"`, "g"), (m, where) =>
    found.push(expectation(where, [m[1]])),
  );

  // AppError::code() → common:errors.<code>
  if (rel === "error.rs") {
    const start = cleaned.indexOf("fn code(&self)");
    if (start !== -1) {
      const open = cleaned.indexOf("{", start);
      const close = matchBrace(cleaned, open);
      const body = cleaned.slice(open, close === -1 ? undefined : close);
      for (const arm of body.matchAll(/=>\s*"([a-z0-9_]+)"/g)) {
        found.push(expectation(at(lineOf(open)), [`common:errors.${arm[1]}`]));
      }
    }
  }

  // Wire enums resolved by the UI
  if (rel === "models.rs") {
    for (const [name, prefix] of Object.entries(ENUM_KEY_PREFIX)) {
      const m = new RegExp(`\\benum\\s+${name}\\s*\\{([^}]*)\\}`).exec(cleaned);
      if (!m) continue;
      const where = at(lineOf(m.index));
      for (const v of m[1].matchAll(/^\s*([A-Z][A-Za-z0-9]*)\s*,?\s*$/gm)) {
        found.push(expectation(where, [`${prefix}${toSnakeCase(v[1])}`]));
      }
    }
  }

  return found;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const allowlist = readJson(allowlistPath);
const ignore = new Set(allowlist.ignore ?? []);
const ignorePrefixes = allowlist.ignorePrefixes ?? [];
const isAllowed = (key) => ignore.has(key) || ignorePrefixes.some((p) => key.startsWith(p));

const keys = localeKeys(SOURCE_LOCALE);
const seen = new Set();
const expectations = walk(rustDir)
  .flatMap((file) => extract(file, cleanRust(readFileSync(file, "utf8"))))
  .filter((e) => {
    const id = `${e.where}|${e.prefix ?? e.keys.join("|")}`;
    if (seen.has(id)) return false;
    seen.add(id);
    return true;
  });

const satisfied = (e) =>
  e.prefix
    ? [...keys].some((k) => k.startsWith(e.prefix))
    : e.keys.some((k) => keys.has(k) || isAllowed(k));

const missing = expectations.filter((e) => !satisfied(e));
const checked = new Set(expectations.flatMap((e) => (e.prefix ? [e.prefix] : e.keys)));

if (process.argv.includes("--list")) {
  for (const e of expectations) {
    console.log(`${e.where}\t${e.prefix ? `${e.prefix}*` : e.keys.join(" | ")}`);
  }
}

if (missing.length) {
  console.error(`x ${missing.length} code(s) without a ${SOURCE_LOCALE} translation:`);
  for (const e of missing) {
    const want = e.prefix ? `${e.prefix}* (prefix)` : e.keys.join(" | ");
    console.error(`    ${e.where}: ${want}`);
  }
  console.error("i18n codes: FAILED");
  process.exit(1);
}
console.log(
  `i18n codes: OK (${checked.size} keys referenced by src-tauri/src exist in ${SOURCE_LOCALE})`,
);

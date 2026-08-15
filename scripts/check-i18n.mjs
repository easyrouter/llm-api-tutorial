#!/usr/bin/env node
/**
 * Verifies that, for every namespace file, all locales have exactly the same set of
 * (flattened) keys. `zh-CN` is the source locale; missing/extra keys elsewhere fail the check,
 * as does a namespace file that exists in one locale but not another.
 *
 * Layout: src/i18n/locales/<lang>/<namespace>.json
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const localesDir = join(here, "..", "src", "i18n", "locales");
const SOURCE = "zh-CN";

function flatten(obj, prefix = "", out = new Set()) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) flatten(v, key, out);
    else out.add(key);
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

const langs = readdirSync(localesDir).filter((d) => statSync(join(localesDir, d)).isDirectory());
if (!langs.includes(SOURCE)) {
  console.error(`x source locale directory ${SOURCE} not found in ${localesDir}`);
  process.exit(1);
}

const sourceFiles = readdirSync(join(localesDir, SOURCE)).filter((f) => f.endsWith(".json"));
let failed = false;
let total = 0;

for (const lang of langs) {
  if (lang === SOURCE) continue;
  const langFiles = new Set(readdirSync(join(localesDir, lang)).filter((f) => f.endsWith(".json")));
  for (const file of sourceFiles) {
    if (!langFiles.has(file)) {
      failed = true;
      console.error(`x ${lang}/${file}: file missing`);
      continue;
    }
    const src = flatten(readJson(join(localesDir, SOURCE, file)));
    const dst = flatten(readJson(join(localesDir, lang, file)));
    const missing = [...src].filter((k) => !dst.has(k));
    const extra = [...dst].filter((k) => !src.has(k));
    total += src.size;
    if (missing.length || extra.length) {
      failed = true;
      console.error(`x ${lang}/${file}: ${missing.length} missing, ${extra.length} extra`);
      for (const k of missing) console.error(`    missing: ${k}`);
      for (const k of extra) console.error(`    extra:   ${k}`);
    } else {
      console.log(`ok ${lang}/${file}: ${src.size} keys`);
    }
  }
  for (const file of langFiles) {
    if (!sourceFiles.includes(file)) {
      failed = true;
      console.error(`x ${lang}/${file}: no counterpart in ${SOURCE}`);
    }
  }
}

console.log(failed ? "i18n parity: FAILED" : `i18n parity: OK (${total} keys checked)`);
process.exit(failed ? 1 : 0);

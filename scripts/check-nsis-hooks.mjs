#!/usr/bin/env node
// Keeps the literal registry paths in src-tauri/nsis/hooks.nsh in sync with tauri.conf.json.
// The hooks file is included before the bundler defines ${MANUFACTURER}/${PRODUCTNAME}, so it
// cannot reference them and has to spell the product name and publisher out.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const conf = JSON.parse(readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
const hooks = readFileSync(path.join(root, "src-tauri/nsis/hooks.nsh"), "utf8");

const product = conf.productName;
const publisher = conf.bundle?.publisher ?? conf.identifier.split(".")[1];
const expected = {
  SR_MANUPRODUCTKEY: `Software\\${publisher}\\${product}`,
  SR_UNINSTKEY: `Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${product}`,
};

let failed = false;
for (const [name, value] of Object.entries(expected)) {
  const match = hooks.match(new RegExp(`^!define ${name} "([^"]*)"`, "m"));
  if (!match) {
    console.error(`hooks.nsh: missing !define ${name}`);
    failed = true;
  } else if (match[1] !== value) {
    console.error(`hooks.nsh: ${name} is "${match[1]}", tauri.conf.json implies "${value}"`);
    failed = true;
  }
}
const hooksPath = conf.bundle?.windows?.nsis?.installerHooks;
if (hooksPath !== "./nsis/hooks.nsh") {
  console.error(`tauri.conf.json: bundle.windows.nsis.installerHooks should be "./nsis/hooks.nsh"`);
  failed = true;
}
if (failed) process.exit(1);
console.log("nsis hooks: registry paths match tauri.conf.json");

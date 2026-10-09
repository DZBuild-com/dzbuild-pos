// Verifies ar/fr/en catalogues have identical key sets and no empty values:
// UI catalogues src/locales/<lang>/<namespace>.json and document catalogues
// crates/pos-docs/locales/<lang>.json; and that the help pages src/help/<lang>/<topic>.md match.
// Run: `pnpm i18n` or `node scripts/i18n/check-i18n.mjs [root]` (root defaults to the repository).
import { readFileSync, readdirSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const LANGS = ["ar", "fr", "en"];

function flatten(obj, prefix = "", out = {}) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) flatten(v, key, out);
    else out[key] = v;
  }
  return out;
}

function readMap(file, label, problems) {
  try {
    return flatten(JSON.parse(readFileSync(file, "utf8")));
  } catch (e) {
    problems.push(`${label} invalid JSON: ${e.message}`);
    return null;
  }
}

function compare(maps, label, problems) {
  const all = new Set(Object.values(maps).flatMap((m) => Object.keys(m)));
  for (const [lang, map] of Object.entries(maps)) {
    const miss = [...all].filter((k) => !(k in map));
    const empty = Object.entries(map)
      .filter(([, v]) => v === "" || v == null)
      .map(([k]) => k);
    if (miss.length)
      problems.push(
        `${lang}/${label} missing keys: ${miss.slice(0, 30).join(", ")}${miss.length > 30 ? " ..." : ""}`,
      );
    if (empty.length) problems.push(`${lang}/${label} empty values: ${empty.slice(0, 30).join(", ")}`);
  }
}

export function checkUiLocales(root) {
  const dir = path.join(root, "src", "locales");
  if (!existsSync(dir)) return [];
  const problems = [];
  const namespaces = new Set();
  for (const l of LANGS) {
    if (!existsSync(path.join(dir, l))) {
      problems.push(`missing locale folder src/locales/${l}`);
      continue;
    }
    for (const f of readdirSync(path.join(dir, l))) if (f.endsWith(".json")) namespaces.add(f);
  }
  for (const ns of namespaces) {
    const maps = {};
    for (const l of LANGS) {
      const p = path.join(dir, l, ns);
      if (!existsSync(p)) {
        problems.push(`${l}/${ns} missing`);
        continue;
      }
      const map = readMap(p, `${l}/${ns}`, problems);
      if (map) maps[l] = map;
    }
    compare(maps, ns, problems);
  }
  return problems;
}

export function checkDocLocales(root) {
  const dir = path.join(root, "crates", "pos-docs", "locales");
  if (!existsSync(dir)) return [];
  const problems = [];
  const maps = {};
  for (const l of LANGS) {
    const p = path.join(dir, `${l}.json`);
    if (!existsSync(p)) {
      problems.push(`crates/pos-docs/locales/${l}.json missing`);
      continue;
    }
    const map = readMap(p, `pos-docs ${l}.json`, problems);
    if (map) maps[l] = map;
  }
  compare(maps, "pos-docs", problems);
  return problems;
}

// Help pages (NFR-027: "help" is part of the translation gate): the same files in the three
// languages, none empty, the same number of "## " sections and of "- [ ]" checklist items.
export function checkHelpPages(root) {
  const dir = path.join(root, "src", "help");
  if (!existsSync(dir)) return [];
  const problems = [];
  const files = {};
  for (const l of LANGS) {
    const langDir = path.join(dir, l);
    files[l] = existsSync(langDir) ? readdirSync(langDir).filter((f) => f.endsWith(".md")) : [];
  }
  const all = new Set(Object.values(files).flat());
  const shape = (text) => ({
    sections: text.split("\n").filter((line) => line.startsWith("## ")).length,
    checks: text.split("\n").filter((line) => line.startsWith("- [ ]")).length,
  });
  for (const file of all) {
    const texts = {};
    for (const l of LANGS) {
      if (!files[l].includes(file)) {
        problems.push(`src/help/${l}/${file} missing`);
        continue;
      }
      const text = readFileSync(path.join(dir, l, file), "utf8");
      if (!text.trim()) {
        problems.push(`${l}/${file} is empty`);
        continue;
      }
      texts[l] = shape(text);
    }
    const reference = texts.fr;
    if (!reference) continue;
    for (const l of LANGS) {
      if (!texts[l] || l === "fr") continue;
      if (texts[l].sections !== reference.sections) {
        problems.push(`${l}/${file} has ${texts[l].sections} sections, fr has ${reference.sections}`);
      }
      if (texts[l].checks !== reference.checks) {
        problems.push(`${l}/${file} has ${texts[l].checks} checklist items, fr has ${reference.checks}`);
      }
    }
  }
  return problems;
}

// Returns problem strings (empty = OK).
export function checkI18n(root) {
  return [...checkUiLocales(root), ...checkDocLocales(root), ...checkHelpPages(root)];
}

export const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

// Prints the problems and returns the exit code (0 = in sync).
export function main(root = repoRoot) {
  const problems = checkI18n(root);
  if (problems.length) {
    console.error(problems.join("\n"));
    return 1;
  }
  console.log("i18n OK (ar/fr/en in sync, UI, documents and help)");
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = main(process.argv[2] ? path.resolve(process.argv[2]) : repoRoot);
}

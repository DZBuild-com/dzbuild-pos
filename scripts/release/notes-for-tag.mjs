// The body of a GitHub release (release.yml): the user-facing notes of src/release-notes/<v>.json
// in English, then French and Arabic, the files to download, then the CHANGELOG.md section.
// Usage: node scripts/release/notes-for-tag.mjs [v<x.y.z>] [--out <file>]
//   The tag defaults to the version of the tree (pnpm release:version --check).
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { archLayout, ARCHES } from "./build.mjs";
import { readVersions, SEMVER } from "./version.mjs";

/** `v1.0.0`, `1.0.0` or `refs/tags/v1.0.0` → `1.0.0`. */
export function versionFromTag(tag) {
  const version = String(tag ?? "")
    .replace(/^refs\/tags\//, "")
    .replace(/^v/, "");
  if (!SEMVER.test(version)) throw new Error(`${tag} is not a version tag (v<x.y.z>)`);
  return version;
}

/** The text under `## <version>` in CHANGELOG.md, up to the next `## ` heading. */
export function changelogSection(changelog, version) {
  const lines = changelog.split(/\r?\n/);
  const heading = (line) => line.startsWith("## ");
  const start = lines.findIndex((line) => heading(line) && line.slice(3).trim().split(/\s/)[0] === version);
  if (start < 0) throw new Error(`CHANGELOG.md has no section for ${version}`);
  const rest = lines.slice(start + 1);
  const end = rest.findIndex(heading);
  return (end < 0 ? rest : rest.slice(0, end)).join("\n").trim();
}

const ARCH_LABELS = { x64: "64-bit", x86: "32-bit" };

/** The Markdown body of the release of `version`. */
export function renderReleaseNotes({ version, notes, changelog }) {
  for (const lang of ["en", "fr", "ar"]) {
    if (!notes?.[lang]?.length) {
      throw new Error(`${lang} release notes are empty (src/release-notes/${version}.json)`);
    }
  }
  const bullets = (lang) => notes[lang].map((line) => `- ${line}`).join("\n");
  const files = Object.keys(ARCHES).flatMap((arch) => {
    const { setup, offlineSetup } = archLayout(".", version, arch);
    const label = ARCH_LABELS[arch];
    return [
      `| \`${setup}\` | Windows 10/11 ${label}, small download (installs WebView2 when missing) |`,
      `| \`${offlineSetup}\` | Windows 10/11 ${label}, installs with no Internet (WebView2 inside) |`,
    ];
  });
  return [
    `## DZBuild POS ${version}`,
    "",
    bullets("en"),
    "",
    "<details>",
    "<summary>Français</summary>",
    "",
    bullets("fr"),
    "",
    "</details>",
    "",
    "<details>",
    "<summary>العربية</summary>",
    "",
    '<div dir="rtl">',
    "",
    bullets("ar"),
    "",
    "</div>",
    "</details>",
    "",
    "### Download",
    "",
    "| File | For |",
    "|---|---|",
    ...files,
    "",
    "Check a download against `SHA256SUMS.txt` (PowerShell: `Get-FileHash <file> -Algorithm SHA256`).",
    "Download page, help and support: https://dzbuild.com/pos",
    "",
    "### Changelog",
    "",
    // One level down: its "### Features" sit under this "### Changelog".
    changelogSection(changelog, version).replace(/^(#{2,5}) /gm, "#$1 "),
    "",
  ].join("\n");
}

function flag(argv, name) {
  const i = argv.indexOf(name);
  if (i < 0) return null;
  const value = argv[i + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(`${name} needs a value`);
  return value;
}

export function main(root = process.cwd(), argv = process.argv.slice(2)) {
  const out = flag(argv, "--out");
  const tag = argv.find((arg, i) => !arg.startsWith("--") && argv[i - 1] !== "--out");
  const version = tag ? versionFromTag(tag) : readVersions(root).cargo;
  const notes = JSON.parse(readFileSync(path.join(root, "src", "release-notes", `${version}.json`), "utf8"));
  const changelog = readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
  const body = renderReleaseNotes({ version, notes, changelog });
  if (out) writeFileSync(out, body);
  else process.stdout.write(body);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

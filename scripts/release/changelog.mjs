// CHANGELOG.md from conventional commits (public, English), plus the empty user-facing notes
// file src/release-notes/<version>.json that a person fills in Arabic, French and English.
// Usage: pnpm release:changelog <version> [--from <tag>]
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { SEMVER } from "./version.mjs";

const SEP = "\u001f";
const END = "\u001e";
// docs, test, chore, ci, build and style are left out of the public changelog.
const TITLES = { feat: "Features", fix: "Fixes", perf: "Performance", refactor: "Other changes" };

export function parseCommits(log) {
  return log
    .split(END)
    .map((raw) => raw.trim())
    .filter(Boolean)
    .map((raw) => {
      const [hash, subject = "", body = ""] = raw.split(SEP);
      const m = /^(\w+)(?:\(([^)]+)\))?(!)?: (.+)$/.exec(subject);
      if (!m) return null;
      return {
        hash,
        type: m[1],
        scope: m[2] ?? null,
        subject: m[4],
        breaking: Boolean(m[3]) || /BREAKING CHANGE/.test(body),
      };
    })
    .filter(Boolean);
}

export function renderSection(version, date, commits) {
  const lines = [`## ${version} — ${date}`, ""];
  for (const [type, title] of Object.entries(TITLES)) {
    const items = commits.filter((c) => c.type === type);
    if (!items.length) continue;
    lines.push(`### ${title}`);
    for (const c of items) {
      lines.push(`- ${c.scope ? `**${c.scope}**: ` : ""}${c.subject}${c.breaking ? " (BREAKING)" : ""}`);
    }
    lines.push("");
  }
  return lines.join("\n");
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const version = process.argv[2];
  if (!SEMVER.test(version ?? "")) {
    console.error("usage: pnpm release:changelog <x.y.z[-pre]> [--from <tag>]");
    process.exit(1);
  }
  const fromIndex = process.argv.indexOf("--from");
  const from =
    fromIndex > 0
      ? process.argv[fromIndex + 1]
      : execFileSync("git", ["describe", "--tags", "--abbrev=0"], { encoding: "utf8" }).trim();
  const log = execFileSync(
    "git",
    ["log", "--no-merges", `${from}..HEAD`, `--pretty=%H${SEP}%s${SEP}%b${END}`],
    {
      encoding: "utf8",
    },
  );
  const section = renderSection(version, new Date().toISOString().slice(0, 10), parseCommits(log));
  const current = existsSync("CHANGELOG.md")
    ? readFileSync("CHANGELOG.md", "utf8").replace(/^# Changelog\r?\n+/, "")
    : "";
  writeFileSync("CHANGELOG.md", `# Changelog\n\n${section}\n${current}`);
  const notes = `src/release-notes/${version}.json`;
  if (!existsSync(notes)) writeFileSync(notes, `${JSON.stringify({ ar: [], fr: [], en: [] }, null, 2)}\n`);
  console.log(`CHANGELOG.md updated; write the user notes in ${notes} (ar, fr, en)`);
}

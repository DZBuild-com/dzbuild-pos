import assert from "node:assert/strict";
import { test } from "node:test";
import { parseCommits, renderSection } from "./changelog.mjs";

const SEP = "\u001f";
const END = "\u001e";
const log = [
  ["a1", "feat(app): signed updater with channels", ""],
  ["a2", "fix(core): a restore keeps the damaged files", ""],
  ["a3", "perf(ui): lazy back office", ""],
  ["a4", "docs: close M5 in PROGRESS", ""],
  ["a5", "Merge branch 'feature/m6'", ""],
  ["a6", "feat(core)!: backups dated by name", "BREAKING CHANGE: old names are not listed"],
]
  .map((c) => c.join(SEP))
  .join(END);

test("conventional commits become a public changelog section", () => {
  const commits = parseCommits(log);
  assert.equal(commits.length, 5, "the merge commit is skipped");
  assert.equal(commits.find((c) => c.subject.startsWith("backups")).breaking, true);
  const section = renderSection("0.6.0", "2026-10-15", commits);
  assert.match(section, /^## 0\.6\.0 — 2026-10-15/);
  assert.match(section, /### Features\n- \*\*app\*\*: signed updater with channels/);
  assert.match(section, /### Fixes\n- \*\*core\*\*: a restore keeps the damaged files/);
  assert.match(section, /### Performance/);
  assert.match(section, /BREAKING/);
  assert.ok(!section.includes("PROGRESS"), "docs commits stay out");
});

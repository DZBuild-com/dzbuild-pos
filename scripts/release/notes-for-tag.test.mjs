import assert from "node:assert/strict";
import { test } from "node:test";
import { changelogSection, renderReleaseNotes, versionFromTag } from "./notes-for-tag.mjs";

const changelog = `# Changelog

## 1.1.0-beta.1 — 2026-11-02

### Fixes
- **till**: a beta fix

## 1.0.0 — 2026-10-09

The free LTS edition.

### Features
- **release**: a 32-bit Windows build

## 0.9.0 — 2026-10-08

The free edition.
`;
const notes = {
  fr: ["Version 1.0.0, édition gratuite."],
  ar: ["الإصدار 1.0.0، النسخة المجانية."],
  en: ["Version 1.0.0, the free edition.", "A 32-bit Windows installer is now available."],
};

test("a tag names its version, with or without the v", () => {
  assert.equal(versionFromTag("v1.0.0"), "1.0.0");
  assert.equal(versionFromTag("1.0.0"), "1.0.0");
  assert.equal(versionFromTag("refs/tags/v1.1.0-beta.1"), "1.1.0-beta.1");
  assert.throws(() => versionFromTag("main"), /not a version tag/);
});

test("the changelog section of a version stops at the next version", () => {
  const section = changelogSection(changelog, "1.0.0");
  assert.match(section, /^The free LTS edition\./);
  assert.match(section, /### Features\n- \*\*release\*\*: a 32-bit Windows build$/);
  assert.ok(!section.includes("0.9.0") && !section.includes("beta fix"));
  assert.match(changelogSection(changelog, "1.1.0-beta.1"), /a beta fix/);
  assert.throws(() => changelogSection(changelog, "2.0.0"), /CHANGELOG\.md has no section for 2\.0\.0/);
});

test("the release body puts the English notes first, then French and Arabic, the files and the changelog", () => {
  const body = renderReleaseNotes({ version: "1.0.0", notes, changelog });
  const en = body.indexOf("- Version 1.0.0, the free edition.");
  const fr = body.indexOf("- Version 1.0.0, édition gratuite.");
  const ar = body.indexOf("- الإصدار 1.0.0، النسخة المجانية.");
  const files = body.indexOf("DZBuildPOS-1.0.0-x86-offline-setup.exe");
  const log = body.indexOf("- **release**: a 32-bit Windows build");
  assert.ok(en > 0 && en < fr && fr < ar && ar < files && files < log, "sections out of order");
  assert.match(body, /<summary>Français<\/summary>/);
  assert.match(body, /<div dir="rtl">/);
  assert.match(body, /SHA256SUMS\.txt/);
  assert.match(body, /https:\/\/dzbuild\.com\/pos/);
  for (const file of [
    "DZBuildPOS-1.0.0-setup.exe",
    "DZBuildPOS-1.0.0-offline-setup.exe",
    "DZBuildPOS-1.0.0-x86-setup.exe",
  ]) {
    assert.ok(body.includes(file), file);
  }
});

test("a release without notes in one of the three languages is refused", () => {
  assert.throws(
    () => renderReleaseNotes({ version: "1.0.0", notes: { ...notes, ar: [] }, changelog }),
    /ar release notes are empty/,
  );
});

test("the changelog headings sit one level under the release's own", () => {
  const body = renderReleaseNotes({ version: "1.0.0", notes, changelog });
  assert.match(body, /\n#### Features\n- \*\*release\*\*/);
  assert.ok(!/\n### Features/.test(body));
});

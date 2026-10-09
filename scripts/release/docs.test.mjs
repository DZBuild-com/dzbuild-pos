import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";

const read = (file) => readFileSync(file, "utf8");

test("one release checklist covers install, updates, data, budgets and every milestone's smoke", () => {
  const list = read("docs/release-checklist.md");
  for (const needle of [
    "Windows 10 22H2",
    "offline-setup.exe",
    "SmartScreen",
    "7345",
    "update",
    "USB",
    "restore",
    "cold start",
    "250 MB",
    "scan-to-line",
    "uninstall",
    "m1-real-till-smoke",
    "licence",
    "activation screen",
  ]) {
    assert.ok(list.toLowerCase().includes(needle.toLowerCase()), `the checklist lacks "${needle}"`);
  }
  for (const smoke of readdirSync("docs/testing/smoke").filter((f) => f.endsWith(".md"))) {
    assert.ok(list.includes(`docs/testing/smoke/${smoke}`), `the checklist does not point to ${smoke}`);
  }
});

test("the release guide, the signing, key, hosting and SmartScreen guides, and six video scripts", () => {
  for (const doc of ["signing", "updater-key", "hosting", "smartscreen", "reference-pc"]) {
    assert.ok(existsSync(`docs/release/${doc}.md`), doc);
  }
  assert.equal(readdirSync("docs/help/videos").filter((f) => f.endsWith(".md")).length, 6);
  const guide = read("docs/release/RELEASING.md");
  for (const command of [
    "pnpm release:version",
    "pnpm release:changelog",
    "pnpm release:build",
    "pnpm release:manifest",
    "tauri signer generate",
  ]) {
    assert.ok(guide.includes(command), `RELEASING.md lacks ${command}`);
  }
  assert.ok(!read("README.md").includes("unsigned until milestone M6"));
  assert.match(
    read("docs/release/updater-key.md"),
    /src-tauri\\keys\\updater\.key|src-tauri\/keys\/updater\.key/,
  );
  assert.ok(existsSync("scripts/perf/bench.ps1") && existsSync("scripts/perf/idle-ram.ps1"));
});

test("the licensing guide covers keygen, custody, issuing, re-issue, rotation and the limits", () => {
  const guide = read("docs/release/licensing.md");
  for (const needle of [
    "dzpos-license keygen",
    "dzpos-license issue",
    "--replace",
    "two USB sticks",
    "WhatsApp",
    "REVOKED",
    "KEYRING_SHA256",
    "## Limits",
    "## Re-issue",
    "## Rotation",
  ]) {
    assert.ok(guide.includes(needle), `licensing.md lacks "${needle}"`);
  }
  assert.match(guide, /DZBuild POS licence key/);
});

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { readVersions, setVersion } from "./version.mjs";

function repo(version = "0.5.0") {
  const root = mkdtempSync(path.join(tmpdir(), "dzpos-version-"));
  mkdirSync(path.join(root, "src-tauri"));
  writeFileSync(
    path.join(root, "package.json"),
    JSON.stringify({ name: "dzbuild-pos", version }, null, 2) + "\n",
  );
  writeFileSync(
    path.join(root, "src-tauri", "tauri.conf.json"),
    JSON.stringify({ productName: "DZBuild POS", version }, null, 2) + "\n",
  );
  writeFileSync(
    path.join(root, "Cargo.toml"),
    `[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nversion = "${version}"\nedition = "2024"\n\n[workspace.dependencies]\nserde = { version = "1.0.229" }\n`,
  );
  return root;
}

test("the three version numbers move together, prereleases included", () => {
  const root = repo();
  setVersion(root, "0.6.0-beta.1");
  assert.deepEqual(readVersions(root), {
    packageJson: "0.6.0-beta.1",
    tauriConf: "0.6.0-beta.1",
    cargo: "0.6.0-beta.1",
  });
  assert.match(
    readFileSync(path.join(root, "Cargo.toml"), "utf8"),
    /serde = \{ version = "1\.0\.229" \}/,
    "dependency versions untouched",
  );
  assert.throws(() => setVersion(root, "6"), /not a semver/);
});

test("only the version line changes: the files keep their prettier layout", () => {
  const root = repo();
  const conf = path.join(root, "src-tauri", "tauri.conf.json");
  const layout = (v) =>
    `{\n  "productName": "DZBuild POS",\n  "version": "${v}",\n  "bundle": {\n    "targets": ["nsis"],\n    "windows": { "minimumWebview2Version": "120.0.2210.55" }\n  }\n}\n`;
  writeFileSync(conf, layout("0.5.0"));
  setVersion(root, "0.6.0");
  assert.equal(readFileSync(conf, "utf8"), layout("0.6.0"));
});

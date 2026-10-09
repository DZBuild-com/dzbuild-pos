import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { collectArtifacts, parseSums } from "./collect.mjs";

const h = (c) => c.repeat(64);

function archArtifact(root, name, files, sums) {
  const dir = path.join(root, name, "1.0.0");
  mkdirSync(dir, { recursive: true });
  for (const file of files) writeFileSync(path.join(dir, file), file);
  writeFileSync(path.join(dir, "SHA256SUMS.txt"), sums);
  return path.join(root, name);
}

test("the per-arch artifacts merge into one release folder with one SHA256SUMS.txt", () => {
  const root = mkdtempSync(path.join(tmpdir(), "dzpos-collect-"));
  const x64 = archArtifact(
    root,
    "installers-x64",
    ["DZBuildPOS-1.0.0-setup.exe", "DZBuildPOS-1.0.0-setup.exe.sig", "DZBuildPOS-1.0.0-offline-setup.exe"],
    `${h("a")}  DZBuildPOS-1.0.0-setup.exe\n${h("b")}  DZBuildPOS-1.0.0-offline-setup.exe\n`,
  );
  const x86 = archArtifact(
    root,
    "installers-x86",
    ["DZBuildPOS-1.0.0-x86-setup.exe", "DZBuildPOS-1.0.0-x86-offline-setup.exe"],
    `${h("c")}  DZBuildPOS-1.0.0-x86-setup.exe\n${h("d")}  DZBuildPOS-1.0.0-x86-offline-setup.exe\n`,
  );
  const out = path.join(root, "dist-release");
  const files = collectArtifacts({ version: "1.0.0", sources: [x64, x86], out });
  const dir = path.join(out, "1.0.0");
  assert.deepEqual(readdirSync(dir).sort(), [
    "DZBuildPOS-1.0.0-offline-setup.exe",
    "DZBuildPOS-1.0.0-setup.exe",
    "DZBuildPOS-1.0.0-setup.exe.sig",
    "DZBuildPOS-1.0.0-x86-offline-setup.exe",
    "DZBuildPOS-1.0.0-x86-setup.exe",
    "SHA256SUMS.txt",
  ]);
  assert.equal(files.length, 6);
  assert.equal(
    readFileSync(path.join(dir, "SHA256SUMS.txt"), "utf8"),
    [
      `${h("a")}  DZBuildPOS-1.0.0-setup.exe`,
      `${h("b")}  DZBuildPOS-1.0.0-offline-setup.exe`,
      `${h("c")}  DZBuildPOS-1.0.0-x86-setup.exe`,
      `${h("d")}  DZBuildPOS-1.0.0-x86-offline-setup.exe`,
      "",
    ].join("\n"),
  );
});

test("an artifact of another version, or none at all, stops the release", () => {
  const root = mkdtempSync(path.join(tmpdir(), "dzpos-collect-"));
  const x64 = archArtifact(root, "installers-x64", ["DZBuildPOS-1.0.0-setup.exe"], "");
  assert.throws(
    () => collectArtifacts({ version: "1.0.1", sources: [x64], out: path.join(root, "out") }),
    /installers-x64 has no 1\.0\.1 folder/,
  );
  assert.throws(
    () => collectArtifacts({ version: "1.0.0", sources: [], out: path.join(root, "out") }),
    /no installer artifact/,
  );
});

test("SHA256SUMS.txt lines read in both the text and the binary (*) form", () => {
  assert.deepEqual(parseSums(`${h("A")}  a.exe\r\n${h("b")} *b.exe\n\nnot a line\n`), [
    { hash: h("a"), name: "a.exe" },
    { hash: h("b"), name: "b.exe" },
  ]);
});

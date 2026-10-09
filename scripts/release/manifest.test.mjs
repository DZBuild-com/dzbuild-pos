import assert from "node:assert/strict";
import { test } from "node:test";
import { buildManifest, channelsFor, checkSignature, PLATFORM_KEYS } from "./manifest.mjs";

const sig = (comment) =>
  Buffer.from(
    `untrusted comment: signature from tauri secret key\nRUQ…\ntrusted comment: ${comment}\nAAA…\n`,
  ).toString("base64");
const notes = { ar: ["تصحيح"], fr: ["Correction"], en: ["Fix"] };

test("stable versions go to both channels, prereleases to beta only", () => {
  assert.deepEqual(channelsFor("0.6.0"), ["stable", "beta"]);
  assert.deepEqual(channelsFor("0.7.0-beta.2"), ["beta"]);
});

test("the manifest refuses a signature without the version", () => {
  checkSignature(sig("timestamp:1790593200\tfile:DZBuild POS_0.6.0_x64-setup.exe\tversion:0.6.0"), "0.6.0");
  assert.throws(
    () => checkSignature(sig("timestamp:1790593200\tfile:x.exe"), "0.6.0"),
    /does not carry version:0\.6\.0/,
  );
  assert.throws(
    () => checkSignature(sig("timestamp:1\tversion:0.5.9"), "0.6.0"),
    /does not carry version:0\.6\.0/,
  );
});

test("the manifest carries the Tauri fields and DZBuild's extras", () => {
  const manifest = buildManifest({
    ...manifestInput(),
    setups: { x64: { ...x64Setup(), size: 7_340_032 } },
    rollout: 10,
    minimumVersion: "0.5.0",
  });
  assert.equal(manifest.version, "0.6.0");
  assert.equal(manifest.notes, "Correction");
  assert.deepEqual(manifest.notes_i18n, { ar: "تصحيح", fr: "Correction", en: "Fix" });
  assert.deepEqual(Object.keys(manifest.platforms).sort(), ["windows-x86_64", "windows-x86_64-nsis"]);
  for (const key of ["windows-x86_64-nsis", "windows-x86_64"]) {
    assert.equal(manifest.platforms[key].url.startsWith("https://"), true);
    assert.equal(manifest.platforms[key].size, 7_340_032);
  }
  assert.equal(manifest.rollout, 10);
  assert.equal(manifest.minimum_version, "0.5.0");
  assert.throws(
    () => buildManifest({ ...manifestInput(), notes: { ar: [], fr: ["x"], en: ["y"] } }),
    /ar release notes are empty/,
  );
  assert.throws(
    () =>
      buildManifest({
        ...manifestInput(),
        setups: { x64: { ...x64Setup(), url: "http://dzbuild.com/x.exe" } },
      }),
    /https/,
  );
});

test("each arch gets its own entries, url and signature (1.0.0, 32-bit)", () => {
  assert.deepEqual(PLATFORM_KEYS, {
    x64: ["windows-x86_64-nsis", "windows-x86_64"],
    x86: ["windows-i686-nsis", "windows-i686"],
  });
  const manifest = buildManifest({
    ...manifestInput(),
    version: "1.0.0",
    setups: {
      x64: {
        sig: sig("timestamp:1\tfile:DZBuild POS_1.0.0_x64-setup.exe\tversion:1.0.0"),
        url: "https://dzbuild.com/pos/download/1.0.0/DZBuildPOS-1.0.0-setup.exe",
        sha256: "ab".repeat(32),
        size: 11,
      },
      x86: {
        sig: sig("timestamp:1\tfile:DZBuild POS_1.0.0_x86-setup.exe\tversion:1.0.0"),
        url: "https://dzbuild.com/pos/download/1.0.0/DZBuildPOS-1.0.0-x86-setup.exe",
        sha256: "cd".repeat(32),
        size: 10,
      },
    },
  });
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [
    "windows-i686",
    "windows-i686-nsis",
    "windows-x86_64",
    "windows-x86_64-nsis",
  ]);
  for (const key of ["windows-x86_64", "windows-x86_64-nsis"]) {
    assert.match(manifest.platforms[key].url, /DZBuildPOS-1\.0\.0-setup\.exe$/);
    assert.equal(manifest.platforms[key].sha256, "ab".repeat(32));
  }
  for (const key of ["windows-i686", "windows-i686-nsis"]) {
    assert.match(manifest.platforms[key].url, /DZBuildPOS-1\.0\.0-x86-setup\.exe$/);
    assert.equal(manifest.platforms[key].sha256, "cd".repeat(32));
    assert.equal(manifest.platforms[key].size, 10);
  }
  assert.notEqual(
    manifest.platforms["windows-i686"].signature,
    manifest.platforms["windows-x86_64"].signature,
  );

  const x86Only = buildManifest({
    ...manifestInput(),
    version: "1.0.0",
    setups: {
      x86: {
        sig: sig("timestamp:1\tversion:1.0.0"),
        url: "https://dzbuild.com/pos/download/1.0.0/DZBuildPOS-1.0.0-x86-setup.exe",
        sha256: "cd".repeat(32),
        size: 10,
      },
    },
  });
  assert.deepEqual(Object.keys(x86Only.platforms).sort(), ["windows-i686", "windows-i686-nsis"]);

  assert.throws(() => buildManifest({ ...manifestInput(), setups: {} }), /nothing to publish/);
  assert.throws(
    () => buildManifest({ ...manifestInput(), setups: { arm64: x64Setup() } }),
    /unknown arch arm64/,
  );
  assert.throws(
    () =>
      buildManifest({
        ...manifestInput(),
        setups: { x64: x64Setup(), x86: { ...x64Setup(), sig: sig("timestamp:1\tversion:0.5.9") } },
      }),
    /does not carry version:0\.6\.0/,
    "each arch's signature is checked",
  );
});

test("a rollout the app cannot read as a whole percentage is refused, not widened to everyone", () => {
  // pos_core::updates reads `rollout` as an unsigned integer and falls back to 100 % otherwise.
  for (const rollout of [10.5, -1, 101, Number.NaN]) {
    assert.throws(() => buildManifest({ ...manifestInput(), rollout }), /rollout/);
  }
  assert.equal(buildManifest({ ...manifestInput(), rollout: 0 }).rollout, 0);
  assert.throws(() => buildManifest({ ...manifestInput(), minimumVersion: "0.5" }), /minimum version/);
});

function x64Setup() {
  return {
    sig: sig("timestamp:1\tversion:0.6.0"),
    url: "https://dzbuild.com/pos/download/0.6.0/DZBuildPOS-0.6.0-setup.exe",
    sha256: "ab".repeat(32),
    size: 1,
  };
}

function manifestInput() {
  return {
    version: "0.6.0",
    notes,
    setups: { x64: x64Setup() },
    pubDate: "2026-10-15T09:00:00Z",
    rollout: 100,
    paused: false,
    minimumVersion: null,
  };
}

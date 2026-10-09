import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import {
  archLayout,
  BUILD_TRIES,
  isFreeEdition,
  isTransientLockError,
  mergeSums,
  ownerToolBuild,
  parseArch,
  RETRY_DELAY_MS,
  releaseConfig,
  sellerContactProblem,
  tauriBuild,
  tauriBuildCommand,
} from "./build.mjs";

test("updater artifacts only with a key, signing only when configured, the offline mode merged", () => {
  assert.deepEqual(releaseConfig({ updater: false, signScript: null, offline: false }), {
    bundle: { createUpdaterArtifacts: false },
  });
  const signed = releaseConfig({
    updater: true,
    signScript: "C:\\repo\\scripts\\release\\sign.mjs",
    offline: false,
  });
  assert.equal(signed.bundle.createUpdaterArtifacts, true);
  assert.deepEqual(signed.bundle.windows.signCommand, {
    cmd: "node",
    args: ["C:\\repo\\scripts\\release\\sign.mjs", "%1"],
  });
  const offline = releaseConfig({ updater: true, signScript: null, offline: true });
  assert.equal(offline.bundle.createUpdaterArtifacts, false, "the USB setup is never an update artifact");
  assert.deepEqual(offline.bundle.windows.webviewInstallMode, { type: "offlineInstaller", silent: true });
});

test("a release needs the seller's WhatsApp or phone in license.toml (M9 final review)", () => {
  const toml = (whatsapp, phone) =>
    `# who sells\nseller = "DZBuild"\n# whatsapp = "commented"\nwhatsapp = "${whatsapp}"\nphone = "${phone}"\nurl = "https://dzbuild.com/pos"\n`;
  assert.match(sellerContactProblem(toml("", "")), /neither whatsapp nor phone/);
  assert.match(sellerContactProblem(toml("  ", "")), /neither whatsapp nor phone/, "blank is empty");
  assert.equal(sellerContactProblem(toml("213550123456", "")), null);
  assert.equal(sellerContactProblem(toml("", "0550 12 34 56")), null);
  assert.equal(
    sellerContactProblem(`free = true
${toml("", "")}`),
    null,
    "the free edition needs no contact",
  );
  assert.equal(sellerContactProblem(toml("213550123456", "0550 12 34 56").replace(/\n/g, "\r\n")), null);
  assert.match(sellerContactProblem('seller = "DZBuild"\n'), /neither/, "absent keys");
});

test("the owner's licence tool is built beside the release, never into it", () => {
  const tool = ownerToolBuild("C:\\repo", "0.9.0");
  assert.deepEqual(tool.command, ["cargo", ["build", "--release", "--locked", "-p", "pos-license-cli"]]);
  assert.equal(tool.from, path.join("C:\\repo", "target", "release", "dzpos-license.exe"));
  assert.equal(tool.to, path.join("C:\\repo", "dist-owner", "0.9.0", "dzpos-license.exe"));
  assert.ok(!tool.to.includes("dist-release"), "dist-release is what gets published");
});

test("--arch picks x64 by default or x86, and nothing else", () => {
  assert.equal(parseArch([]), "x64");
  assert.equal(parseArch(["--arch", "x64"]), "x64");
  assert.equal(parseArch(["--arch", "x86"]), "x86");
  assert.throws(() => parseArch(["--arch", "arm64"]), /x64 or x86/);
  assert.throws(() => parseArch(["--arch"]), /x64 or x86/);
});

test("x86 builds i686-pc-windows-msvc under its own target folder and names; x64 names stay", () => {
  assert.equal(
    tauriBuildCommand("C:\\repo\\online.json", "x64"),
    'pnpm tauri build --bundles nsis --config "C:\\repo\\online.json"',
  );
  assert.equal(
    tauriBuildCommand("C:\\repo\\online.json", "x86"),
    'pnpm tauri build --bundles nsis --config "C:\\repo\\online.json" --target i686-pc-windows-msvc',
  );
  const x64 = archLayout("C:\\repo", "1.0.0", "x64");
  assert.equal(
    x64.built,
    path.join("C:\\repo", "target", "release", "bundle", "nsis", "DZBuild POS_1.0.0_x64-setup.exe"),
  );
  assert.equal(x64.setup, "DZBuildPOS-1.0.0-setup.exe");
  assert.equal(x64.offlineSetup, "DZBuildPOS-1.0.0-offline-setup.exe");
  const x86 = archLayout("C:\\repo", "1.0.0", "x86");
  assert.equal(
    x86.built,
    path.join(
      "C:\\repo",
      "target",
      "i686-pc-windows-msvc",
      "release",
      "bundle",
      "nsis",
      "DZBuild POS_1.0.0_x86-setup.exe",
    ),
  );
  assert.equal(x86.setup, "DZBuildPOS-1.0.0-x86-setup.exe");
  assert.equal(x86.offlineSetup, "DZBuildPOS-1.0.0-x86-offline-setup.exe");
});

test("both arches merge their lines into one SHA256SUMS.txt", () => {
  const h = (c) => c.repeat(64);
  const x64 = mergeSums("", [
    { hash: h("a"), name: "DZBuildPOS-1.0.0-setup.exe" },
    { hash: h("b"), name: "DZBuildPOS-1.0.0-offline-setup.exe" },
  ]);
  assert.equal(x64, `${h("a")}  DZBuildPOS-1.0.0-setup.exe\n${h("b")}  DZBuildPOS-1.0.0-offline-setup.exe\n`);
  const both = mergeSums(x64.replace(/\n/g, "\r\n"), [
    { hash: h("c"), name: "DZBuildPOS-1.0.0-x86-setup.exe" },
    { hash: h("d"), name: "DZBuildPOS-1.0.0-x86-offline-setup.exe" },
  ]);
  assert.equal(both.trim().split("\n").length, 4, "the x64 lines are kept");
  assert.ok(both.startsWith(x64));
  const rebuilt = mergeSums(both, [{ hash: h("e"), name: "DZBuildPOS-1.0.0-setup.exe" }]);
  assert.equal(rebuilt.trim().split("\n").length, 4, "a rebuilt setup replaces its own line");
  assert.ok(rebuilt.startsWith(`${h("e")}  DZBuildPOS-1.0.0-setup.exe\n`));
  assert.ok(!rebuilt.includes(h("a")));
  assert.equal(mergeSums("not a sums line\n", []), "", "junk is dropped");
});

test("a locked exe is tried again up to five times, 3 s apart; any other failure stops at once", async () => {
  assert.equal(BUILD_TRIES, 5);
  assert.equal(RETRY_DELAY_MS, 3000);
  const locked =
    "failed to bundle project: can't open main binary C:\\repo\\target\\i686-pc-windows-msvc\\release\\dzbuild-pos.exe: Access is denied. (os error 5)";
  assert.ok(isTransientLockError(locked));
  assert.ok(isTransientLockError("LINK : fatal error LNK1104: cannot open file 'dzbuild_pos.exe'"));
  assert.ok(!isTransientLockError("error[E0308]: mismatched types"));

  const fake = (results) => {
    const calls = [];
    const run = async (command) => {
      calls.push(command);
      return results.shift();
    };
    return { calls, run };
  };
  const waits = [];
  const wait = async (ms) => waits.push(ms);

  const twice = fake([
    { status: 1, output: locked },
    { status: 1, output: locked },
    { status: 0, output: "" },
  ]);
  assert.equal(await tauriBuild("c.json", "x86", {}, twice.run, wait), 3);
  assert.deepEqual(waits, [3000, 3000]);
  assert.ok(twice.calls.every((c) => c.endsWith("--target i686-pc-windows-msvc")));

  const always = fake(Array.from({ length: 9 }, () => ({ status: 1, output: locked })));
  await assert.rejects(tauriBuild("c.json", "x86", {}, always.run, wait), /after 5 tries/);
  assert.equal(always.calls.length, 5);

  const broken = fake([{ status: 1, output: "error[E0308]: mismatched types" }]);
  await assert.rejects(tauriBuild("c.json", "x64", {}, broken.run, wait), /tauri build failed$/);
  assert.equal(broken.calls.length, 1);
});

test("the free edition is read from license.toml (no owner tool, no sales contact)", () => {
  assert.equal(isFreeEdition('seller = "DZBuild"\nfree = true\n'), true);
  assert.equal(isFreeEdition('seller = "DZBuild"\r\nfree = true # 0.9.0\r\n'), true);
  assert.equal(isFreeEdition("free = false\n"), false);
  assert.equal(isFreeEdition('# free = true\nseller = "DZBuild"\n'), false);
  assert.equal(
    isFreeEdition(readFileSync("src-tauri/license.toml", "utf8")),
    true,
    "1.0.0 is the free edition",
  );
  assert.equal(sellerContactProblem(readFileSync("src-tauri/license.toml", "utf8")), null);
});

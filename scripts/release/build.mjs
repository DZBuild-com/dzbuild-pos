// Both installers of a release into dist-release/<version>/ (spec §13, NFR-002):
//   DZBuildPOS-<v>-setup.exe (+ .sig when the updater key is available) ≤ 15 MiB,
//   DZBuildPOS-<v>-offline-setup.exe (WebView2 runtime inside) ≥ 100 MiB, SHA256SUMS.txt.
// Usage: pnpm release:build [--arch x64|x86]. x64 is the default; `--arch x86` (1.0.0) builds
// i686-pc-windows-msvc (`rustup target add i686-pc-windows-msvc` once) into
// DZBuildPOS-<v>-x86-setup.exe and DZBuildPOS-<v>-x86-offline-setup.exe. Both arches merge their
// lines into the one SHA256SUMS.txt of dist-release/<version>/.
// The updater key comes from TAURI_SIGNING_PRIVATE_KEY (CI secret) or src-tauri/keys/ (the
// owner's PC); it is never printed. Signing runs when DZPOS_SIGN_PROVIDER is set.
// The offline mode below is the one src-tauri/tauri.offline.conf.json holds (both are tested).
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { readVersions } from "./version.mjs";

const MiB = 1024 * 1024;

/** The Windows arches a release is built for: the Rust target (none: the host's x64), the arch
 * label tauri's NSIS bundler puts in its file name, and the suffix of the published names. */
export const ARCHES = {
  x64: { target: null, bundleLabel: "x64", suffix: "" },
  x86: { target: "i686-pc-windows-msvc", bundleLabel: "x86", suffix: "-x86" },
};

/** `--arch x64|x86` from the command line, x64 when absent. */
export function parseArch(argv) {
  const i = argv.indexOf("--arch");
  if (i < 0) return "x64";
  const arch = argv[i + 1];
  if (!Object.hasOwn(ARCHES, arch ?? "")) {
    throw new Error(`--arch takes x64 or x86, not ${arch ?? "nothing"}`);
  }
  return arch;
}

/** Where tauri leaves the NSIS setup of `arch`, and the names it is published under. */
export function archLayout(root, version, arch) {
  const { target, bundleLabel, suffix } = ARCHES[arch];
  const release = target
    ? path.join(root, "target", target, "release")
    : path.join(root, "target", "release");
  const bundleDir = path.join(release, "bundle", "nsis");
  return {
    bundleDir,
    built: path.join(bundleDir, `DZBuild POS_${version}_${bundleLabel}-setup.exe`),
    setup: `DZBuildPOS-${version}${suffix}-setup.exe`,
    offlineSetup: `DZBuildPOS-${version}${suffix}-offline-setup.exe`,
  };
}

/** The `pnpm tauri build` command line of one setup. */
export function tauriBuildCommand(configFile, arch) {
  const { target } = ARCHES[arch];
  return `pnpm tauri build --bundles nsis --config "${configFile}"${target ? ` --target ${target}` : ""}`;
}

/** SHA256SUMS.txt with `entries` ({ name, hash }) merged in: a line of another file (the other
 * arch) is kept in place, a line of the same file is replaced, a new file goes at the end. */
export function mergeSums(existing, entries) {
  const lines = (existing ?? "")
    .split(/\r?\n/)
    .map((line) => /^([0-9a-fA-F]{64})\s+\*?(.+)$/.exec(line.trim()))
    .filter(Boolean)
    .map((match) => ({ hash: match[1].toLowerCase(), name: match[2] }));
  for (const entry of entries) {
    const at = lines.findIndex((line) => line.name === entry.name);
    if (at >= 0) lines[at] = entry;
    else lines.push(entry);
  }
  return lines.map((line) => `${line.hash}  ${line.name}\n`).join("");
}

/** Windows' "Access is denied" on the exe just linked (an antivirus scanning it), seen as
 * tauri's "can't open main binary … (os error 5)" or the linker's LNK1104: worth another try. */
export function isTransientLockError(output) {
  return /os error 5\b|Access is denied|LNK1104/i.test(output);
}

export const BUILD_TRIES = 5;
export const RETRY_DELAY_MS = 3000;

export function releaseConfig({ updater, signScript, offline }) {
  const config = { bundle: { createUpdaterArtifacts: updater && !offline } };
  if (signScript || offline) config.bundle.windows = {};
  if (signScript) config.bundle.windows.signCommand = { cmd: "node", args: [signScript, "%1"] };
  if (offline) config.bundle.windows.webviewInstallMode = { type: "offlineInstaller", silent: true };
  return config;
}

/** The owner's licence tool (M9): built with the release into dist-owner/, never into an installer
 * nor into dist-release/ (what gets published). Not built for the free edition (1.0.0). */
export function ownerToolBuild(root, version) {
  return {
    command: ["cargo", ["build", "--release", "--locked", "-p", "pos-license-cli"]],
    from: path.join(root, "target", "release", "dzpos-license.exe"),
    to: path.join(root, "dist-owner", version, "dzpos-license.exe"),
  };
}

/** The value of `key = "…"` in the TOML text `toml` (top level, one line), "" when absent. */
function tomlString(toml, key) {
  const pattern = new RegExp(`^${key}\\s*=\\s*"([^"]*)"`);
  for (const line of toml.split(/\r?\n/)) {
    const match = line.trim().match(pattern);
    if (match) return match[1].trim();
  }
  return "";
}

/** The free edition (0.9.0): `free = true` in src-tauri/license.toml. */
export function isFreeEdition(toml) {
  return toml.split(/\r?\n/).some((line) => /^free\s*=\s*true\b/.test(line.trim()));
}

/** M9 final review: a release names the seller's WhatsApp or phone in src-tauri/license.toml, so
 * that a new customer's activation screen offers a way to buy (the QR opens a WhatsApp chat with the
 * device code). `null` when it does, else why the build stops. */
export function sellerContactProblem(toml) {
  // 0.9.0: the free edition never shows the activation screen, so no contact is needed.
  if (isFreeEdition(toml)) return null;
  if (tomlString(toml, "whatsapp") || tomlString(toml, "phone")) return null;
  return "src-tauri/license.toml has neither whatsapp nor phone: put the sales number there before building a release, or every customer's activation screen offers only a web address";
}

/** The Authenticode status of `file` (`Valid`, `NotSigned`, `HashMismatch`…), from PowerShell. */
export function authenticodeStatus(file) {
  // Windows PowerShell 5.1 started from a PowerShell 7 session inherits its PSModulePath and then
  // fails to load Microsoft.PowerShell.Security: it gets its own default module path instead.
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([name]) => name.toUpperCase() !== "PSMODULEPATH"),
  );
  return execFileSync(
    "powershell",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      "(Get-AuthenticodeSignature -LiteralPath $env:DZPOS_CHECK_FILE).Status",
    ],
    { encoding: "utf8", env: { ...env, DZPOS_CHECK_FILE: file } },
  ).trim();
}

/** One `pnpm tauri build`, its output shown live and its tail kept for `isTransientLockError`. */
function runOnce(command, env) {
  return new Promise((resolve) => {
    // One quoted command line: pnpm is a .cmd shim that only runs through the shell.
    const child = spawn(command, { env, shell: true, stdio: ["inherit", "pipe", "pipe"] });
    let tail = "";
    const keep = (chunk, sink) => {
      sink.write(chunk);
      tail = (tail + chunk.toString()).slice(-64 * 1024);
    };
    child.stdout.on("data", (chunk) => keep(chunk, process.stdout));
    child.stderr.on("data", (chunk) => keep(chunk, process.stderr));
    child.on("error", (error) => resolve({ status: 1, output: `${tail}\n${error.message}` }));
    child.on("close", (status) => resolve({ status, output: tail }));
  });
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** `pnpm tauri build`, run again (up to BUILD_TRIES, RETRY_DELAY_MS apart) when Windows refused the
 * freshly linked exe (1.0.0, the first i686 build); cargo has nothing left to compile then, so a
 * retry only links and bundles. Any other failure stops at once. Resolves to the tries it took. */
export async function tauriBuild(configFile, arch, env, run = runOnce, wait = delay) {
  const command = tauriBuildCommand(configFile, arch);
  for (let attempt = 1; ; attempt += 1) {
    const { status, output } = await run(command, env);
    if (status === 0) return attempt;
    if (attempt >= BUILD_TRIES || !isTransientLockError(output)) {
      throw new Error(`tauri build failed${attempt > 1 ? ` after ${attempt} tries` : ""}`);
    }
    console.log(
      `tauri build: the exe was locked (try ${attempt} of ${BUILD_TRIES}), again in ${RETRY_DELAY_MS / 1000} s`,
    );
    await wait(RETRY_DELAY_MS);
  }
}

/** The setup tauri just built, or an error naming what its bundle folder holds instead. */
function builtSetup(layout) {
  if (existsSync(layout.built)) return layout.built;
  const found = existsSync(layout.bundleDir) ? readdirSync(layout.bundleDir).join(", ") : "nothing";
  throw new Error(`${layout.built} is missing (the folder holds: ${found || "nothing"})`);
}

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

export async function main(root = process.cwd(), argv = process.argv.slice(2)) {
  const arch = parseArch(argv);
  const versions = readVersions(root);
  if (new Set(Object.values(versions)).size !== 1) {
    throw new Error(`versions differ: ${JSON.stringify(versions)}`);
  }
  const version = versions.cargo;
  const licenseToml = readFileSync(path.join(root, "src-tauri", "license.toml"), "utf8");
  const contact = sellerContactProblem(licenseToml);
  if (contact) throw new Error(contact);
  const env = { ...process.env };
  const keyFile = path.join(root, "src-tauri", "keys", "updater.key");
  if (!env.TAURI_SIGNING_PRIVATE_KEY && existsSync(keyFile)) {
    env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(keyFile, "utf8").trim();
    const pw = path.join(root, "src-tauri", "keys", "updater.password");
    if (!env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD && existsSync(pw)) {
      env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = readFileSync(pw, "utf8").trim();
    }
  }
  const updater = Boolean(env.TAURI_SIGNING_PRIVATE_KEY);
  const signScript = env.DZPOS_SIGN_PROVIDER ? path.join(root, "scripts", "release", "sign.mjs") : null;
  console.log(
    `release ${version} ${arch}: updater artifacts ${updater ? "on" : "OFF (no key)"}, signing ${signScript ? env.DZPOS_SIGN_PROVIDER : "OFF (unsigned build)"}`,
  );
  const configDir = path.join(root, "target", "release-config");
  mkdirSync(configDir, { recursive: true });
  const out = path.join(root, "dist-release", version);
  mkdirSync(out, { recursive: true });
  const layout = archLayout(root, version, arch);

  const online = path.join(configDir, "online.json");
  writeFileSync(online, JSON.stringify(releaseConfig({ updater, signScript, offline: false })));
  await tauriBuild(online, arch, env);
  const built = builtSetup(layout);
  const setup = path.join(out, layout.setup);
  copyFileSync(built, setup);
  // A .sig left by an earlier run would not match this setup.
  if (updater) copyFileSync(`${built}.sig`, `${setup}.sig`);
  else rmSync(`${setup}.sig`, { force: true });
  if (statSync(setup).size > 15 * MiB) {
    throw new Error(
      `the online setup is ${(statSync(setup).size / MiB).toFixed(1)} MiB, over 15 MiB (NFR-002)`,
    );
  }

  const offlineConfig = path.join(configDir, "offline.json");
  writeFileSync(offlineConfig, JSON.stringify(releaseConfig({ updater, signScript, offline: true })));
  await tauriBuild(offlineConfig, arch, env);
  const offlineSetup = path.join(out, layout.offlineSetup);
  copyFileSync(builtSetup(layout), offlineSetup);
  if (statSync(offlineSetup).size < 100 * MiB) {
    throw new Error("the offline setup is under 100 MiB: the WebView2 runtime is missing");
  }

  // One SHA256SUMS.txt per release: the other arch's lines stay, this arch's are replaced.
  const sumsFile = path.join(out, "SHA256SUMS.txt");
  const sums = mergeSums(
    existsSync(sumsFile) ? readFileSync(sumsFile, "utf8") : "",
    [setup, offlineSetup].map((f) => ({ hash: sha256(f), name: path.basename(f) })),
  );
  writeFileSync(sumsFile, sums);
  if (signScript) {
    for (const f of [setup, offlineSetup]) {
      const status = authenticodeStatus(f);
      if (status !== "Valid") throw new Error(`${path.basename(f)}: Authenticode ${status}`);
    }
  }
  if (isFreeEdition(licenseToml)) {
    console.log("owner tool: not built (free edition, no licence to issue)");
  } else {
    const tool = ownerToolBuild(root, version);
    const toolRun = spawnSync(tool.command[0], tool.command[1], { stdio: "inherit", env: process.env });
    if (toolRun.status !== 0) throw new Error("the owner's licence tool did not build");
    mkdirSync(path.dirname(tool.to), { recursive: true });
    copyFileSync(tool.from, tool.to);
    console.log(`owner tool: ${tool.to} (keep it with the licence key; never publish it)`);
  }
  console.log(`done: ${out}\n${sums}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}

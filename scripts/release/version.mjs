// One version for package.json, src-tauri/tauri.conf.json and Cargo.toml [workspace.package].
// Usage: pnpm release:version <x.y.z[-pre]> | --check. Refresh Cargo.lock afterwards with
// `cargo metadata --format-version 1 > $null` (the lockfile is never edited by hand).
// Only the version line of each file changes, so the files keep their prettier layout.
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

const JSON_FILES = ["package.json", path.join("src-tauri", "tauri.conf.json")];
// The top-level key of a two-space indented JSON file (nested keys sit deeper).
const JSON_VERSION = /^( {2}"version":\s*)"[^"]*"/m;

export function readVersions(root) {
  const cargo = readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const block = cargo.split("[workspace.package]")[1] ?? "";
  return {
    packageJson: JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version,
    tauriConf: JSON.parse(readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8")).version,
    cargo: /^version = "([^"]+)"/m.exec(block.split("\n[")[0])?.[1],
  };
}

export function setVersion(root, version) {
  if (!SEMVER.test(version)) throw new Error(`${version} is not a semver version`);
  for (const file of JSON_FILES) {
    const full = path.join(root, file);
    const text = readFileSync(full, "utf8");
    if (!JSON_VERSION.test(text)) throw new Error(`${file} has no top-level "version" line`);
    const updated = text.replace(JSON_VERSION, `$1"${version}"`);
    if (JSON.parse(updated).version !== version) {
      throw new Error(`${file}: the top-level version did not change`);
    }
    writeFileSync(full, updated);
  }
  const cargoPath = path.join(root, "Cargo.toml");
  const cargo = readFileSync(cargoPath, "utf8");
  const [before, after] = cargo.split("[workspace.package]");
  if (after === undefined) throw new Error("Cargo.toml has no [workspace.package]");
  const end = after.indexOf("\n[");
  const block = end === -1 ? after : after.slice(0, end);
  const rest = end === -1 ? "" : after.slice(end);
  writeFileSync(
    cargoPath,
    `${before}[workspace.package]${block.replace(/^version = "[^"]+"/m, `version = "${version}"`)}${rest}`,
  );
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const root = process.cwd();
  const arg = process.argv[2];
  if (arg === "--check" || !arg) {
    const v = readVersions(root);
    const same = v.packageJson === v.tauriConf && v.tauriConf === v.cargo;
    console.log(same ? `version ${v.cargo}` : `versions differ: ${JSON.stringify(v)}`);
    process.exit(same ? 0 : 1);
  }
  try {
    setVersion(root, arg);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
  console.log(`version set to ${arg} (now run: cargo metadata --format-version 1 > $null)`);
}

// The update manifests (Tauri static JSON + DZBuild extras) of the built release, one per
// channel: dist-release/<v>/updates/<channel>/latest.json, uploaded by the owner to
// https://dzbuild.com/pos/updates/<channel>/latest.json with the setup next to its URL.
// Usage: pnpm release:manifest [--rollout 10] [--pause] [--minimum 0.5.0] [--allow-unsigned] [--base https://dzbuild.com/pos/download]
// Each arch built by pnpm release:build (x64, and x86 with --arch x86) whose setup has its .sig
// gets its own entries: windows-x86_64 / windows-x86_64-nsis and windows-i686 / windows-i686-nsis.
// The extras are read by pos_core::updates (ManifestExtras); the Tauri plugin ignores them.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { ARCHES, archLayout, authenticodeStatus } from "./build.mjs";
import { readVersions, SEMVER } from "./version.mjs";

/** The Tauri updater's platform keys of each Windows arch (`{os}-{arch}` and its `-nsis` form). */
export const PLATFORM_KEYS = {
  x64: ["windows-x86_64-nsis", "windows-x86_64"],
  x86: ["windows-i686-nsis", "windows-i686"],
};

export function channelsFor(version) {
  return version.includes("-") ? ["beta"] : ["stable", "beta"];
}

export function checkSignature(sigText, version) {
  const decoded = Buffer.from(sigText.trim(), "base64").toString("utf8");
  const comment = decoded.split("\n").find((line) => line.startsWith("trusted comment:")) ?? "";
  const fields = comment.replace("trusted comment:", "").trim().split("\t");
  const signed = fields
    .find((f) => f.startsWith("version:"))
    ?.slice("version:".length)
    .replace(/^v/, "");
  if (signed !== version) {
    throw new Error(`the signature does not carry version:${version} (requireSignedVersion would refuse it)`);
  }
}

/** `setups`: { x64?, x86? }, each { sig, url, sha256, size } — at least one. */
export function buildManifest({ version, notes, setups, pubDate, rollout, paused, minimumVersion }) {
  for (const lang of ["ar", "fr", "en"]) {
    if (!notes?.[lang]?.length)
      throw new Error(`${lang} release notes are empty (src/release-notes/${version}.json)`);
  }
  const arches = Object.keys(setups ?? {});
  if (!arches.length) throw new Error("no setup with its .sig: nothing to publish");
  for (const arch of arches) {
    if (!Object.hasOwn(PLATFORM_KEYS, arch)) throw new Error(`unknown arch ${arch}`);
    if (!setups[arch].url?.startsWith("https://")) throw new Error("the download URL must be https");
  }
  // pos_core::updates reads `rollout` as a whole number and treats anything else as 100 %.
  if (!Number.isInteger(rollout) || rollout < 0 || rollout > 100) {
    throw new Error("the rollout must be a whole percentage from 0 to 100");
  }
  if (minimumVersion != null && !SEMVER.test(minimumVersion)) {
    throw new Error(`the minimum version ${minimumVersion} is not a semver version`);
  }
  const platforms = {};
  for (const arch of arches) {
    const { sig, url, sha256, size } = setups[arch];
    checkSignature(sig, version);
    const platform = { signature: sig.trim(), url, sha256, size };
    for (const key of PLATFORM_KEYS[arch]) platforms[key] = platform;
  }
  const joined = (lang) => notes[lang].join("\n");
  return {
    version,
    notes: joined("fr"),
    pub_date: pubDate,
    platforms,
    notes_i18n: { ar: joined("ar"), fr: joined("fr"), en: joined("en") },
    paused: Boolean(paused),
    rollout,
    minimum_version: minimumVersion ?? null,
  };
}

function flag(name) {
  const i = process.argv.indexOf(name);
  if (i < 0) return null;
  const value = process.argv[i + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(`${name} needs a value`);
  return value;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    const version = readVersions(process.cwd()).cargo;
    const dir = path.join("dist-release", version);
    const base = (flag("--base") ?? "https://dzbuild.com/pos/download").replace(/\/+$/, "");
    const allowUnsigned = process.argv.includes("--allow-unsigned") && version.includes("-");
    const setups = {};
    for (const arch of Object.keys(ARCHES)) {
      const setup = path.join(dir, archLayout(".", version, arch).setup);
      // An arch without its .sig (not built, or built without the updater key) is left out.
      if (!existsSync(setup) || !existsSync(`${setup}.sig`)) continue;
      const status = authenticodeStatus(setup);
      if (status !== "Valid" && !allowUnsigned) {
        throw new Error(
          `${setup} is not signed (Authenticode ${status}); only a beta may be published unsigned (--allow-unsigned)`,
        );
      }
      setups[arch] = {
        sig: readFileSync(`${setup}.sig`, "utf8"),
        url: `${base}/${version}/${path.basename(setup)}`,
        sha256: createHash("sha256").update(readFileSync(setup)).digest("hex"),
        size: statSync(setup).size,
      };
      console.log(`${arch}: ${setup}`);
    }
    if (!Object.keys(setups).length) {
      throw new Error(
        `no setup with its .sig in ${dir}: run pnpm release:build (with the updater key) first`,
      );
    }
    const rollout = flag("--rollout");
    const manifest = buildManifest({
      version,
      notes: JSON.parse(readFileSync(path.join("src", "release-notes", `${version}.json`), "utf8")),
      setups,
      pubDate: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
      rollout: rollout === null ? 100 : Number(rollout),
      paused: process.argv.includes("--pause"),
      minimumVersion: flag("--minimum"),
    });
    for (const channel of channelsFor(version)) {
      const target = path.join(dir, "updates", channel);
      mkdirSync(target, { recursive: true });
      writeFileSync(path.join(target, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
      console.log(`${channel}: ${path.join(target, "latest.json")}`);
    }
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

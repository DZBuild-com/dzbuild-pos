// release.yml: the installers each arch's job built (one artifact per arch, each holding
// <version>/ with its setups, their .sig and its own SHA256SUMS.txt) gathered into
// dist-release/<version>/ with one SHA256SUMS.txt, as `pnpm release:build` leaves it on one PC
// that built both arches. pnpm release:manifest then reads that folder.
// Usage: node scripts/release/collect.mjs <version> <artifact dir>... [--out dist-release]
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { mergeSums } from "./build.mjs";
import { SEMVER } from "./version.mjs";

const SUMS = "SHA256SUMS.txt";
const SUM_LINE = /^([0-9a-fA-F]{64})\s+\*?(.+)$/;

/** The `{ hash, name }` lines of a SHA256SUMS.txt (`<hash>  <name>` or `<hash> *<name>`). */
export function parseSums(text) {
  return text
    .split(/\r?\n/)
    .map((line) => SUM_LINE.exec(line.trim()))
    .filter(Boolean)
    .map((match) => ({ hash: match[1].toLowerCase(), name: match[2] }));
}

/** Copies every file of `<source>/<version>/` into `<out>/<version>/`, merging the SHA256SUMS.txt
 * lines in the order of `sources`. Returns the paths of the files in the release folder. */
export function collectArtifacts({ version, sources, out }) {
  if (!SEMVER.test(version ?? "")) throw new Error(`${version} is not a version`);
  if (!sources.length) throw new Error("no installer artifact to collect");
  const target = path.join(out, version);
  mkdirSync(target, { recursive: true });
  let sums = "";
  for (const source of sources) {
    const dir = path.join(source, version);
    if (!existsSync(dir)) throw new Error(`${path.basename(source)} has no ${version} folder`);
    for (const name of readdirSync(dir)) {
      const file = path.join(dir, name);
      if (!statSync(file).isFile()) continue;
      if (name === SUMS) {
        sums = mergeSums(sums, parseSums(readFileSync(file, "utf8")));
      } else {
        copyFileSync(file, path.join(target, name));
      }
    }
  }
  writeFileSync(path.join(target, SUMS), sums);
  return readdirSync(target).map((name) => path.join(target, name));
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    const argv = process.argv.slice(2);
    const at = argv.indexOf("--out");
    const out = at >= 0 ? argv[at + 1] : "dist-release";
    const [version, ...sources] = argv.filter((_, i) => at < 0 || (i !== at && i !== at + 1));
    for (const file of collectArtifacts({ version, sources, out })) console.log(file);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

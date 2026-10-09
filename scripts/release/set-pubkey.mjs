// Writes the updater's public key (the `.pub` that `tauri signer generate` wrote next to the
// private key) into src-tauri/tauri.conf.json > plugins.updater.pubkey. Reads only the public
// half; refuses a private key. Usage: node scripts/release/set-pubkey.mjs [--pub <file>]
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export function setPubkey(confPath, pubPath) {
  const pub = readFileSync(pubPath, "utf8").trim();
  if (!pub) throw new Error(`${pubPath} is empty`);
  const decoded = Buffer.from(pub, "base64").toString("utf8");
  if (!/public key/i.test(decoded) || /secret key/i.test(decoded)) {
    throw new Error(`${pubPath} is not a public key`);
  }
  const conf = JSON.parse(readFileSync(confPath, "utf8"));
  conf.plugins ??= {};
  conf.plugins.updater ??= {};
  conf.plugins.updater.pubkey = pub;
  writeFileSync(confPath, `${JSON.stringify(conf, null, 2)}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
  const flag = process.argv.indexOf("--pub");
  const pub = flag > 0 ? process.argv[flag + 1] : path.join(root, "src-tauri", "keys", "updater.key.pub");
  setPubkey(path.join(root, "src-tauri", "tauri.conf.json"), pub);
  console.log("plugins.updater.pubkey updated");
}

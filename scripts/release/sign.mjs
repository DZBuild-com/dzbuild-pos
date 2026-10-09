// Tauri's bundle.windows.signCommand runs `node scripts/release/sign.mjs <file>` for every
// binary when DZPOS_SIGN_PROVIDER is set (build.mjs adds it). Secrets come from the environment
// and are never printed. Providers: signtool (a certificate in the Windows store, including an
// HSM key exposed through its KSP), keylocker (DigiCert KeyLocker smctl), esigner (SSL.com
// eSigner CodeSignTool). Azure Artifact Signing is not available to an Algerian publisher.
// Every provider signs with SHA-256 and an RFC 3161 timestamp: signtool through
// DZPOS_TIMESTAMP_URL; smctl and CodeSignTool through their own TSA (smctl's `--timestamp` is an
// on/off switch, CodeSignTool reads TSA_URL from its conf/code_sign_tool.properties).
// The owner checks the flags once with `smctl sign --help` / `CodeSignTool.bat --help`.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { pathToFileURL } from "node:url";

const DEFAULT_TSA = "http://timestamp.digicert.com";

function need(env, name) {
  const value = env[name];
  if (!value) throw new Error(`${name} is not set for the signing provider`);
  return value;
}

export function signCommand(provider, env, file) {
  const tsa = env.DZPOS_TIMESTAMP_URL || DEFAULT_TSA;
  switch (provider) {
    case "signtool":
      return {
        cmd: env.DZPOS_SIGNTOOL || "signtool.exe",
        args: [
          "sign",
          "/sha1",
          need(env, "DZPOS_SIGN_THUMBPRINT"),
          "/fd",
          "sha256",
          "/tr",
          tsa,
          "/td",
          "sha256",
          "/d",
          "DZBuild POS",
          file,
        ],
        secrets: [],
      };
    case "keylocker":
      return {
        cmd: env.DZPOS_SMCTL || "smctl",
        args: [
          "sign",
          "--keypair-alias",
          need(env, "SM_KEYPAIR_ALIAS"),
          "--input",
          file,
          "--digalg",
          "SHA256",
          "--timestamp=true",
        ],
        // smctl reads SM_HOST, SM_API_KEY, SM_CLIENT_CERT_FILE and SM_CLIENT_CERT_PASSWORD itself.
        secrets: [env.SM_API_KEY, env.SM_CLIENT_CERT_PASSWORD].filter(Boolean),
      };
    case "esigner": {
      const user = need(env, "ES_USERNAME");
      const password = need(env, "ES_PASSWORD");
      const credential = need(env, "ES_CREDENTIAL_ID");
      const totp = need(env, "ES_TOTP_SECRET");
      const dir = need(env, "CODESIGNTOOL_DIR");
      return {
        cmd: path.join(dir, "CodeSignTool.bat"),
        args: [
          "sign",
          `-username=${user}`,
          `-password=${password}`,
          `-credential_id=${credential}`,
          `-totp_secret=${totp}`,
          `-input_file_path=${file}`,
          "-override",
        ],
        secrets: [user, password, credential, totp],
        // CodeSignTool finds its conf/ and jar/ folders from where it runs.
        cwd: dir,
      };
    }
    default:
      throw new Error(`unknown signing provider ${provider} (signtool, keylocker or esigner)`);
  }
}

export function masked(command) {
  let text = [command.cmd, ...command.args].join(" ");
  for (const secret of command.secrets) text = text.split(secret).join("***");
  return text;
}

// A .bat file only runs through cmd.exe, which splits on spaces and expands `%NAME%` even inside
// quotes: every part is quoted, and a `"` or `%` (most likely in a password) stops the signing
// with a message that names no secret. Trailing backslashes are doubled so the closing quote
// stays a quote for the program's own argument parser.
export function batchLine(command) {
  return [command.cmd, ...command.args]
    .map((part) => {
      if (/["%]/.test(part)) {
        throw new Error(
          `an argument of ${path.basename(command.cmd)} holds " or %, which cmd.exe cannot pass safely: change that setting`,
        );
      }
      return `"${part.replace(/(\\+)$/, "$1$1")}"`;
    })
    .join(" ");
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    const file = process.argv[2];
    if (!file) throw new Error("usage: node scripts/release/sign.mjs <file>");
    const command = signCommand(process.env.DZPOS_SIGN_PROVIDER, process.env, file);
    console.log(`signing: ${masked(command)}`);
    const run = /\.(bat|cmd)$/i.test(command.cmd)
      ? spawnSync(batchLine(command), { stdio: "inherit", shell: true, cwd: command.cwd })
      : spawnSync(command.cmd, command.args, { stdio: "inherit", cwd: command.cwd });
    if (run.error) throw new Error(`${path.basename(command.cmd)} did not start (${run.error.code})`);
    process.exit(run.status ?? 1);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

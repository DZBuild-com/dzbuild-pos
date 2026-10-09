import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";

const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const windows = conf.bundle.windows;
const LANG_KEYS = [
  "addOrReinstall",
  "alreadyInstalled",
  "alreadyInstalledLong",
  "appRunning",
  "appRunningOkKill",
  "chooseMaintenanceOption",
  "choowHowToInstall",
  "createDesktop",
  "dontUninstall",
  "dontUninstallDowngrade",
  "failedToKillApp",
  "installingWebview2",
  "newerVersionInstalled",
  "older",
  "olderOrUnknownVersionInstalled",
  "silentDowngrades",
  "unableToUninstall",
  "uninstallApp",
  "uninstallBeforeInstalling",
  "unknown",
  "webview2AbortError",
  "webview2DownloadError",
  "webview2DownloadSuccess",
  "webview2Downloading",
  "webview2InstallError",
  "webview2InstallSuccess",
  "deleteAppData",
];

test("the online setup embeds the bootstrapper and the offline one the runtime", () => {
  assert.deepEqual(windows.webviewInstallMode, { type: "embedBootstrapper", silent: true });
  const offline = JSON.parse(readFileSync("src-tauri/tauri.offline.conf.json", "utf8"));
  assert.deepEqual(offline.bundle.windows.webviewInstallMode, { type: "offlineInstaller", silent: true });
  const rust = readFileSync("src-tauri/src/webview.rs", "utf8");
  assert.match(
    rust,
    new RegExp(`MIN_WEBVIEW2: &str = "${windows.minimumWebview2Version.replaceAll(".", "\\.")}"`),
    "installer and app agree on the WebView2 minimum",
  );
});

test("per machine, three languages with their own texts, hooks in place", () => {
  assert.equal(windows.nsis.installMode, "perMachine");
  assert.deepEqual(windows.nsis.languages, ["English", "French", "Arabic"]);
  for (const lang of windows.nsis.languages) {
    const file = path.join("src-tauri", windows.nsis.customLanguageFiles[lang]);
    assert.ok(existsSync(file), file);
    const text = readFileSync(file, "utf8");
    const keys = [...text.matchAll(/^LangString (\w+) \$\{LANG_\w+\}/gm)].map((m) => m[1]).sort();
    assert.deepEqual(keys, [...LANG_KEYS].sort(), `${lang} defines every Tauri message`);
    assert.ok(!text.includes("{{product_name}}"), `${lang}: custom files are not templated`);
    assert.match(text, /LangString deleteAppData .*(cache|ذاكرة)/i, `${lang}: the box says what it removes`);
  }
  const hooks = readFileSync("src-tauri/windows/hooks.nsh", "utf8");
  for (const macro of ["NSIS_HOOK_POSTINSTALL", "NSIS_HOOK_PREUNINSTALL", "NSIS_HOOK_POSTUNINSTALL"]) {
    assert.match(hooks, new RegExp(`!macro ${macro}\\b`), macro);
  }
  assert.match(hooks, /localport=7345/);
  assert.match(hooks, /profile=private/);
  assert.match(hooks, /LocalDumps\\\$\{MAINBINARYNAME\}\.exe/);
  assert.match(hooks, /StrCpy \$DeleteAppDataCheckboxState 0/, "the shop's data is never deleted");
  assert.ok(!/RmDir \/r "\$APPDATA/.test(hooks), "never removes %APPDATA%");
  assert.match(hooks, /\$UpdateMode <> 1/, "an update keeps the rule");
});

test("the installer adds, renews and removes the two LAN rules the main PC checks", () => {
  const firewall = readFileSync("crates/pos-server/src/firewall.rs", "utf8");
  const wire = readFileSync("crates/pos-server/src/wire.rs", "utf8");
  const ruleName = (constant) => firewall.match(new RegExp(`pub const ${constant}: &str = "([^"]+)";`))[1];
  const rules = [
    { name: ruleName("RULE_TCP"), protocol: "TCP", port: wire.match(/pub const PORT: u16 = (\d+);/)[1] },
    { name: ruleName("RULE_MDNS"), protocol: "UDP", port: firewall.match(/protocol=UDP localport=(\d+)/)[1] },
  ];
  const hooks = readFileSync("src-tauri/windows/hooks.nsh", "utf8");
  const macro = (name) => {
    const body = hooks.match(new RegExp(`!macro ${name}\\b([\\s\\S]*?)!macroend`));
    assert.ok(body, `${name} is defined`);
    return body[1];
  };
  const program = String.raw`program="$INSTDIR\${MAINBINARYNAME}.exe"`;
  for (const { name, protocol, port } of rules) {
    const add = `netsh advfirewall firewall add rule name="${name}" dir=in action=allow protocol=${protocol} localport=${port} profile=private ${program} enable=yes`;
    const remove = `netsh advfirewall firewall delete rule name="${name}"`;
    const install = macro("NSIS_HOOK_POSTINSTALL");
    assert.ok(install.includes(add), `${name}: the line pos_server::firewall::install_commands names`);
    assert.ok(
      install.indexOf(remove) !== -1 && install.indexOf(remove) < install.indexOf(add),
      `${name}: renewed on every install and update`,
    );
    assert.ok(macro("NSIS_HOOK_POSTUNINSTALL").includes(remove), `${name}: removed on uninstall`);
  }
});

test("the setup is compressed with one LZMA dictionary over the whole app (NFR-002)", () => {
  assert.equal(windows.nsis.compression, "lzma");
  const hooks = readFileSync("src-tauri/windows/hooks.nsh", "utf8");
  // Outside every macro: Tauri includes the hooks after its SetCompressor and before any file,
  // the only place NSIS accepts a compressor setting.
  const outside = hooks.replace(/^!macro [\s\S]*?^!macroend/gm, "");
  const size = outside.match(/^SetCompressorDictSize (\d+)$/m);
  assert.ok(size, "the hooks set the LZMA dictionary size at the top level");
  // NSIS's default 8 MB window cannot reach back across the ~50 MB app; 64 MB covers it, and
  // that is also the memory the setup needs while it unpacks (4 GB shop PCs).
  assert.equal(Number(size[1]), 64);
});

test("the updater requires signed versions and https", () => {
  const updater = conf.plugins.updater;
  assert.equal(updater.requireSignedVersion, true);
  assert.ok(updater.endpoints.length > 0);
  for (const url of updater.endpoints) assert.match(url, /^https:\/\/.+\/stable\/latest\.json$/);
  assert.match(Buffer.from(updater.pubkey, "base64").toString("utf8"), /minisign public key/);
  assert.equal(updater.windows.installMode, "passive");
  assert.equal(conf.bundle.createUpdaterArtifacts, false, "only release:build signs update artifacts");
  assert.equal(windows.signCommand ?? null, null, "signing is added by release:build when configured");
});

test("the installers never carry the owner's licence tool or key", () => {
  const text = ["src-tauri/tauri.conf.json", "src-tauri/tauri.offline.conf.json"]
    .map((file) => readFileSync(file, "utf8"))
    .join("\n");
  assert.ok(!/dzpos-license|pos-license-cli|licence-signing|ledger\.json/.test(text));
});

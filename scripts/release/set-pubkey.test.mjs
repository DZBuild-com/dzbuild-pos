import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { setPubkey } from "./set-pubkey.mjs";

test("the public key lands in plugins.updater and nothing else changes", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "dzpos-pubkey-"));
  const conf = path.join(dir, "tauri.conf.json");
  writeFileSync(
    conf,
    JSON.stringify(
      {
        productName: "DZBuild POS",
        plugins: {
          updater: { pubkey: "", endpoints: ["https://dzbuild.com/pos/updates/stable/latest.json"] },
        },
      },
      null,
      2,
    ),
  );
  const pub = path.join(dir, "updater.key.pub");
  writeFileSync(pub, "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk=\n");
  setPubkey(conf, pub);
  const written = JSON.parse(readFileSync(conf, "utf8"));
  assert.equal(written.plugins.updater.pubkey, "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk=");
  assert.equal(written.productName, "DZBuild POS");
  assert.ok(readFileSync(conf, "utf8").endsWith("}\n"));
});

test("a private key or an empty file is refused", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "dzpos-pubkey-"));
  const conf = path.join(dir, "tauri.conf.json");
  writeFileSync(conf, "{}");
  const secret = path.join(dir, "updater.key");
  writeFileSync(
    secret,
    Buffer.from("untrusted comment: rsign encrypted secret key\nRWRTY0...").toString("base64"),
  );
  assert.throws(() => setPubkey(conf, secret), /not a public key/);
  const empty = path.join(dir, "empty.pub");
  writeFileSync(empty, "");
  assert.throws(() => setPubkey(conf, empty), /empty/);
});

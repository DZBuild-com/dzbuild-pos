import assert from "node:assert/strict";
import { test } from "node:test";
import { batchLine, masked, signCommand } from "./sign.mjs";

const file = "C:\\build\\DZBuild POS_0.6.0_x64-setup.exe";

test("each provider signs with SHA-256 and an RFC 3161 timestamp", () => {
  const st = signCommand("signtool", { DZPOS_SIGN_THUMBPRINT: "A1B2C3" }, file);
  assert.equal(st.cmd, "signtool.exe");
  assert.deepEqual(st.args, [
    "sign",
    "/sha1",
    "A1B2C3",
    "/fd",
    "sha256",
    "/tr",
    "http://timestamp.digicert.com",
    "/td",
    "sha256",
    "/d",
    "DZBuild POS",
    file,
  ]);
  const kl = signCommand("keylocker", { SM_KEYPAIR_ALIAS: "dzbuild-ov" }, file);
  assert.deepEqual(kl.args.slice(0, 3), ["sign", "--keypair-alias", "dzbuild-ov"]);
  assert.ok(kl.args.includes(file));
  const es = signCommand(
    "esigner",
    {
      ES_USERNAME: "owner@dzbuild.dev",
      ES_PASSWORD: "p@ss",
      ES_CREDENTIAL_ID: "cred",
      ES_TOTP_SECRET: "totp",
      CODESIGNTOOL_DIR: "C:\\CodeSignTool",
    },
    file,
  );
  assert.equal(es.cmd, "C:\\CodeSignTool\\CodeSignTool.bat");
  assert.ok(es.args.includes(`-input_file_path=${file}`) && es.args.includes("-override"));
  const shown = masked(es);
  for (const secret of ["p@ss", "totp", "owner@dzbuild.dev"])
    assert.ok(!shown.includes(secret), `${secret} printed`);
});

test("a missing setting or an unknown provider stops the build", () => {
  assert.throws(() => signCommand("signtool", {}, file), /DZPOS_SIGN_THUMBPRINT/);
  assert.throws(() => signCommand("esigner", { ES_USERNAME: "x" }, file), /ES_PASSWORD/);
  assert.throws(() => signCommand("azure", {}, file), /unknown signing provider/);
});

test("CodeSignTool.bat gets every argument quoted, a path with spaces stays one argument", () => {
  const env = {
    ES_USERNAME: "u",
    ES_PASSWORD: "a&b|c",
    ES_CREDENTIAL_ID: "c",
    ES_TOTP_SECRET: "t",
    CODESIGNTOOL_DIR: "C:\\Code Sign Tool",
  };
  const line = batchLine(signCommand("esigner", env, file));
  assert.ok(line.startsWith('"C:\\Code Sign Tool\\CodeSignTool.bat" "sign" '));
  assert.ok(line.includes(`"-input_file_path=${file}"`));
  assert.ok(line.includes('"-password=a&b|c"'), "cmd.exe metacharacters stay inside quotes");
  for (const bad of ['p"ss', "p%PATH%ss"]) {
    assert.throws(
      () => batchLine(signCommand("esigner", { ...env, ES_PASSWORD: bad }, file)),
      (error) => {
        assert.match(error.message, /cmd\.exe cannot pass/);
        assert.ok(!error.message.includes(bad), "the secret is not in the message");
        return true;
      },
    );
  }
});

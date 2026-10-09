# Signing DZBuild POS

Every `.exe` of a release is Authenticode-signed with an RFC 3161 timestamp (the signature stays
valid after the certificate expires). An EV certificate no longer skips SmartScreen (March 2024):
buy an **OV** code-signing certificate whose key lives in the CA's cloud HSM, so CI can sign.
Azure Artifact Signing does not accept an Algerian publisher. Certificates issued since
2026-03-01 last at most 460 days: renew every year **under the same subject name** so the
reputation carries over.

## Providers the release scripts know

`scripts/release/sign.mjs` runs for every binary when `DZPOS_SIGN_PROVIDER` is set; it prints
the command with every secret replaced by `***`.

| `DZPOS_SIGN_PROVIDER` | Service | Variables |
|---|---|---|
| `esigner` | SSL.com eSigner (CodeSignTool) | `ES_USERNAME`, `ES_PASSWORD`, `ES_CREDENTIAL_ID`, `ES_TOTP_SECRET`, `CODESIGNTOOL_DIR` |
| `keylocker` | DigiCert KeyLocker (`smctl`) | `SM_HOST`, `SM_API_KEY`, `SM_CLIENT_CERT_FILE`, `SM_CLIENT_CERT_PASSWORD`, `SM_KEYPAIR_ALIAS`, optional `DZPOS_SMCTL` (path to `smctl`) |
| `signtool` | Any certificate installed in the Windows store (a USB token or a KSP-backed cloud key) | `DZPOS_SIGN_THUMBPRINT`, optional `DZPOS_SIGNTOOL` |

`DZPOS_TIMESTAMP_URL` overrides the timestamp server of the `signtool` provider (default
`http://timestamp.digicert.com`; SSL.com's is `http://ts.ssl.com`). `smctl` and CodeSignTool
timestamp through their own server: CodeSignTool reads `TSA_URL` from
`conf\code_sign_tool.properties` in `CODESIGNTOOL_DIR`.

Check the flags once with the provider's own tool (`smctl sign --help`, `CodeSignTool.bat --help`)
before the first signed release. With eSigner, no value may contain `"` or `%`: CodeSignTool is a
`.bat` run through `cmd.exe`, which cannot pass them safely, so `sign.mjs` refuses them.

## On the owner's PC

    $env:DZPOS_SIGN_PROVIDER = "esigner"
    $env:ES_USERNAME = "<account e-mail>"; $env:ES_PASSWORD = "<password>"
    $env:ES_CREDENTIAL_ID = "<from the eSigner dashboard>"; $env:ES_TOTP_SECRET = "<the TOTP secret>"
    $env:CODESIGNTOOL_DIR = "C:\Tools\CodeSignTool"
    pnpm release:build

Check: `Get-AuthenticodeSignature "dist-release\<v>\DZBuildPOS-<v>-setup.exe" | Format-List Status, SignerCertificate, TimeStamperCertificate`
→ `Status : Valid`, the subject `CN=DZBuild…`, a timestamper.

## In CI

Add the same variables as repository secrets (Settings › Secrets and variables › Actions); the
`release.yml` job passes them through `env:` only. It passes the provider's variables of the table
above, not the optional `DZPOS_TIMESTAMP_URL`, `DZPOS_SIGNTOOL` and `DZPOS_SMCTL` (the defaults
apply).

The GitHub-hosted runner has none of the providers' tools: `signtool` needs the certificate in the
runner's Windows store, `keylocker` needs `smctl` installed and the client certificate file at
`SM_CLIENT_CERT_FILE`, `esigner` needs CodeSignTool unpacked at `CODESIGNTOOL_DIR`. Until a step
that installs the chosen provider's tool is added to `release.yml`, leave `DZPOS_SIGN_PROVIDER`
unset in CI: CI then builds unsigned setups and still drafts the GitHub release, without update
manifests for a stable version (manifest.mjs refuses an unsigned stable setup; the step says so
in a notice), and the published setups are built and signed on the owner's PC. The secrets and
how to add them: `docs/release/github-actions.md`.

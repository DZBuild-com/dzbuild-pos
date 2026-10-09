# GitHub Actions

This page lists the GitHub workflows of DZBuild POS, the secrets a release needs and how to add
them safely, the runner minutes a release uses, and how to start a build by hand.
The workflows live in `.github/workflows/`; `scripts/release/workflows.test.mjs` checks their key
properties with the other tooling tests.

## The workflows

| Workflow | When | What it does |
|---|---|---|
| `ci.yml` | every push to `main`, or by hand; called by `release.yml` | **frontend** (Ubuntu): `pnpm install --frozen-lockfile`, ESLint, TypeScript, Vitest, the tooling tests (`scripts/**/*.test.mjs`), the ar/fr/en parity check, Prettier, the frontend build and the POS bundle budget. **rust** (Windows): `cargo fmt --check`, Clippy with `-D warnings`, `cargo test --workspace`, and a `cargo check` of the 32-bit target `i686-pc-windows-msvc`. **deny** (Ubuntu): `cargo deny check advisories bans licenses sources` with `deny.toml`. **build** (Windows, pushes to `main` only): `pnpm tauri build --no-bundle`. A new push cancels the run it replaces. |
| `release.yml` | a pushed `v*` tag, or by hand | Checks that the tag is `v<version>`, runs the CI gates, then builds the installers of **x64 and x86 in parallel** (`pnpm release:build --arch <arch>`), gathers them with one `SHA256SUMS.txt`, writes the update manifests when it can (below) and creates or refreshes a **draft** GitHub release. It never publishes anything to shops. |
| `nightly.yml` | by hand only | The Playwright end-to-end suite on Edge (screenshots in the three languages as an artifact), the performance budgets, `pnpm audit` and `cargo deny check advisories`. |
| `codeql.yml` | pushes to `main`, or by hand | CodeQL code scanning (`security-extended` queries) of the TypeScript, the Rust and the workflows themselves. |
| `secret-scan.yml` | pushes to `main`, or by hand | TruffleHog on the working tree and on the git history; only **verified** live credentials fail the run. |
| `dependency-review.yml` | pull requests | Refuses a new dependency with a known vulnerability (moderate or worse) or a licence that does not fit GPL-3.0-or-later. |
| `scorecard.yml` | by hand only | OpenSSF Scorecard of the repository's security practices, published for the README badge. |
| `labeler.yml` | pull requests | Labels a pull request from the paths it changes (`.github/labeler.yml`). It never checks out the pull request's code. |

Nothing runs on a timer and no build starts for a pull request: the two workflows that do run on a
pull request (`dependency-review.yml`, `labeler.yml`) finish in seconds. A maintainer who wants the
checks on a branch starts `CI` from the Actions tab with that branch selected. This keeps the
runner minutes for the pushes to `main` and the releases.

Dependabot (`.github/dependabot.yml`) only opens pull requests for security advisories. Routine
version updates are turned off (`open-pull-requests-limit: 0`); the maintainers update dependencies
in batches.

While the repository is **private**: CodeQL uploads and dependency review need GitHub Code
Security, so they report but do not fail; Scorecard is skipped. All three work on their own once
the repository is public. Every third-party action is pinned to a release tag.

## The release, step by step

1. **prepare** (Ubuntu): reads the version (`pnpm release:version --check`), refuses a tag that is
   not `v<version>`, checks `src/release-notes/<version>.json` and the `CHANGELOG.md` section,
   and picks the arches (`both` by default) and the pre-release flag (a version with a `-`
   suffix, like `1.1.0-beta.1`, or the `prerelease` input).
2. **gates**: the whole of `ci.yml`.
3. **installers** (Windows, one job per arch): `pnpm release:build` (x64) or
   `pnpm release:build --arch x86` (after `rustup target add i686-pc-windows-msvc`). Each job
   uploads the artifact `installers-x64` or `installers-x86`: the online setup, the offline setup,
   the `.sig` when the updater key is there, and its `SHA256SUMS.txt` lines. Artifacts are kept 30
   days.
4. **release** (Windows): `scripts/release/collect.mjs` puts both arches into
   `dist-release/<version>/` with **one** `SHA256SUMS.txt`; `pnpm release:manifest` writes the
   update manifests when the x64 setup has its `.sig` (skipped with a notice otherwise, and for a
   stable version whose setups are not Authenticode-signed); `scripts/release/notes-for-tag.mjs`
   writes the release text (English, then French and Arabic, the download table, the changelog);
   the draft release `v<version>` gets the four setups, `SHA256SUMS.txt`, the `.sig` files and
   `latest-<channel>.json` when present. A release that is already published is never changed: the
   run stops and asks for a new version.

Without any secret the release still works: the setups are unsigned (Windows SmartScreen warns),
have no updater signature, and no update manifest is written. That is enough for testing and for
contributors' forks.

## Secrets

Only a maintainer adds them, in **Settings › Secrets and variables › Actions › Repository secrets**
of `DZBuild-com/dzbuild-pos`, or with the GitHub CLI below. Workflows read them only as
`${{ secrets.NAME }}` in the `env:` of the one step that needs them; GitHub masks them in logs and
never gives them to pull requests from forks. Never paste a secret on the command line (it stays in
the shell history): pipe the file or let `gh` prompt for it.

| Secret | Needed for | How to add it |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | updater signatures (`.sig`) and the update manifests | the content of `src-tauri/keys/updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the same key's password | the content of `src-tauri/keys/updater.password` |
| `DZPOS_SIGN_PROVIDER` | Authenticode signing: `esigner`, `keylocker` or `signtool` | the provider's name; leave it **unset** until a step installs the provider's tool on the runner (`docs/release/signing.md`, "In CI") |
| `ES_USERNAME`, `ES_PASSWORD`, `ES_CREDENTIAL_ID`, `ES_TOTP_SECRET`, `CODESIGNTOOL_DIR` | provider `esigner` (SSL.com eSigner) | from the eSigner account |
| `SM_HOST`, `SM_API_KEY`, `SM_CLIENT_CERT_FILE`, `SM_CLIENT_CERT_PASSWORD`, `SM_KEYPAIR_ALIAS` | provider `keylocker` (DigiCert KeyLocker) | from the DigiCert account |
| `DZPOS_SIGN_THUMBPRINT` | provider `signtool` (a certificate in the Windows store) | the certificate's thumbprint |

From PowerShell on the release PC, in the repository folder (nothing is printed):

```powershell
Get-Content -Raw src-tauri\keys\updater.key | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo DZBuild-com/dzbuild-pos
Get-Content -Raw src-tauri\keys\updater.password | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo DZBuild-com/dzbuild-pos
gh secret list --repo DZBuild-com/dzbuild-pos   # names and dates only, never values
```

Authenticode signing, only for the provider you use (each command asks for the value without
showing it):

```powershell
gh secret set DZPOS_SIGN_PROVIDER --repo DZBuild-com/dzbuild-pos   # esigner, keylocker or signtool
# esigner (SSL.com eSigner)
gh secret set ES_USERNAME --repo DZBuild-com/dzbuild-pos
gh secret set ES_PASSWORD --repo DZBuild-com/dzbuild-pos
gh secret set ES_CREDENTIAL_ID --repo DZBuild-com/dzbuild-pos
gh secret set ES_TOTP_SECRET --repo DZBuild-com/dzbuild-pos
gh secret set CODESIGNTOOL_DIR --repo DZBuild-com/dzbuild-pos
# keylocker (DigiCert KeyLocker)
gh secret set SM_HOST --repo DZBuild-com/dzbuild-pos
gh secret set SM_API_KEY --repo DZBuild-com/dzbuild-pos
gh secret set SM_CLIENT_CERT_FILE --repo DZBuild-com/dzbuild-pos
gh secret set SM_CLIENT_CERT_PASSWORD --repo DZBuild-com/dzbuild-pos
gh secret set SM_KEYPAIR_ALIAS --repo DZBuild-com/dzbuild-pos
# signtool (a certificate in the Windows store)
gh secret set DZPOS_SIGN_THUMBPRINT --repo DZBuild-com/dzbuild-pos
```

From Git Bash the same works with a redirect:
`gh secret set TAURI_SIGNING_PRIVATE_KEY --repo DZBuild-com/dzbuild-pos < src-tauri/keys/updater.key`.

Optional hardening once the secrets are in place: move them into an environment (**Settings ›
Environments › New environment**, for example `release`, with a deployment rule that only allows
`v*` tags and a maintainer as required reviewer), then add `environment: release` to the
`installers` and `release` jobs of `release.yml`.

Repository settings that go with these workflows (**Settings › Actions › General**): "Workflow
permissions" on **Read repository contents** (each workflow asks for what it needs), and "Fork pull
request workflows" set to require approval for first-time contributors.

## Labels, milestones and branch protection

- **Labels** are listed in `.github/labels.yml` (type, `area/*`, `lang/*`, `priority/*`,
  `status/*`). The issue templates, Dependabot and the path labeler (`.github/labeler.yml`) only use
  names from that list. To add or change one, edit the file and run
  `gh label create "<name>" --color <color> --description "<description>" --force --repo DZBuild-com/dzbuild-pos`.
- **Milestones**: `1.0.x` (fixes to the LTS), `1.1.0` (next feature release), `Backlog`.
- **Branch protection** for `main` is the ruleset in `.github/rulesets/main.json`: no deletion, no
  force push, changes through a pull request (stale approvals dismissed), and the three CI jobs
  (frontend, rust, dependencies) green on an up-to-date branch. Repository admins can bypass it.
  GitHub only enforces rulesets on a private repository with a paid plan, so apply it once the
  repository is public (or the plan allows it):

  ```powershell
  gh api -X POST repos/DZBuild-com/dzbuild-pos/rulesets --input .github/rulesets/main.json
  ```

  The check names must match the `name:` of the jobs in `ci.yml`; rename both together.

## Cost in runner minutes

Public repositories run on GitHub-hosted runners for free. A private repository spends the plan's
included minutes, and Windows minutes count double (check GitHub's billing page for the current
rates). Rough figures for one full release run (`arch=both`), from a cold cache:

| Job | Runner | Minutes |
|---|---|---|
| prepare, CI frontend, CI deny | Ubuntu | about 10 |
| CI rust (fmt, Clippy, tests, i686 check) | Windows | 30–45 |
| installers x64 (online + offline: 2 release builds) | Windows | 35–60 |
| installers x86 (online + offline: 2 release builds) | Windows | 35–60 |
| release (collect, manifest, draft) | Windows | about 5 |

That is about 4 release builds, roughly **105–170 Windows minutes**, which a private repository
bills as about **210–340 minutes**. The jobs run in parallel, so the release takes about one to
one and a half hours from the tag to the draft. A warm Rust cache shortens later runs.

## Run a build by hand

```powershell
gh workflow run release.yml --repo DZBuild-com/dzbuild-pos -f arch=both
gh workflow run release.yml --repo DZBuild-com/dzbuild-pos -f arch=x86 -f prerelease=true
gh run watch --repo DZBuild-com/dzbuild-pos          # follow the run
gh run download --repo DZBuild-com/dzbuild-pos -n installers-x86   # the setups of one arch
```

A run started by hand from a branch builds the version in that branch and drafts the release
`v<version>` on that commit; delete the draft if it was only a test (`gh release delete
v<version> --yes`). The usual way stays: `git tag v<version>` on `main`, `git push origin
v<version>`.

## After the draft

The draft is the hand-off to `docs/release-checklist.md`: download the setups, run the checklist,
publish to dzbuild.com/pos as `docs/release/hosting.md` says, then press **Publish release** on
GitHub. Publishing on GitHub is the last step and never makes an update live by itself: shops only
see an update when the manifest on dzbuild.com/pos changes.

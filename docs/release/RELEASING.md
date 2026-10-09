# Releasing DZBuild POS for Windows

How a version goes from `main` to the shops: version bump, notes, gates, build, signing, update
manifests, checklist, publication. Run the steps in this order.

1. **Version.** `pnpm release:version <x.y.z>` (or `x.y.z-beta.N` for the beta channel), then
   `cargo metadata --format-version 1 > $null` to refresh `Cargo.lock`.
2. **Notes.** `pnpm release:changelog <x.y.z>`, then write the user-facing notes in
   `src/release-notes/<x.y.z>.json` in ar, fr and en (short sentences, no commit jargon).
3. **Gates.** `pnpm check`, `pnpm perf:bundle`, the last nightly `pnpm e2e` green, `cargo test --workspace`.
4. **Keys.** The updater key lives in `src-tauri/keys/` on the release PC or in the CI secrets
   (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). It was made once with
   `pnpm tauri signer generate --ci -p <password> -w src-tauri\keys\updater.key` and its public half set with
   `node scripts/release/set-pubkey.mjs`. Never print it or commit it (`.gitignore` excludes `keys/`, `*.pfx`
   and `*.key`). Custody and rotation: [`updater-key.md`](updater-key.md).
5. **Build.** `pnpm release:build` writes both setups to `dist-release/<v>/`: the online setup (15 MiB at most,
   with its `.sig`), the offline setup (about 215 MiB with the WebView2 runtime) and `SHA256SUMS.txt`. The
   setups are Authenticode-signed when `DZPOS_SIGN_PROVIDER` is set ([`signing.md`](signing.md)). Count about
   40 minutes (two release builds).
   Then `pnpm release:build --arch x86` (run `rustup target add i686-pc-windows-msvc` once) writes
   `DZBuildPOS-<v>-x86-setup.exe` and `DZBuildPOS-<v>-x86-offline-setup.exe` and merges their lines into the
   same `SHA256SUMS.txt`. A locked exe (os error 5 / LNK1104) is retried 5 times, 3 s apart.
6. **Update manifests.** `pnpm release:manifest [--rollout 10]` writes
   `dist-release/<v>/updates/stable|beta/latest.json` with the trilingual notes, the digest, the size and the
   kill switch (`--pause`); one entry pair per arch whose setup has its `.sig` (`windows-x86_64[-nsis]`,
   `windows-i686[-nsis]`). It refuses an unsigned stable setup.
7. **Checklist.** Run [`docs/release-checklist.md`](../release-checklist.md): install, update from the
   previous release, data safety, the reference-PC benchmark (`scripts\perf\bench.ps1`), uninstall keeps the
   data.
8. **Publish** as [`hosting.md`](hosting.md) describes (setups first, manifests last), roll out
   10 → 50 → 100 %, tag `v<x.y.z>`.
   On GitHub ([`github-actions.md`](github-actions.md)): pushing the tag (or
   `gh workflow run release.yml -f arch=both [-f prerelease=true]`) runs the CI gates, builds x64 and x86 in
   parallel on Windows runners (`pnpm release:build [--arch x86]`), merges one `SHA256SUMS.txt`
   (`scripts/release/collect.mjs`), runs `pnpm release:manifest` when the updater key secret is set (a stable
   version also needs Authenticode signing), and creates or refreshes a **draft** release with notes from
   `scripts/release/notes-for-tag.mjs`. It never touches a published release. Secrets are set only with
   `gh secret set` (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `DZPOS_SIGN_PROVIDER`
   and the provider's variables). Publish the draft last, after the checklist.
9. **Tauri CLI upgrades.** Compare `src-tauri/windows/lang/*.nsh` with the bundler's language files
   (`scripts/release/installer-config.test.mjs` lists the keys).

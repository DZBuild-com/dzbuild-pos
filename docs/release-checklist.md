# Release checklist

Run it for every stable release, on real machines, and keep the filled copy with the release
(`docs/release/checklist-<version>.md`). A line that fails blocks the release until a fix and its
regression test land. Beta releases run sections 1–4 and 9.

Machines: **A** a clean Windows 10 22H2 x64 VM, 4 GB RAM, no Internet after the WebView2 step;
**B** the reference PC (Celeron J1900, 4 GB, HDD, 1024×768 touch) with the 80 mm printer, the
drawer, a USB scanner on AZERTY and on Arabic, a USB stick; **C** a Windows 11 PC with the
previous release installed and a month of data.

## 1. Build and gates (build PC)

Two ways to build, with the same scripts: on GitHub (push the tag `v<version>`, or `gh workflow run release.yml -f arch=both`: `release.yml` runs the CI gates, builds x64 and x86 in parallel and drafts the GitHub release with the four setups, `SHA256SUMS.txt`, the `.sig` files and the manifests when the secrets allow; see `docs/release/github-actions.md`), or on the build PC with the commands below. Either way, every line of this section holds for the files that will be published.

- [ ] `pnpm release:version --check` prints the new version; `CHANGELOG.md` has its section; `src/release-notes/<version>.json` has notes in ar, fr and en.
- [ ] `pnpm check` exits 0; `pnpm perf:bundle` ≤ 300 kB; the last nightly `e2e` run is green and its ar/fr/en screenshots were looked at (1024×768 and 1366×768).
- [ ] `pnpm release:build`: in `dist-release\<v>\`, `DZBuildPOS-<v>-setup.exe` ≤ 15 MiB (about 11 MiB in 0.3.0) with its `.sig`, `DZBuildPOS-<v>-offline-setup.exe` about 215 MiB (the WebView2 runtime is inside; the script refuses one under 100 MiB), `SHA256SUMS.txt`; with the certificate, both show `Get-AuthenticodeSignature` → `Valid` and a timestamp.
- [ ] `pnpm release:build --arch x86` (from 1.0.0; `rustup target add i686-pc-windows-msvc` once): `DZBuildPOS-<v>-x86-setup.exe` with its `.sig` and `DZBuildPOS-<v>-x86-offline-setup.exe` next to the x64 ones, and `SHA256SUMS.txt` now lists the four setups (each arch merges its lines, a rebuild replaces its own). When Windows locks the freshly linked exe ("can't open main binary … Access is denied (os error 5)" or `LNK1104`, an antivirus scan), the script runs `pnpm tauri build` again by itself, up to 5 tries 3 s apart; any other failure stops at once.
- [ ] `pnpm release:manifest` writes `dist-release\<v>\updates\stable\latest.json` and `dist-release\<v>\updates\beta\latest.json` (a beta: `beta` only); notes present in the three languages; `platforms` holds `windows-x86_64` / `windows-x86_64-nsis` and, when the x86 setup has its `.sig`, `windows-i686` / `windows-i686-nsis` with their own URL and signature.

## 2. Install (machine A, then B)

- [ ] The online setup on A with Internet: the language page offers English, Français, العربية; per-machine install asks for admin; WebView2 is installed by the bootstrapper if missing; Start menu and desktop shortcuts carry the icon.
- [ ] The offline setup on A with the network cable out: installs and starts with zero network (the WebView2 runtime is inside).
- [ ] SmartScreen: on a machine that never ran it, note what Windows shows (signed: publisher "DZBuild"; see `docs/release/smartscreen.md`).
- [ ] `Get-NetFirewallRule -DisplayName "DZBuild POS LAN"` → inbound, Private profile, and `| Get-NetFirewallPortFilter` → TCP 7345 (the same for `"DZBuild POS mDNS"` with UDP 5353); `HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\dzbuild-pos.exe` exists with `DumpCount` 5.
- [ ] First start in each language: the wizard offers the demo shop, the import from another PC and the tour; the demo opens and leaves; the real setup completes.

## 3. Update (machine C)

- [ ] Point C to the staging manifest (edit `plugins.updater.endpoints` of a staging build, or host `latest.json` on the staging URL; the URL must start with `https://` and contain `/stable/`, the beta channel is read from `/beta/` next to it): Réglages › Mises à jour › "Rechercher maintenant" finds the version with its notes in the UI language.
- [ ] With a drawer open: "Installer maintenant" is disabled and names the till; no download starts by itself.
- [ ] Close the Z: the update is offered; "Installer maintenant" → a `pre_migration` backup appears (and on the USB stick), the setup runs in passive mode, the app restarts on the new version, "Voir les nouveautés" shows the notes, the journal has `update.downloaded`, `update.install_started`, `app.updated`.
- [ ] Every sale, Z and journal row of the month is still there; `documents_verify_chain` and Réglages › Journal d'audit › "Vérifier le journal" are clean.
- [ ] A manifest with `"paused": true` (`pnpm release:manifest --pause`) is not offered; one with a wrong `sha256` downloads and is refused as "incomplete or altered".

## 4. Data safety (machine B)

- [ ] Réglages › Sauvegardes: choose the USB stick ("Choisir une clé USB ou un dossier"); "Sauvegarder maintenant" copies to the stick; pull the stick, back up again: the local copy works and a banner says the second copy failed; plug it back: the next backup copies again.
- [ ] "Sauvegarder sur une clé USB maintenant" lists the stick within 3 s and copies to `DZBuild POS\sauvegardes`.
- [ ] "Aperçu et restauration" shows the last ticket of a backup; a restore with the owner PIN restarts on it.
- [ ] Guided restore: overwrite `pos.db` with zeros (on a copy of the data folder), start: the red screen offers "Restaurer une sauvegarde", the backups of the PC and the stick are listed, a wrong PIN is refused, the right one restores; `backups\corrupt-…` holds the damaged file.
- [ ] Power-cut drill (kill the process right after "Valider"): the sale is whole or absent; the next start journals the unclean shutdown (`db.unclean_shutdown`).
- [ ] Close the app: "Sauvegarde avant fermeture…" then a new `-auto.db`.
- [ ] Tray (finding 27): minimise the window, then restore it from the tray (the log shows the memory target LOW, then NORMAL); minimise again and choose "Quitter" in the tray on a single PC (the window comes back and shows "Sauvegarde avant fermeture…") and on a main PC with a till online (the window comes back with the "postes de caisse connectés" confirmation); after an Arabic sign-in the tray menu reads in Arabic.
- [ ] Clock drill: set Windows' date one day back, start: the "clock went back" notice and a `clock.moved_back` journal row; put the date back.

## 5. Performance on the reference PC (machine B)

Run `powershell -ExecutionPolicy Bypass -File scripts\perf\bench.ps1` (see `docs/release/reference-pc.md`) and paste its report:

- [ ] cold start (median of 5) ≤ 3 s — `cold_start_ms` in the log;
- [ ] idle RAM after 60 s on the sale screen ≤ 250 MB (`scripts\perf\idle-ram.ps1`), and lower after minimising (WebView2 memory target LOW);
- [ ] SQLite open < 50 ms; scan lookup p95 ≤ 20 ms (and the scan-to-line of the nightly E2E ≤ 50 ms on this PC: `pnpm e2e` once on B); sale commit p95 ≤ 80 ms; search on 100 000 products < 100 ms; Arabic receipt render ≤ 300 ms and receipt to paper ≤ 1.5 s with a stopwatch; five busy years < 2 GB;
- [ ] light mode on: no animation, the till grid without images, still usable at 1024×768 with the on-screen keyboard.

## 6. Sale, printing and hardware (machine B)

- [ ] The 12 lines of the M1 real-till smoke (`docs/testing/smoke/m1-real-till-smoke.md`): 80 mm then 58 mm printer, drawer, scanner on AZERTY and Arabic, returns, the Z, the kill test.
- [ ] The hardware lab of M4: `docs/testing/smoke/m4-hardware-lab.md` (labels, scale, display) when M4 is in the release.
- [ ] The M8 field validation: `docs/testing/smoke/m8-field-validation.md` (kitchen and driver-only printers, checkout scale, data pack) when M8 is in the release.
- [ ] The all-in-one terminal pass (HW-045): on a 15" 1024×768 touch terminal, every screen of section 7 is usable by touch only (44 px targets, the on-screen keyboard for names), no horizontal scroll in Arabic.

## 7. Back office, documents, LAN, DZBuild

- [ ] M2: `docs/testing/smoke/m2-manual-smoke.md`.
- [ ] M3: `docs/testing/smoke/m3-manual-smoke.md` when M3 is in the release.
- [ ] M5: `docs/testing/smoke/m5-two-pc-smoke.md` (the LAN smoke on two PCs) when M5 is in the release.
- [ ] M7: `docs/testing/smoke/m7-dzbuild-staging.md` when M7 is in the release and DZBuild staging answers.

## 8. Help and languages

- [ ] F1 on five screens in each language opens the right page; the quick card prints on A4 ("Microsoft Print to PDF" is enough); the tour opens from the wizard.
- [ ] The diagnostic zip opens; `LISEZ-MOI.txt` in three languages; no customer name, phone, PIN hash, token or database in it.

## 9. Uninstall (machine A)

- [ ] Uninstall with the box "Supprimer aussi le cache d'affichage" ticked: `%APPDATA%\dev.dzbuild.pos` (pos.db, backups, logs) stays; `%LOCALAPPDATA%\dev.dzbuild.pos` is gone; both firewall rules and the WER key are gone.
- [ ] Reinstall: the app opens on the same shop at the login screen.
- [ ] The "Supprimer toutes les données" help page matches what a person has to do by hand.

## 10. Publish

- [ ] Upload the two setups and `SHA256SUMS.txt` to `https://dzbuild.com/pos/download/<v>/`, then the manifests to `https://dzbuild.com/pos/updates/<channel>/latest.json` (last: the manifest makes the release live), as `docs/release/hosting.md` says; `Invoke-RestMethod` the manifest back.
- [ ] Staged rollout: first `--rollout 10`, after two days without a problem `--rollout 50`, then 100; a problem: `--pause`.
- [ ] The GitHub draft release `v<version>` (made by `release.yml`) lists the four setups and `SHA256SUMS.txt`, its notes read well in English, French and Arabic, and its files match the ones published on dzbuild.com/pos (same SHA-256); the pre-release box is ticked only for a beta. Then **Publish release**; tag `v<version>` is already there.

## 11. Licence (M9, machine A then B)

- [ ] A new install opens on the activation screen in the three languages; the PC's code is the one `dzpos-license issue` accepts; the QR opens a WhatsApp chat with the seller (when `src-tauri/license.toml` has the number).
- [ ] A trial issued for machine A activates (paste and `.dzlic`); the banner counts the days; on machine B the same key is refused ("issued for another PC").
- [ ] Machine A with the clock moved 8 days ahead: read-only mode — no sale, the Z of an open session closes and prints, reports, exports and a backup work; clock set back: still read-only.
- [ ] A lifetime licence replaces the trial; a second trial for machine A is refused by `dzpos-license issue`.
- [ ] Uninstall with the box ticked, reinstall: the licence is still active (the `%LOCALAPPDATA%` copy is repaired from the database and the Credential Manager).
- [ ] A till without a licence cannot join the main PC; with its own licence it joins and sells.

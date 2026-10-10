# Release checklist — 1.0.0 LTS (free edition)

The copy of `docs/release-checklist.md` for 1.0.0. Each line is marked **OK**, **KO** or **not run** (with the machine or the owner's step it
needs) when it is run; a line left `[ ]` with no mark has not been run yet.

1.0.0 is the free long-term-support edition: free forever, no licence, no key, no subscription
(`src-tauri/license.toml` `free = true`). What changed since the last internal build:

- **No licence.** Every PC sells right after the install: no activation screen, no read-only mode, no
  Settings › Licence, and no owner licence tool (`pnpm release:build` no longer builds `dzpos-license` for the
  free edition; nothing goes to `dist-owner\`).
- **No managed online backup.** The DZBuild cloud backup is removed; local backups, the second copy, USB sticks
  and restore are unchanged. Schema 9 is unchanged (migrations 0001–0009; the old `cloud_backups` tables stay
  unused).
- **Both arches.** `pnpm release:build` (x64) and `pnpm release:build --arch x86` (i686, 32-bit Windows) put four
  setups in `dist-release\1.0.0\` with one `SHA256SUMS.txt`; the update manifest carries
  `windows-x86_64` / `windows-x86_64-nsis` and `windows-i686` / `windows-i686-nsis`, each with its own URL and
  signature. A 32-bit app reads only its own entry (`pos_core::updates::PLATFORM_KEYS` follows the build's arch).
- **Hosting at dzbuild.com/pos.** Download page `https://dzbuild.com/pos`, setups at
  `https://dzbuild.com/pos/download/1.0.0/<file>`, support `https://dzbuild.com/pos/support`, manifests at
  `https://dzbuild.com/pos/updates/<channel>/latest.json` (`docs/release/hosting.md`). `dzbuild.dev` is never shown
  to shop users.
- Simpler wording in every screen and help page (ar/fr/en), every page opens at the top with a "Back to top" button.

Machines: **A** a clean Windows 10 22H2 x64 VM, 4 GB RAM, no Internet after the WebView2 step;
**A32** a clean 32-bit Windows 10 22H2 VM (x86), 2–4 GB RAM; **B** the reference PC (Celeron J1900, 4 GB, HDD,
1024×768 touch) with the 80 mm printer, the drawer, a USB scanner on AZERTY and on Arabic, a USB stick; **C** a
Windows 11 PC with an earlier internal build installed and a month of data.

## 1. Build and gates (build PC)

- [ ] `pnpm release:version --check` prints `version 1.0.0` (`package.json`, `src-tauri/tauri.conf.json`, `Cargo.toml`; `Cargo.lock` refreshed); `CHANGELOG.md` starts with `## 1.0.0`; `src/release-notes/1.0.0.json` has its notes in ar, fr and en.
- [ ] `pnpm check` exits 0; `pnpm perf:bundle` ≤ 300 kB; `pnpm test:tooling` green.
- [ ] `pnpm e2e` (local, Edge) green, ar/fr/en screenshots looked at (1024×768 and 1366×768).
- [ ] `pnpm release:build` (x64): `DZBuildPOS-1.0.0-setup.exe` ≤ 15 MiB with its `.sig`, `DZBuildPOS-1.0.0-offline-setup.exe` about 215 MiB, `SHA256SUMS.txt`; the log ends with `owner tool: not built (free edition, no licence to issue)` then `done:`. With the certificate, both show `Get-AuthenticodeSignature` → `Valid` and a timestamp.
- [ ] `pnpm release:build --arch x86` (`rustup target add i686-pc-windows-msvc` once): `DZBuildPOS-1.0.0-x86-setup.exe` ≤ 15 MiB with its `.sig`, `DZBuildPOS-1.0.0-x86-offline-setup.exe` ≥ 100 MiB (the x86 WebView2 runtime inside); `SHA256SUMS.txt` lists the **four** setups and matches `Get-FileHash` for each. The first i686 attempt stopped at "can't open main binary … Access is denied (os error 5)" right after linking: the script now runs `pnpm tauri build` again by itself on that error or `LNK1104` (5 tries, 3 s apart; the log says `the exe was locked (try n of 5)`). If all five fail, exclude `target\i686-pc-windows-msvc` from the antivirus scan for the build and run it again.
- [ ] Each `pnpm release:build` makes two release builds (about 40 minutes for x64, more for a first i686 build). Start it from a terminal of its own, or detached with its output in a log file read until `done:`.
- [ ] The release binaries (`target\release\dzbuild-pos.exe`, `target\i686-pc-windows-msvc\release\dzbuild-pos.exe`) scanned for `dzas_`, `dzpk_live_`, `dzpk_test_`, `client_secret`, `BEGIN PRIVATE`, `AGE-SECRET-KEY`, `dzbuild.dev`: only the known refusal and redaction patterns, no key material, no developer URL shown to users.
- [ ] `pnpm release:manifest` writes `dist-release\1.0.0\updates\stable\latest.json` and `…\beta\latest.json`; notes in the three languages; `platforms` holds `windows-x86_64`, `windows-x86_64-nsis` (URL `…/1.0.0/DZBuildPOS-1.0.0-setup.exe`) and `windows-i686`, `windows-i686-nsis` (URL `…/1.0.0/DZBuildPOS-1.0.0-x86-setup.exe`), each with its own signature, `sha256` and `size`. Without the x86 `.sig` only the x64 pair is written (and the reverse). Needs the owner's certificate for the stable channel.

## 2. Install (machines A and A32, then B)

- [ ] The online setup on A with Internet: the language page offers English, Français, العربية; per-machine install asks for admin; WebView2 is installed by the bootstrapper if missing; Start menu and desktop shortcuts carry the icon.
- [ ] The x86 online setup on A32 with Internet: the same, the x86 WebView2 runtime installed; Task Manager shows `dzbuild-pos.exe` as a 32-bit process on A (x64 Windows runs the x86 setup too).
- [ ] The offline setups (x64 on A, x86 on A32) with the network cable out: install and start with zero network.
- [ ] SmartScreen: on a machine that never ran it, note what Windows shows (signed: publisher "DZBuild"; `docs/release/smartscreen.md`).
- [ ] `Get-NetFirewallRule -DisplayName "DZBuild POS LAN"` → inbound, Private profile, TCP 7345 (and `"DZBuild POS mDNS"` UDP 5353); the WER `LocalDumps\dzbuild-pos.exe` key with `DumpCount` 5 — on A and on A32.
- [ ] First start in each language: **no activation screen**; the wizard offers the demo shop, the import from another PC and the tour; its last step has the one free/website line; the demo opens and leaves; the real setup completes.

## 3. Update (machine C)

- [ ] Point C to the staging manifest: Réglages › Mises à jour › "Rechercher maintenant" finds 1.0.0 with its notes in the UI language.
- [ ] With a drawer open: "Installer maintenant" is disabled and names the till; no download starts by itself.
- [ ] Close the Z, install: a `pre_migration` backup appears (and on the USB stick), the setup runs in passive mode, the app restarts on 1.0.0, "Voir les nouveautés" shows the 1.0.0 notes, the journal has `update.downloaded`, `update.install_started`, `app.updated`. Schema stays 9.
- [ ] An x86 install of an earlier internal build (or a 1.0.0 x86 build pointed at a staging 1.0.1 manifest) takes the **x86** setup: the downloaded file's digest is the `windows-i686` one.
- [ ] Every sale, Z and journal row of the month is still there; the action log check (Réglages › Journal des opérations › "Vérifier") is clean.
- [ ] A shop that used the managed online backup of an earlier internal build: no backup card or notice about it remains, its old journal rows still read with their labels, local backups go on.
- [ ] A manifest with `"paused": true` is not offered; one with a wrong `sha256` downloads and is refused as "incomplete or altered".

## 4. Data safety (machine B)

- [ ] Réglages › Sauvegardes with the USB stick as second copy: pull it, back up, plug it back.
- [ ] "Sauvegarder sur une clé USB maintenant" lists the stick within 3 s and copies to `DZBuild POS\sauvegardes`.
- [ ] "Aperçu et restauration" shows the last ticket of a backup; a restore with the owner PIN restarts on it.
- [ ] Guided restore from the red screen; `backups\corrupt-…` holds the damaged file.
- [ ] Power-cut drill (kill the process right after "Valider"): the sale is whole or absent; the next start journals `db.unclean_shutdown`.
- [ ] Close the app: "Sauvegarde avant fermeture…" then a new `-auto.db`; hourly copies while a session is open.
- [ ] Tray: minimise, restore, "Quitter" on a single PC and on a main PC with a till online; the tray menu in Arabic.
- [ ] Clock drill: set Windows' date one day back, start: the "clock went back" notice and `clock.moved_back` (sales are never blocked by a licence clock in the free edition).

## 5. Performance on the reference PC (machine B, and A32 for x86)

- [ ] cold start (median of 5) ≤ 3 s — `cold_start_ms` in the log (`scripts\perf\bench.ps1`, `docs/release/reference-pc.md`).
- [ ] idle RAM after 60 s on the sale screen ≤ 250 MB (`scripts\perf\idle-ram.ps1`), lower after minimising; the same measured on A32 for the x86 build.
- [ ] SQLite open < 50 ms; scan lookup p95 ≤ 20 ms; sale commit p95 ≤ 80 ms; search on 100 000 products < 100 ms; Arabic receipt render ≤ 300 ms and receipt to paper ≤ 1.5 s.
- [ ] light mode on: no animation, the till grid without images, still usable at 1024×768 with the on-screen keyboard; the "Back to top" button never shows on the till.

## 6. Sale, printing and hardware (machine B)

- [ ] The 12 lines of the M1 real-till smoke (`docs/testing/smoke/m1-real-till-smoke.md`).
- [ ] The hardware lab of M4: `docs/testing/smoke/m4-hardware-lab.md`; a receipt and a label printed from the x86 build on A32 (winspool RAW).
- [ ] The M8 printers and scale: `docs/testing/smoke/m8-field-validation.md` lines 1 to 3.
- [ ] The all-in-one terminal pass (HW-045), the restaurant floor and the table ticket included.

## 7. Back office, documents, LAN, DZBuild

- [ ] M2: `docs/testing/smoke/m2-manual-smoke.md`.
- [ ] M3: `docs/testing/smoke/m3-manual-smoke.md`.
- [ ] M5: `docs/testing/smoke/m5-two-pc-smoke.md` on two PCs (no licence needed any more), one of them the x86 build. The main PC mirrors only its own arch's setup and marks the mirrored manifest with it (`mirror_arch`; no mark = x86_64). A till of the other arch skips the mirror and takes its own setup from dzbuild.com (it needs Internet for that; offline, run its own setup from USB). Check: x86 till behind an x64 main PC is offered the update from dzbuild.com, not from the main PC, and installs it.
- [ ] M7: `docs/testing/smoke/m7-dzbuild-staging.md` (the optional link); Settings › About shows "free" and the "Grow with DZBuild" card opening `https://dzbuild.com/register` and `https://blog.dzbuild.com`.

## 8. Help and languages

- [ ] F1 on five screens in each language opens the right page; the wording is the plain 1.0.0 one ("Journal des opérations", "سجل العمليات", "Action log").
- [ ] Every "Télécharger", support and update link opens `https://dzbuild.com/pos…`; none opens `dzbuild.dev`.
- [ ] The quick card prints on A4 with no browser header or footer; the tour opens from the wizard.
- [ ] The diagnostic zip opens; `LISEZ-MOI.txt` in three languages; no customer name, phone, PIN hash, token or database in it.

## 9. Uninstall (machines A and A32)

- [ ] Uninstall with the box "Supprimer aussi le cache d'affichage" ticked: `%APPDATA%\dev.dzbuild.pos` stays; `%LOCALAPPDATA%\dev.dzbuild.pos` is gone; both firewall rules and the WER key are gone.
- [ ] Reinstall: the app opens on the same shop at the login screen.
- [ ] Installing x64 over x86 (or the reverse) on A keeps the shop's data (`%APPDATA%\dev.dzbuild.pos` is shared).

## 10. Publish (owner)

- [ ] Upload the four setups and `SHA256SUMS.txt` to `https://dzbuild.com/pos/download/1.0.0/`, then the manifests to `https://dzbuild.com/pos/updates/<channel>/latest.json` (last: the manifest makes the release live), as `docs/release/hosting.md` says; `Invoke-RestMethod` the manifest back and check its four platform keys.
- [ ] The download page `https://dzbuild.com/pos` offers the 64-bit and the 32-bit setups (online and offline) and says the POS is free.
- [ ] Staged rollout: `--rollout 10`, then 50, then 100; a problem: `--pause`.
- [ ] Tag `v1.0.0`; the release workflow builds both arches (x64 then x86 in one job) and drafts the GitHub release.

## 11. Free edition (machine A)

- [ ] A new install never shows the activation screen, the licence banner or Settings › Licence, in the three languages.
- [ ] The clock moved 8 days ahead: sales go on (no trial, no read-only mode).
- [ ] An install of an earlier internal build holding a trial or a lifetime licence, updated to 1.0.0: sells, no licence screen, its data intact.
- [ ] `dist-owner\1.0.0\` does not exist after both builds; no licence key, seller number or internal DZBuild URL in the app or the setups.

## 12. Vertical packs (M8)

- [ ] A fresh install: the wizard's activity step lists the 25 templates in ar, fr and en; a restaurant template opens the floor at `/pos` (Ctrl+1); a butcher template shows the scale step.
- [ ] The owner's field lines: `docs/testing/smoke/m8-field-validation.md`.

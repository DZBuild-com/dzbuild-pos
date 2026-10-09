# M5 multi-till LAN: two-PC checklist

Run it on two real PCs and fill in this file. No automated test uses two machines, a real switch, mDNS, the Windows firewall or
a pulled power plug: the tests run a main PC and a till as two data folders on one PC over loopback
(`src-tauri/src/lan/two_pcs.rs`), so this checklist is the only place where the LAN is proved.

**Status: not run.** None of the 15 lines has been run yet.

## How to run it

- **Machines:** the main PC is the reference PC: Celeron J1900, 4 GB RAM, HDD, 1024×768 touch screen,
  Windows 10 or 11 x64, with an 80 mm receipt printer and a USB scanner. The till is any Windows 10 1809+
  or 11 x64 PC with a receipt printer. Both are cabled to one switch (no Wi-Fi between them for the timed
  lines), on the same subnet. Line 14 needs a second till on the J1900: any third Windows PC.
- **Build:** `DZBuild POS_0.5.0_x64-setup.exe` (unsigned: SmartScreen asks for "Exécuter quand même"),
  installed from the same file on both PCs. Check its SHA-256 against the build record below before
  installing. The installer does not add the firewall rules yet (PLT-008 is M6's NSIS hook): line 1 adds
  them by hand.
- **Data to bring:** on the main PC, a shop with 10 000 products and 20 000 tickets (line 2; the app ships
  no generator for them), an owner, a manager and a cashier with their PINs, and a customer with a kridi
  limit (line 5). The till starts as a fresh install.
- **Order:** run the 15 lines in Arabic, then again in French. Switch the language with the AR/FR/EN
  toggles in the top bar of each PC. The lines that pair or change a PC's role (2, 3, 11, 12, 13) run
  once: check them in Arabic, then switch to French and look at the same screens again. Run line 14
  before line 11, which switches the J1900 off as the main PC; lines 12 and 13 then use line 11's new
  main PC and its till.
- **Labels:** the lines quote the French labels. In Arabic they read: "جعل هذا الجهاز الجهاز الرئيسي"
  (Faire de ce PC le poste principal), "إعادة الفحص" (Vérifier à nouveau), "جهاز بيع لمحل" (Poste de
  caisse d'une boutique), "اختيار" (Choisir), "متصل" (Connecté), "يستمر البيع دون اتصال." (La vente
  continue hors ligne), "سليم" (Intact), "فرق 120 دقيقة" (120 min d'écart), "ساعة جهاز غير مضبوطة"
  (Heure d'un poste décalée), "إقفال Z دون اتصال" (Z hors ligne, and the alert Z fait hors ligne), "عرض
  إقفالات Z" (Voir les Z), "إغلاق الجهاز الرئيسي؟" (Fermer le poste principal ?), "مغادرة الشبكة"
  (Quitter le réseau), "تغيير الشبكة" (Changement de réseau), "يحتاج مراجعة" (À vérifier). Settings ›
  Network is Réglages › Réseau in French and الإعدادات › الشبكة in Arabic.
- **Timing:** time each budget with a stopwatch, from the action to the result on the other PC's screen.
  Write the measured value, not only OK. Read the RAM and the CPU (line 14) in the Task Manager's
  Details tab: add `dzbuild-pos.exe` and all its `msedgewebview2.exe` children.
- **Secrets:** the capture of line 3 and the searches of lines 3 and 15 handle a pairing code and the
  first characters of a till's token. Never write them into this file, a photo or a ticket; delete the
  capture and `pair.json` once line 3 is done.
- **KO:** a KO line opens a fix task first, with an automated regression test over the loopback
  harness (`two_pcs.rs`) wherever one can reproduce it. The line is run again once the fix is merged.

## Checklist

1. Main PC: Réglages › Réseau › "Faire de ce PC le poste principal"; the firewall card says the rule is
   missing; an administrator runs the two `netsh` lines from the card; "Vérifier à nouveau" turns green;
   the network profile shows private.
2. Till (fresh install): "Poste de caisse d'une boutique" → the main PC is found by mDNS within 3 s;
   "Choisir"; code typed from the main PC's dialog; the copy (a shop with 10 000 products and 20 000
   tickets) downloads with its progress bar in ≤ 60 s; the till restarts as `C2` and signs in with a
   cashier's PIN made on the main PC.
3. A wrong code 5 times burns it (the main PC shows a new one only when asked); the same pairing request
   replayed from a capture (`curl` with the saved body) is refused; the till's token is in Windows
   Credential Manager (`DZBuild POS/lan-till-token`) and nowhere in `%APPDATA%\dev.dzbuild.pos` (search
   the folder for the token's first 8 characters).
4. Online sale on the till: it appears in the main PC's sales history within 2 s; the pill says
   "Connecté"; the main PC's pill counts the till.
5. Unplug the main PC's network cable: the till's banner says "La vente continue hors ligne", 20 sales go
   through at the usual speed (≤ 1 s each), the pill counts what is left to send; a kridi sale is refused
   with the message; a product created on the main PC meanwhile is not on the till.
6. Plug the cable back: the 20 sales reach the main PC in ≤ 10 s; the stock of the products sold is the
   same on both PCs; the product created meanwhile appears on the till (and can be scanned).
7. Pull the till's power plug in the middle of step 6 (sales pending): after the reboot the till sends
   again; the main PC shows each ticket once; the till's journal is "Intact" in the tills table.
8. Set the till's Windows clock 2 hours ahead while online: the till's banner warns within a minute; the
   main PC's tills table shows "120 min d'écart" and a "Heure d'un poste décalée" alert; a new drawer
   does not open on the till until the clock is fixed; tickets already printed keep their time on the
   main PC.
9. Z on the till while the main PC is down: "Z hors ligne" with a manager's PIN; the printed Z says "Z
   hors ligne — opérations non envoyées"; when the main PC is back, its "Voir les Z" for `C2` marks it
   offline and an alert "Z fait hors ligne" is open.
10. Close the main PC's window with the till online: "Fermer le poste principal ?" appears; Annuler keeps
    it open; a second click on the close button within a minute closes it.
11. On the till, "Faire de ce poste le poste principal" with the old main PC still on is refused ("Le
    poste principal répond encore"); switch the old main PC off, retry: the till restarts as a main PC
    and a second till joins it with a new code.
12. "Quitter le réseau" on a till with nothing to send: the PC restarts as a new PC; its old copy is in
    the backups list as "Changement de réseau".
13. Revoke a till on the main PC: within one cycle the till's pill says "À vérifier" and nothing more
    goes through; it keeps selling offline.
14. Idle RAM with the main PC serving two tills ≤ 280 MB on the J1900 (spec §2: 250 MB for the app, 30 MB
    for the server); CPU at rest ≤ 2 %.
15. Logs of both PCs (`%APPDATA%\dev.dzbuild.pos\logs`) contain no token, code, PIN, hash or row image
    (search for the token and the code).

Line 1: the card's two lines are `pos_server::firewall::install_commands`: the rules `DZBuild POS LAN`
(TCP 7345) and `DZBuild POS mDNS` (UDP 5353), Private profile only, for the installed `DZBuild POS.exe`.
Run them in a Command Prompt opened as administrator ("Copier" on the card copies each one). If the
network shows as public, switch it to private in Windows Settings › Network › Properties first. Windows
may also show its own "Alerte de sécurité Windows" the first time the main PC listens: write down what
was clicked. Cancelling it can add a block rule for the program, and a block rule wins over the two
allow rules (look for one in `wf.msc` › Règles de trafic entrant).

Line 2: the copy's 60 s run from "Rejoindre" to the restart. If the main PC is not found within 3 s,
note the time it took (or that it never came) and go on with "Taper l'adresse" and the address the main
PC shows in Réglages › Réseau: that is a KO of the 3 s budget, not of the join.

Line 3, the replay: capture the till's `POST /pair` during line 2 with Wireshark on either PC (filter
`tcp.port == 7345`, Follow › HTTP Stream) and save the request body as `pair.json`. Once the till has
joined, send it again from the till:

```powershell
curl.exe -i -X POST "http://<main PC address>:7345/pair" -H "Content-Type: application/json" --data-binary "@pair.json"
```

The main PC answers `409` with the code `pairing_code_invalid` (a code is spent at its first use).
More than 10 tries a minute from one address answer `429`. Use `curl.exe`, not `curl`, in Windows
PowerShell 5.1 (`curl` is an alias of `Invoke-WebRequest` there).

Line 3, the token: Credential Manager (Panneau de configuration › Gestionnaire d'identification ›
Informations d'identification Windows) lists the generic credential `DZBuild POS/lan-till-token` but
never shows its value. Read only its first 8 characters on the till, with DZBuild POS closed, then
search the data folder in both encodings:

```powershell
Add-Type -Namespace Cred -Name Api -MemberDefinition @'
[DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
public static extern bool CredRead(string target, int type, int flags, out IntPtr cred);
[DllImport("advapi32.dll")]
public static extern void CredFree(IntPtr cred);
'@
$p = [IntPtr]::Zero
if ([Cred.Api]::CredRead('DZBuild POS/lan-till-token', 1, 0, [ref]$p)) {
  $size = [Runtime.InteropServices.Marshal]::ReadInt32($p, 32)
  $blob = [Runtime.InteropServices.Marshal]::ReadIntPtr($p, 40)
  $first8 = [Runtime.InteropServices.Marshal]::PtrToStringUni($blob, $size / 2).Substring(0, 8)
  [Cred.Api]::CredFree($p)
  $files = Get-ChildItem "$env:APPDATA\dev.dzbuild.pos" -Recurse -File
  $files | Select-String -SimpleMatch $first8 -List | Select-Object Path
  $files | Select-String -SimpleMatch $first8 -List -Encoding Unicode | Select-Object Path
} else { "No till token on this PC" }
```

Both searches print nothing when the line is OK. The token is 64 hexadecimal characters; the main PC
keeps only its SHA-256, which does not contain them.

Line 3, the PIN hashes (a known trade-off, not a line to tick): each till holds a copy of every user's
PIN hash (Argon2id) so that a cashier signs in on a till whose main PC is down. Argon2id makes guessing
a 4 to 8 digit PIN from a stolen copy slow, not impossible. If a till PC is lost or stolen, the owner
revokes it in the tills table and changes every PIN on the main PC: the new hashes reach the other tills
at their next feed.

Line 5: the kridi refusal is the error `kridi_needs_host` ("Le crédit client (kridi) se fait quand le
poste principal répond…" in French, "البيع بالكريدي يتطلب استجابة الجهاز الرئيسي…" in Arabic). Cash, card
and cheque sales go through.

Line 7: pull the till's plug while the pill still counts sales to send (unplug the till's cable first
if the 20 sales leave too fast, then plug it back and pull the power at once). Count the tickets of
`C2` on the main PC's sales history before and after: each number appears once.

Line 8: "a new drawer does not open" means opening a cash session on the till is refused while it is
connected with its clock more than 15 minutes off (`lan_clock_skew`). Sales are never blocked by the
clock. Set the clock back at the end of the line, before line 9.

Line 13: one cycle is at most 30 s on an idle till; a sale or "Envoyer et recevoir maintenant" starts
one at once.

Line 14: pair the third PC as `C3` with a new code first. Read the RAM after 10 minutes idle on the
sale screen of the main PC, with both tills online and signed in, and the CPU over one minute.

Line 15: on each PC, search the `logs` folder for the 8 characters of line 3, the pairing codes used in
lines 2, 3 and 11, the owner's PIN and `$argon2id$` (the start of every PIN hash), for example
`Get-ChildItem "$env:APPDATA\dev.dzbuild.pos\logs" -Recurse -File | Select-String -SimpleMatch
'$argon2id$'`. Every search must print nothing.

## Record

Run by: (to fill) · Date: (to fill) · Main PC (Windows version, WebView2 runtime version, receipt
printer, scanner): (to fill) · Till PC (model, Windows version): (to fill) · Switch: (to fill) ·
Installer SHA-256 checked: (to fill)

Write the measured value and OK or KO in each language cell, for example `1.4 s OK`.

| #   | Budget                                                                    | Arabic    | French    |
| --- | ------------------------------------------------------------------------- | --------- | --------- |
| 1   | main PC role; firewall card red then green after the two lines; private   | (to fill) | (to fill) |
| 2   | found by mDNS ≤ 3 s; copy ≤ 60 s with progress; `C2`; cashier PIN works   | (to fill) | (to fill) |
| 3   | 5 wrong codes burn it; replayed request 409; token only in Cred. Manager  | (to fill) | (to fill) |
| 4   | online sale on the main PC ≤ 2 s; "Connecté"; main PC counts the till     | (to fill) | (to fill) |
| 5   | offline banner; 20 sales ≤ 1 s each; pill counts; kridi refused; no new   | (to fill) | (to fill) |
| 6   | 20 sales on the main PC ≤ 10 s; same stock; new product scanned on till   | (to fill) | (to fill) |
| 7   | power cut mid-send: resent, each ticket once, journal "Intact"            | (to fill) | (to fill) |
| 8   | banner ≤ 1 min; "120 min d'écart" and alert; no drawer; times kept        | (to fill) | (to fill) |
| 9   | offline Z with manager PIN; printed mention; marked offline; alert open   | (to fill) | (to fill) |
| 10  | close held once, Annuler keeps it, second click closes                    | (to fill) | (to fill) |
| 11  | promote refused while the old main PC answers; then promoted; new till    | (to fill) | (to fill) |
| 12  | leave: restarts as a new PC; backup "Changement de réseau"                | (to fill) | (to fill) |
| 13  | revoked: "À vérifier" within one cycle; nothing sent; still sells         | (to fill) | (to fill) |
| 14  | idle RAM ≤ 280 MB serving two tills; CPU at rest ≤ 2 %                    | (to fill) | (to fill) |
| 15  | no token, code, PIN, hash or row image in either PC's logs                | (to fill) | (to fill) |

## Build record (2026-10-01, build PC)

- **Status: open.** None of the 15 lines has been run. The build PC is a single PC, not the reference PC:
  an Intel Core i7-1255U with 16 GB of RAM and an SSD, no second PC, no switch, no receipt printer. Its
  timings and RAM readings would not answer the budgets. The automated cover below is interim evidence
  only. It does not replace the run on two PCs.
- Build PC: Windows 11 Pro 10.0.26200 x64, WebView2 runtime 154.0.4258.48; rustc 1.98.1, Node 24.15.0,
  pnpm 10.32.1. The gates and the build ran on the tree of the commit that adds this file, branch
  `lane/m5-app` (the M5 app lane). Only this file was edited after the build.
- Gates, all exit 0 except where noted: `cargo test -p dzbuild-pos lan::tests::the_firewall_lines` (1
  passed at once: it pins the firewall lines and the main PC's port together);
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace` (dzbuild-pos 199 passed and 2 ignored, pos-core 1048 passed, 2 failed and 13
  ignored, pos-docs 78 passed and 1 ignored, pos-hw 216 passed and 1 ignored, pos-server 41 passed and 1
  ignored; see the load notes); `write_bindings` then `git diff --exit-code src/bindings.ts` (no
  change); `pnpm exec tsc --noEmit`; `pnpm lint`; `pnpm exec prettier --check .`;
  `node scripts/i18n/check-i18n.mjs`; `pnpm exec vitest run` (73 files, 473 tests; see the load notes).
- Load notes. Other checkouts ran their own Rust builds on this PC at the same time (11 `rustc` and `cargo`
  processes during the Vitest run). Under that load:
  - `pos-core` `sales::robustness::sale_commit_p95_stays_within_budget` measured a p95 of 302 ms in the
    workspace run and passed alone. `pos-core` `cloud::engine::tests::not_ready_raises_one_notice_and_retries_daily`
    (M7 core, untouched by M5) failed in the workspace run, once more next to the p95 test, then passed
    four times in five alone: it sleeps 500 ms and expects the engine to have published a cycle by
    then. The workspace run stops at the first failing crate, so `pos-docs`, `pos-hw` and `pos-server`
    ran in a second `cargo test` (all passed).
  - The full Vitest run failed 11 tests in 8 files, all on their 15 s or 20 s timeouts or on a `findBy`
    wait: `shift`, `scanner`, `history`, `returns`, `till-hardware`, `products`, `stock-m2` and
    `SetupWizard`. Together they then failed 3; alone, `products.test.tsx` (14) and `shift.test.tsx`
    (4) passed, and `scanner.test.tsx` (9) passed in two runs of three: its HW-022 test reads the time
    between the keys of a burst.
- `pnpm tauri build`: exit 0 (release build 96 min 44 s, 99 min 38 s in all, under the load above). It
  wrote `target\release\bundle\nsis\DZBuild POS_0.5.0_x64-setup.exe`: unsigned, 9 737 642 bytes
  (9.29 MiB), SHA-256 `53119aa5213f48078fc30d164ffbf808a1c60e70cde6c5aa77fdbbcf959ac790`.
  `dzbuild-pos.exe` reports product version 0.5.0. Vite warns that the main JavaScript chunk is larger
  than 500 kB (1 086 kB, 290 kB gzip).

## Automated cover on one PC

What `cargo test --workspace` and `pnpm exec vitest run` already prove with the loopback harness and
mocked IPC. It is interim evidence only and does not replace a line of the run above.

- **End to end:** app `lan::smoke_tests::selling_continues_when_the_main_pc_is_down`,
  `a_till_killed_mid_replay_resends_and_the_host_keeps_one_copy` and
  `a_two_hour_skew_keeps_the_receipt_times_and_is_recorded_on_both_sides` run a main PC and a till as
  two data folders over loopback (`lan/two_pcs.rs`); `commands::shift_tests` runs the M1 shift alone,
  on a main PC serving a till, on a till, and on a till whose main PC is down;
  `src/app/shift.test.tsx` walks the same four places in the interface.

1. App `commands::lan::tests::a_single_pc_becomes_the_main_pc_hands_out_codes_and_comes_back`,
   `lan::tests::the_firewall_lines_open_the_port_the_main_pc_listens_on`; `pos-server` `firewall` tests
   (`the_installer_rules_are_exact`, `the_tills_reach_this_pc_only_when_both_rules_are_present`,
   `a_public_network_is_flagged`, `an_mdns_rule_for_the_public_profile_only_is_not_ok`);
   `network-settings.test.tsx` ("explains a blocked firewall and gives the administrator's commands",
   "says which firewall rule is missing and gives only its command"). No test runs `netsh` or reads the
   real firewall.
2. `pos-server` `discovery::tests::a_main_pc_is_found_by_a_till` (ignored: it needs multicast on a real
   interface; on the reference PC run
   `cargo test -p pos-server a_main_pc_is_found_by_a_till -- --ignored`), `client::join` tests
   (`a_new_till_joins_downloads_and_adopts`), `host::routes` `the_snapshot_streams_with_its_seq_and_hash`;
   `pos-core` `sync::snapshot::tests::a_snapshot_is_a_sanitised_copy_with_its_seq`,
   `sync::replica` install tests; app `lan::tests::a_main_pc_serves_and_a_new_till_starts_from_its_copy`;
   `join.test.tsx` ("finds the main PC, joins with the code and restarts", "types the address when
   nothing answers on the network"). No test times a copy of 10 000 products and 20 000 tickets.
3. `pos-core` `sync::pairing::tests::five_wrong_codes_burn_the_code`,
   `a_pairing_code_works_once_and_a_replayed_request_is_refused`,
   `sync::devices::tests::a_replayed_pairing_request_finds_its_code_spent`,
   `tokens_are_stored_hashed_and_revocation_is_immediate`; `pos-server`
   `host::routes` `a_code_becomes_a_token_once`, `pair_is_rate_limited_per_address`; app
   `lan::vault::tests::the_memory_vault_keeps_one_token`, `the_token_has_its_own_credential`
   (`the_credential_vault_round_trips` is ignored: it touches the user's Credential Manager),
   `lan::tests::a_till_without_its_token_says_so`.
4. `pos-core` `sync::replay` tests (`a_till_sale_is_recorded_on_the_main_pc_as_it_was_made`,
   `the_host_chain_equals_the_till_chain`); `pos-server`
   `client::engine` `the_engine_replays_then_pulls_and_reports_its_status`,
   `host::routes` `the_feed_socket_pokes_when_the_main_pc_writes`; `lan-shell.test.tsx` ("counts the
   tills on the main PC", "an online till shows no banner"). No test times the 2 s.
5. App `lan::smoke_tests::selling_continues_when_the_main_pc_is_down`,
   `commands::lan_rules_tests::kridi_on_an_offline_till_is_refused_and_cash_sales_are_not`; `pos-server`
   `client::engine` `a_sale_commits_locally_when_the_host_drops_mid_push`,
   `client::socket_lost_tests::a_till_hears_at_once_that_its_main_pc_went_away`; `pos-core`
   `sync::outbox::tests::the_sale_and_its_outbox_item_commit_together`; `lan-shell.test.tsx` ("a till
   offline shows the pill and the banner with what is left to send").
6. App `lan::smoke_tests::selling_continues_when_the_main_pc_is_down` (the sales arrive, the stock agrees,
   the product made meanwhile reaches the till); `pos-core` `sync::apply` tests
   (`a_till_mirrors_the_catalogue_and_finds_it_by_search`), `sync::replay::tests::
   an_offline_sale_that_took_the_last_piece_twice_is_recorded_with_a_notice`. No test times the 10 s.
7. App `lan::smoke_tests::a_till_killed_mid_replay_resends_and_the_host_keeps_one_copy`; `pos-core`
   `sync::replay::tests::a_bundle_replayed_twice_is_applied_once`, `a_lost_answer_is_given_again_from_the_inbox`,
   `sync::journal::tests::a_tills_journal_is_checked_on_arrival_and_chained_again_by_the_host`. No test
   cuts the power of a real disk.
8. App `lan::smoke_tests::a_two_hour_skew_keeps_the_receipt_times_and_is_recorded_on_both_sides`;
   `pos-core` `sync::skew` tests, `sync::replay::tests::
   the_host_records_its_reception_time_and_the_skew_but_keeps_the_ticket_time`,
   `sync::replay_skew_tests`; `pos-server` `client::sync_now_tests`, `host::touch_tests`;
   `lan-shell.test.tsx` ("warns when the till's clock is off by more than 5 minutes").
9. App `commands::lan_rules_tests::the_z_waits_for_the_outbox_or_is_closed_offline_with_an_approval`,
   `commands::shift_tests::the_same_shift_on_a_till_whose_main_pc_is_down_reaches_it_later`; `pos-core`
   `sync::tills::tests::a_tills_z_is_listed_and_verified_on_the_main_pc`;
   `offline-z-primary.test.tsx`, `shift.test.tsx`, `network-settings.test.tsx` ("lists the tills with
   what needs checking, shows a till's Z and removes a till").
10. App `lan::close_tests::the_main_pc_holds_its_first_close_while_tills_are_online`;
    `lan-shell.test.tsx` ("asks before closing the main PC and closes on confirmation").
11. App `commands::lan::tests::promote_and_leave_need_the_owners_pin_and_a_till`; `pos-core`
    `sync::role::tests::a_till_promoted_to_main_pc_keeps_its_rows_and_sheds_its_guards`; `join.test.tsx`
    ("makes this till the main PC only with the owner's PIN, and says when the old one still answers").
12. `pos-core` `sync::role::tests::a_till_leaves_only_with_an_empty_outbox`,
    `sync::replica::tests::leaving_archives_the_tills_copy_at_the_next_start`; app
    `lan::tests::a_till_is_written_down_in_local_json_until_it_leaves`.
13. `pos-server` `client::engine` `a_revoked_till_stops_and_says_so_but_keeps_selling`,
    `host::routes` `a_revoked_token_is_refused_on_every_route`; `network-settings.test.tsx` (removes a
    till).
14. None. It needs the installed app on the reference PC.
15. `pos-core` `sync::devices::tests::a_replayed_pairing_request_finds_its_code_spent` (the `Debug` of
    a paired till hides its token); app
    `commands::tests::login_input_debug_hides_the_pin`. No test reads the log files.

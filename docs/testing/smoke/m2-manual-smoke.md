# M2 back office: manual smoke

The person who runs it fills in this file. `m2` is tagged only when every line reads OK in both languages.

## How to run it

- **Machine:** the reference PC: Celeron J1900, 4 GB RAM, HDD, 1024×768 touch screen, Windows 10 or 11
  x64, with an 80 mm ESC/POS printer and a USB barcode scanner.
- **Build:** `DZBuild POS_0.2.0_x64-setup.exe`, installed over the M1 build (`0.1.0`) that already holds
  a shop's data. Check its SHA-256 against the record below before installing.
- **Order:** run the 11 lines in Arabic, then run them again in French. Switch the language with the
  AR/FR/EN toggles in the top bar. The upgrade in line 1 happens only once: check it in Arabic, then
  switch to French and look at the same screens again.
- **Data to bring:**
  - an xlsx of 50 000 products exported by a competitor's software, and the same file saved as a
    Windows-1256 CSV (line 2);
  - one year of data with 100 000 tickets (line 6);
  - a USB stick with product photos (line 8);
  - a second PC with version 0.2.0 installed (line 9).
- **Timing:** time each budget with a stopwatch from the click to the result on screen. Write the
  measured value, not only OK. Read the RAM (line 11) in the Task Manager's Details tab. Add the
  `dzbuild-pos.exe` process and its `msedgewebview2.exe` children.
- **KO:** a KO line opens a fix task first, with an automated regression test wherever one is possible.
  `m2` is tagged only after that task is done and the line is run again.

## Checklist

1. Upgrade in place from the M1 build with data: the pre-migration backup is listed, migration 0003
   runs, every M1 ticket, Z and audit row is still there, `documents_verify_chain` is clean.
2. Import 50 000 products from an xlsx exported by a competitor (Windows-1256 CSV variant too): dry
   run ≤ 60 s, apply ≤ 3 min, tills keep selling during the apply (a sale every 10 s on a second
   session stays under 1 s), undo works the same day.
3. Customer at the till: F3, search by phone typed with Arabic digits, credit sale above the limit →
   manager PIN → receipt shows the customer and the balance; versement in cash with the duty printed on
   the RV receipt; statement printed on the 80 mm printer; the Z shows "Versements clients".
4. Purchase order → two partial receipts with a lot and an expiry → supplier invoice → payment half
   from the drawer: the X report shows the paid-out; the CMUP history shows two changes.
5. Blind count of a category with the scanner (200 scans), close with differences: the P&L "stock
   losses" line shows the value.
6. Every report of the catalogue opens in ≤ 2 s on one year of data (100 000 tickets), exports to xlsx
   in ar (right-to-left sheet, Arabic headers) and to csv in fr (opens in Excel with accents and
   decimal commas).
7. Dashboard renders in ≤ 1 s at start; charts readable in light and dark themes; RTL mirrored in
   Arabic.
8. Product photo from a USB stick shows in the product list and the till grid; the webview has no file
   access (DevTools `fetch("file:///C:/")` fails; an `asset:` URL outside `media/` is refused).
9. `.dzpos` export on one PC, import on another, restore from the M0 backups screen: the shop reopens
   with its customers and balances.
10. Full data export zip: one CSV per table, no `pin_hash`, `LISEZ-MOI.txt` in three languages.
11. Idle RAM with the dashboard and a 10 000-row customer list open ≤ 250 MB (spec §2).

Line 2: the app runs as a single instance, and several tills on a LAN come only with M5. So the
"second session" is the sale screen of the same app. Open it from the navigation once the apply has
started, and time each sale there.

Line 8: the installed release build has no DevTools, because Tauri enables them only in debug builds
and the app switches F12 off. Check the photos in the installed build. Then run the two `fetch` checks
in a debug build of the same commit (`pnpm tauri build --debug`) on the reference PC: right-click ›
Inspect › Console.

## Record

Run by: (to fill) · Date: (to fill) · Reference PC (Windows version, printer model and port, scanner
model and keyboard layout): (to fill) · Installer SHA-256 checked: (to fill)

Write the measured value and OK or KO in each language cell, for example `41 s OK`.

| #   | Budget                                                     | Arabic      | French      |
| --- | ---------------------------------------------------------- | ----------- | ----------- |
| 1   | backup listed, 0003 applied, chain clean                   | (to fill)   | (to fill)   |
| 2   | dry run ≤ 60 s, apply ≤ 3 min, till sale < 1 s, undo       | (to fill)   | (to fill)   |
| 3   | approval, customer and balance on the ticket, duty, Z line | (to fill)   | (to fill)   |
| 4   | paid-out on the X, two CMUP changes                        | (to fill)   | (to fill)   |
| 5   | 200 scans, stock losses value on the P&L                   | (to fill)   | (to fill)   |
| 6   | each report ≤ 2 s, xlsx RTL in ar, csv in fr               | (to fill)   | (to fill)   |
| 7   | dashboard ≤ 1 s, light and dark, RTL                       | (to fill)   | (to fill)   |
| 8   | photo in the list and the grid, no file access             | (to fill)   | (to fill)   |
| 9   | shop reopens with customers and balances                   | (to fill)   | (to fill)   |
| 10  | one CSV per table, no `pin_hash`, trilingual readme        | (to fill)   | (to fill)   |
| 11  | idle RAM ≤ 250 MB                                          | (to fill)   | (to fill)   |

## Build record (2026-09-30, build PC)

- **Status: open.** None of the 11 lines has been run, so `m2` is not tagged. The build PC is not the
  reference PC: it has an Intel Core i7-1255U, 16 GB of RAM and an SSD, and no receipt printer or
  scanner is attached. Its timings and RAM readings would not answer the budgets. The automated cover
  below is interim evidence only. It does not replace the run on the reference PC.
- Build PC: Windows 11 Pro 10.0.26200 x64; rustc 1.98.1, Node 24.15.0, pnpm 10.32.1. The gates and
  the build ran on the tree of the commit that adds this file. Only this file was edited after the
  build.
- Gates, all exit 0: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace` (dzbuild-pos 119 passed and 1 ignored, pos-core 545 passed and 5 ignored,
  pos-docs 46, pos-hw 209, pos-server 1); `write_bindings` then `git diff --exit-code src/bindings.ts`
  (no change); `pnpm exec tsc --noEmit`; `pnpm lint`; `pnpm exec prettier --check .`;
  `pnpm exec vitest run` (48 files, 305 tests); `node --test scripts/*.test.mjs` (6);
  `node scripts/i18n/check-i18n.mjs`. Two earlier Vitest runs failed 4 and then 2 timing tests
  (`scanner.test.tsx` HW-022 burst, and the `shift.test.tsx` 20 s timeout). Other builds were keeping
  the CPU at 96 % at the time. The same files pass alone, and the full suite passed once the load
  dropped.
- `pnpm tauri build`: exit 0 (release build 16 min 58 s, 18 min 22 s in all). It wrote
  `target\release\bundle\nsis\DZBuild POS_0.2.0_x64-setup.exe`: unsigned, 7 591 980 bytes (7.24 MiB),
  SHA-256 `a6838b353514cdccece00d477130b693ff3d65ac19bd8b4991adb747bd8db841`. `dzbuild-pos.exe`
  reports product version 0.2.0. Vite warns that the single JavaScript chunk is larger than 500 kB
  (1 439 kB, 411 kB gzip).

Automated cover of each line (in `cargo test --workspace` and `pnpm exec vitest run`). No test measures
a budget on the reference PC.

1. `pos-core` `db::m2_tests::the_m1_fixture_upgrades_in_place_after_a_backup`: the M1 fixture database
   is backed up, then upgraded by 0003; its ticket and stock are still there, and the audit chain and
   the document chains verify.
2. `pos-core` `imports::tests::a_large_file_is_applied_in_batches_of_500_each_journaled_with_the_job_id`,
   `a_dry_run_writes_nothing`, `undo_restores_updated_products_and_archives_created_ones`; `pos-docs`
   `import::tests::windows_1256_arabic_csv_is_decoded_and_the_delimiter_sniffed`;
   `import-wizard.test.tsx`. No test times 50 000 rows on an HDD.
3. `pos-core` `sales::account_tests::credit_above_the_limit_needs_a_manager_approval`,
   `customers::credit::tests::a_cash_versement_pays_the_stamp_duty_on_the_cash_it_brings`; app
   `commands::m2_tests::a_ticket_on_credit_carries_its_customer_and_the_balance_after_it`,
   `the_x_and_the_z_print_the_cash_versements_counted_in_the_expected_cash`, `slips::tests`;
   `till-m2.test.tsx`, `digits.test.ts`.
4. `pos-core` `purchases::tests::an_order_is_drafted_edited_validated_and_received_in_two_parts`,
   `a_receipt_can_record_the_supplier_invoice_and_pay_it_from_the_till`,
   `stock::tests::each_receipt_moves_the_cmup_and_writes_its_history`; `purchases.test.tsx`.
5. `pos-core` `stock::counts::tests::a_count_posts_the_difference_with_the_level_at_close`,
   `reports::tests::a_count_report_values_the_differences_at_cost`; app
   `commands::m2_month::a_whole_month_balances_to_the_centime` (the P&L stock losses);
   `stock-m2.test.tsx`.
6. App `commands::m2_month::a_whole_month_balances_to_the_centime`: the 34 report kinds run in ar, fr
   and en, exported to xlsx and csv and read back. `pos-docs` `sheet::tests`; `reports.test.tsx`. No
   test times the reports on 100 000 tickets.
7. `pos-core` `reports::tests::the_dashboard_shows_today_and_hides_what_the_viewer_may_not_see`;
   `dashboard.test.tsx`, `bar-chart.test.tsx`. jsdom does no layout and no timing.
8. App `commands::m2_tests::a_product_photo_is_resized_into_media_and_nowhere_else`,
   `a_folder_of_photos_named_by_barcode_is_imported_and_the_rest_reported`; `media.test.ts`,
   `extras.test.tsx`. The asset scope `$APPDATA/media/**/*` and the CSP are set in
   `src-tauri/tauri.conf.json`. No test reaches WebView2.
9. `pos-docs` `archive::tests::a_backup_file_round_trips_into_the_backups_folder`,
   `a_tampered_or_foreign_file_is_refused`; app
   `commands::m2_tests::backup_files_and_the_full_export_need_settings_data_and_are_journaled`;
   the `pos-core` `db::backup` restore tests.
10. `pos-docs` `archive::tests::the_full_export_has_one_csv_per_table_a_manifest_and_a_readme` (no
    `pin_hash` in `users.csv`).
11. None. It needs the installed app on the reference PC.


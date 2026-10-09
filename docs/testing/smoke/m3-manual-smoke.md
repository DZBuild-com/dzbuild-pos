# M3 documents and fiscal: manual smoke

The person who runs it fills in this file. `m3` is tagged only when every line reads OK in both languages.

## How to run it

- **Machine:** the reference PC: Celeron J1900, 4 GB RAM, HDD, 1024×768 touch screen, Windows 10 or 11
  x64, with an A4 laser printer, an 80 mm ESC/POS printer and a USB barcode scanner.
- **Build:** `DZBuild POS_0.3.0_x64-setup.exe`, installed over the M2 build (`0.2.0`) that already holds
  a shop's data with tickets. Check its SHA-256 against the build record below before installing.
- **Order:** run the 15 lines in Arabic, then run them again in French. Switch the language with the
  AR/FR/EN toggles in the top bar. The upgrade in line 1 happens only once: check it in Arabic, then
  switch to French and look at the same screens again.
- **Data to bring:**
  - the M2 shop's data with its tickets, already on the reference PC under 0.2.0 (line 1);
  - Microsoft Edge and a PDF reader (line 3), 7-Zip (line 12);
  - a second PC, or a copy of the shop's data, for the IFU test (line 4);
  - a database with 20 000 documents (line 10) and one year of data with 100 000 tickets (line 12).
    The app ships no generator for them.
- **Timing:** time each budget with a stopwatch from the click to the result on screen, or to the sheet
  leaving the printer for the prints. Write the measured value, not only OK. Read the RAM (lines 12 and
  14) in the Task Manager's Details tab. Add the `dzbuild-pos.exe` process and its `msedgewebview2.exe`
  children.
- **KO:** a KO line opens a fix task first, with an automated regression test wherever one is possible.
  `m3` is tagged only after that task is done and the line is run again.

## Checklist

1. Upgrade in place from 0.2.0: the pre-migration backup is listed, migration 0004 runs,
   `documents_verify_chain` and the audit chain are clean, every M2 ticket reprints identical to its
   original apart from "DUPLICATA".
2. Shop settings: activity in three languages, RC, NIF, NIS, AI, capital; `/settings/documents` preview
   follows accent, paper, texts and language live.
3. Invoice typed at the desk for "SARL Amine" (19 % and 9 % lines), second language Arabic: blocked
   while the customer's RC is missing, validated once completed, paid 5 000 DA cash + rest on account:
   duty printed as its own line, "Net à payer" right, the balance on the customer's account; printed on
   the A4 laser in ≤ 5 s from the click (first print of the session ≤ 10 s), bilingual layout readable,
   Arabic right to left, numbers left to right; reprint prints "DUPLICATA"; the PDF opens in Edge and in
   a PDF reader.
4. IFU shop (switch a test copy): the same invoice shows TTC only and "Régime IFU – TVA non applicable".
5. Exempt customer (attestation number): HT prices, no TVA, the exemption mention.
6. Quote → proforma → invoice; an expired quote converts only at today's prices; a quote loaded at the
   till (`Ctrl+D`) and sold: the quote shows "Transformé".
7. Two BL → one invoice: stock moved once (stock card), the invoice cites both BL; a ticket → its invoice
   from the sales history; the ticket can no longer be returned ("Facturé").
8. Goods credit note (3 of 10 bags, restocked) then a full financial one on another invoice: that
   invoice prints "FACTURE ANNULÉE — avoir AV…" diagonally.
9. Arabic guard in "consumer" mode: the banner at the till in French, one tap switches the receipt to
   Arabic; never blocks.
10. Documents archive: 20 000 documents, search by number, name and amount answers in ≤ 300 ms; states
    right; PDF export of the list and of the M2 customers list (PDF of any table).
11. `/reports/fiscal` for last month: every G50 amount equals the VAT and stamp statements; copy puts
    `1710,00` on the clipboard; the nine registers open and export.
12. Fiscal archive of a year (100 000 tickets): export ≤ 3 min with a moving progress bar, RAM stays
    ≤ 350 MB during the export, "Vérifier une archive" says intact; edit one CSV in the zip with 7-Zip →
    verification lists that file.
13. No network: unplug the cable and disable Wi-Fi, repeat 3 and 11: identical.
14. Idle RAM with the documents list open and the print host closed ≤ 250 MB; the print host closes
    itself 10 minutes after the last print (Task Manager: one WebView2 process group fewer).
15. A printer switched off: the error says so in the chosen language; nothing is journaled; the next
    successful print is still the original.
16. Long documents on paper and in PDF (M3 final review): a 40-line invoice cancelled by a credit note,
    printed on the A4 printer and saved as a PDF, then reprinted: 12 mm margins on every sheet (8 mm on
    A5), "Page 1 / 2" and "Page 2 / 2" at the bottom, "FACTURE ANNULÉE — avoir AV…" across every sheet
    and DUPLICATA at the top of every sheet of the reprint; a 40-line ticket saved as a PDF from its
    A4/PDF copy fits one 80 mm page, its barcode number included.

Line 1: `main` also carries migration 0005 (the M4 hardware core: labels, scales, weight barcodes), so
the upgrade from 0.2.0 runs 0004 then 0005 behind the same pre-migration backup. Check that the backups
screen lists one pre-migration copy and that both migrations ran.

Line 4: never switch the regime of the shop's live data. Export a `.dzpos` file, restore it on the
second PC (or on this one after setting the data folder aside), switch Settings › Shop to IFU there, run
the line, then discard the copy.

Lines 6, 7 and 12 quote the French labels. In Arabic the states read "محوَّلة" (Transformé) and "مفوترة"
(Facturé), and the verifier button "التحقق من أرشيف" (Vérifier une archive).

Line 13: print the invoice again (a DUPLICATA now) and export its PDF; open the fiscal screen of the
same month. Compare the amounts with the online run.

## Record

Run by: (to fill) · Date: (to fill) · Reference PC (Windows version, WebView2 runtime version, A4
printer model and port, receipt printer, scanner): (to fill) · Installer SHA-256 checked: (to fill)

Write the measured value and OK or KO in each language cell, for example `3.8 s OK`.

| #   | Budget                                                                       | Arabic    | French    |
| --- | ---------------------------------------------------------------------------- | --------- | --------- |
| 1   | backup listed, 0004 (and 0005) applied, chains clean, M2 reprints DUPLICATA  | (to fill) | (to fill) |
| 2   | activity in 3 languages, RC, NIF, NIS, AI, capital; live preview             | (to fill) | (to fill) |
| 3   | RC blocks; duty line, Net à payer, balance; print ≤ 5 s (first ≤ 10 s); PDF  | (to fill) | (to fill) |
| 4   | IFU: TTC only and the IFU mention                                            | (to fill) | (to fill) |
| 5   | exempt: HT, no TVA, exemption mention                                        | (to fill) | (to fill) |
| 6   | DV → PF → FA, expired quote repriced, `Ctrl+D` sale → "Transformé"           | (to fill) | (to fill) |
| 7   | 2 BL → 1 FA, stock once; ticket → FA, "Facturé" not returnable               | (to fill) | (to fill) |
| 8   | 3 of 10 bags restocked; "FACTURE ANNULÉE — avoir AV…"                        | (to fill) | (to fill) |
| 9   | guard banner, one tap to Arabic, never blocks                                | (to fill) | (to fill) |
| 10  | search ≤ 300 ms on 20 000 documents, states, PDF of the lists                | (to fill) | (to fill) |
| 11  | G50 = VAT and stamp statements, `1710,00` copied, nine registers export      | (to fill) | (to fill) |
| 12  | archive ≤ 3 min, progress moves, RAM ≤ 350 MB, intact, edited file listed    | (to fill) | (to fill) |
| 13  | lines 3 and 11 identical offline                                             | (to fill) | (to fill) |
| 14  | idle RAM ≤ 250 MB, print host closed 10 min after the last print             | (to fill) | (to fill) |
| 15  | printer off: error in the language, nothing journaled, next print original   | (to fill) | (to fill) |
| 16  | 12 mm margins, page numbers, cancelled mark and DUPLICATA on every sheet; 80 mm PDF one page | (to fill) | (to fill) |

## Build record (2026-10-01, build PC)

- **Status: open.** None of the 15 lines has been run, so `m3` is not tagged. The build PC is not the
  reference PC: it has an Intel Core i7-1255U, 16 GB of RAM and an SSD, and no A4 printer, receipt
  printer or scanner is attached. Its timings and RAM readings would not answer the budgets. The
  automated cover below is interim evidence only. It does not replace the run on the reference PC.
- Build PC: Windows 11 Pro 10.0.26200 x64, WebView2 runtime 154.0.4258.37; rustc 1.98.1, Node 24.15.0,
  pnpm 10.32.1. The gates and the build ran on the tree of the commits that add this file. Only this
  file was edited after the build.
- Gates, all exit 0 except where noted: `cargo fmt --all -- --check`;
  `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` (dzbuild-pos 142
  passed and 1 ignored, pos-core 716 passed and 6 ignored, pos-docs 77 passed and 1 ignored, pos-hw 210,
  pos-server 1; see the load notes); `write_bindings` then `git diff --exit-code src/bindings.ts` (no
  change); `write_print_fixtures` then `git diff --exit-code src/documents/__fixtures__` (no change);
  `cargo test -p dzbuild-pos --test print_to_pdf -- --ignored` (1 passed: a real WebView2 `PrintToPdf`
  of a right-to-left page); `pnpm exec tsc --noEmit`; `pnpm lint`; `pnpm exec prettier --check .`;
  `pnpm exec vitest run` (63 files, 416 tests; see the load notes); `node scripts/i18n/check-i18n.mjs`.
- Load notes. Two other checkouts ran their own `cargo test --workspace` and builds on this PC at the same
  time, with the CPU at 87 to 100 %. Under that load:
  - `cargo test --workspace` failed two timing tests. `pos-core`
    `sales::robustness::sale_commit_p95_stays_within_budget` (p95 < 200 ms) measured a p95 of 459 ms
    and 488 ms. Alone it measured 204 ms, 230 ms and then 192 ms (pass), with a p50 of 10 to 20 ms. On
    an idle PC earlier milestones measured a p95 of 5 to 12 ms. `pos-hw`
    `driver::tests::a_tcp_printer_answers_a_status_query` timed out on its loopback socket and passed
    alone.
  - Full `pnpm exec vitest run` runs failed 3, then 23 and 23 tests, all on their 15 s or 20 s timeouts
    or on a `findBy` wait. The 15 files that failed passed when run together, except two
    `returns.test.tsx` timeouts, and `returns.test.tsx` then passed alone (8 of 8).
  - The first Vitest run also reported 2 unhandled rejections from `src/print-host/print-host.test.tsx`
    (`__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener is not a function`). That one is not load: it
    failed the run on every try. The fix commit before this file's commit makes the print host page
    unsubscribe during the effect cleanup, the same fix as the staging merge `1adc12a`.
- `pnpm tauri build`: exit 0 (release build 49 min 09 s, 53 min 45 s in all, under the load above). It
  wrote `target\release\bundle\nsis\DZBuild POS_0.3.0_x64-setup.exe`: unsigned, 8 267 247 bytes
  (7.88 MiB), SHA-256 `4e533c8028d8a8a2324040a3343393ba4d534cdf2a0550903f2fc66ab26b726c`.
  `dzbuild-pos.exe` reports product version 0.3.0. The print host page `print-host.html` is in `dist\`
  and embedded in `dzbuild-pos.exe` with the other frontend assets (Tauri bundles no separate resource
  file for it). Vite warns that the main JavaScript chunk is larger than 500 kB (1 425 kB, 399 kB gzip).

Automated cover of each line (in `cargo test --workspace` and `pnpm exec vitest run`). No test measures
a budget on the reference PC.

1. `pos-core` `db::m3_tests::the_m2_fixture_upgrades_in_place_after_a_backup`,
   `db::m3_tests::a_restore_fast_forwards_the_counters_and_every_chain_still_verifies`,
   `db::m4_tests::an_m1_database_climbs_every_migration_and_keeps_its_ticket`; app
   `commands::print_tests::a_ticket_prints_as_an_80_mm_duplicate_page`.
2. `pos-core` `billing::settings::tests::settings_are_validated_and_read_back`,
   `billing::mentions::tests::a_company_form_without_capital_is_a_warning`;
   `documents-settings.test.tsx`, `src/documents/overlay.test.ts`, `settings.test.tsx`.
3. `pos-core` `billing::invoices::tests::a_cash_part_pays_the_duty_and_the_credit_part_goes_to_the_account`,
   `billing::invoices::tests::an_invoice_cannot_be_validated_without_the_mandatory_mentions`,
   `billing::mentions::tests::an_invoice_needs_every_blocking_mention_of_both_parties`; `pos-docs`
   `a4::document::tests::a_bilingual_invoice_pairs_both_languages`; app
   `commands::print_tests::the_first_output_is_the_original_and_every_next_one_a_journaled_duplicata`,
   `src-tauri/tests/print_to_pdf.rs` (ignored, run above); `editor.test.tsx`, `templates.test.tsx`. No
   test prints on paper or times a print.
4. `pos-core` `billing::drafts::tests::an_ifu_shop_prices_ttc_and_never_exempts`; `pos-docs`
   `a4::document::tests::an_ifu_invoice_never_shows_tva`; `templates.test.tsx`.
5. `pos-core` `billing::drafts::tests::an_exempt_customer_pays_ht_without_tva`,
   `billing::pricing::tests::only_the_documents_of_an_exempt_buyer_lose_their_tva`; `pos-docs`
   `a4::document::tests::an_exempt_buyer_gets_the_exemption_and_no_tva`; `templates.test.tsx`.
6. `pos-core` `billing::convert::tests::a_quote_becomes_an_invoice_draft_at_the_quoted_prices`,
   `billing::convert::tests::an_expired_quote_converts_only_repriced`,
   `billing::convert::tests::a_quote_sold_at_the_till_is_linked_to_its_ticket`; app
   `commands::m3_tests::the_till_loads_a_quote_and_links_its_sale`; `conversions.test.tsx`.
7. `pos-core` `billing::convert::tests::invoicing_deliveries_never_moves_stock_again_and_cites_each_bl`,
   `billing::convert::tests::a_ticket_is_invoiced_once_and_never_counted_twice`,
   `billing::convert::tests::an_invoiced_ticket_is_corrected_by_a_credit_note_only`;
   `conversions.test.tsx`.
8. `pos-core`
   `billing::credit_notes::tests::a_goods_credit_note_restocks_and_credits_the_pro_rata_of_the_invoice`,
   `billing::credit_notes::tests::credit_notes_stop_at_what_the_invoice_holds_and_then_cancel_it`;
   `pos-docs` `a4::document::tests::a_fully_credited_invoice_prints_cancelled_and_a_reprint_duplicata`;
   `credit-note.test.tsx`.
9. `pos-core` `billing::guard::tests::consumer_documents_without_arabic_are_warned_per_setting`;
   `credit-note.test.tsx` ("suggests Arabic for a consumer document without blocking"). No test drives
   the banner inside the till's payment dialog.
10. `pos-core` `billing::archive::tests::the_archive_finds_by_number_name_amount_kind_and_pages`; app
    `commands::m3_tests::the_archive_exports_every_page_of_the_kinds_the_user_may_see`;
    `documents.test.tsx`. No test times a search on 20 000 documents.
11. App `commands::m3_smoke::a_month_of_invoices_balances_with_the_reports` (the G50 against the VAT
    and stamp statements, the registers); `pos-core`
    `fiscal::g50::tests::a_cash_paid_invoice_above_the_limit_is_not_deductible`; `fiscal.test.tsx`
    ("shows the G50 lines of a month and copies an amount the way the forms want it").
12. `pos-core` `fiscal::archive::tests::the_archive_holds_every_part_of_the_period_and_nothing_else`,
    `fiscal::archive::tests::an_export_is_anchored_in_the_journal`; `pos-docs`
    `fiscal_archive::tests::a_tampered_file_no_longer_matches_its_manifest_hash`; app
    `commands::print_tests::the_fiscal_archive_is_written_verified_and_anchored`; `fiscal.test.tsx`. No
    test times 100 000 tickets or reads the RAM.
13. None cuts the network. The templates need none: the fonts are bundled, the logo travels as a
    `data:` URL in the job and the QR is drawn as SVG (`templates.test.tsx`).
14. App `print_host::tests::a_render_timeout_frees_the_host_and_reports_print_render_timeout`,
    `print_host::tests::jobs_run_one_at_a_time`. Nothing tests the ten-minute idle close (`IDLE_CLOSE`
    in `src-tauri/src/print_host.rs`) or the RAM.
15. App `commands::print_tests::a_failed_print_journals_nothing`. The printer error codes are
    translated in `src/locales/{ar,fr,en}/errors.json` (i18n parity).
16. App `print_host::tests::every_job_carries_the_page_rule_of_its_setup`,
    `print_host::tests::page_setups_follow_the_paper_and_the_receipt_height`; `print-host.test.tsx` (the job's
    `@page` rule); `templates.test.tsx` ("repeats the cancelled mark and the DUPLICATA on every printed
    sheet"). jsdom does not paginate: only paper shows the sheets.

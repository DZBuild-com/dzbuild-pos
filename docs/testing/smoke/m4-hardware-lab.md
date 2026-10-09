# M4 hardware: lab checklist

Run it on real devices and fill in this file; the release (M6) runs it again before shipping. No automated test touches real
hardware: every driver has a memory or file double (`driver::MemoryDriver`, the `file` transport,
`serial::MemoryLinks`), and this checklist is the only place where the real devices are proved.

**Status: not run.** None of the 13 lines has been run yet.

## How to run it

- **Machine:** the reference PC: Celeron J1900, 4 GB RAM, HDD, 1024×768 touch screen, Windows 10 or 11
  x64, French AZERTY keyboard with the Arabic input language added (line 7), a second monitor
  (line 6), free USB ports and a USB hub for the USB-serial adapters.
- **Build:** the installer of the M4 commit, installed over a shop that already has products, a
  receipt printer and an open till. Note its version and SHA-256 in the record.
- **Settings:** every device is set up in Réglages › Matériel (one tab per device kind), then saved
  with "Enregistrer les réglages du matériel". The weight-barcode formats are in the "Balance" tab.
- **Catalogue to prepare:** a few weighed items (unit kg, "Article au poids", a PLU each: tomatoes,
  minced meat, olives), a few piece items with an EAN-13, one with a long Arabic name, one in a food
  category (the labels keep Arabic, DOC-034).
- **Recording:** for each line write the device's model, its firmware when the device shows it (self-test
  page, boot screen, display self-test), and the result: OK, or KO with what happened. Keep the photos
  of printed labels and displays with the record.
- **KO:** a KO line opens a fix task first, with an automated regression test over the memory or file
  doubles wherever one can reproduce it. The line is run again once the fix is merged.

## Checklist

1. Label printer Xprinter XP-365B (USB, Windows driver, RAW): the settings test label ("Imprimer une
   étiquette de test"); a 40×25 AR+FR price label (Arabic shaped, EAN-13 scanned back); if the label
   comes out in negative, switch "Inverser le noir et le blanc des images" and note it in
   `bitmap_black_is_zero`; gap calibration on 40×30 stock (Détection: "Espace entre étiquettes");
   2-up on 20×40 stock; darkness 6/8/12; 50 labels of a goods receipt in one job.
2. A ZPL printer (Zebra GK420d or Godex in ZPL emulation) if available: the same labels.
3. Label scale Rongta RLS1000 (or ACLAS LS6 / Dibal D900): its weight labels (prefix 21/27/29) and
   price labels (22/28) scanned at the till give the right product, weight and price; the PLU file
   exported from the POS imported through the scale's PC tool (note the column mapping used).
4. A serial checkout scale (Toledo 8217 mode, a CAS in ECR mode, or a generic indicator in continuous
   mode, USB-serial CH340/PL2303): "Détecter la balance", live weight, stable/in motion, tare, zero
   (Toledo), unplug/replug during a sale (the till keeps working, the dot turns red then green), F11
   adds the stable weight.
5. Pole display COMPOS PL200 (or any 2×20 VFD) in ESC/POS then CD5220 mode: the test pattern, a sale
   line, the total, the change, "Merci !", accents in PC858 mode, an Arabic ticket shown in French.
6. Second monitor (1024×768 or 1366×768): the customer screen full screen on DISPLAY2, not on the
   till's screen; unplugged → no window, the preview works; the sale in Arabic, French and English; the
   thanks for the configured delay; the logo. Unplug DISPLAY2 while the customer screen is open: within
   3 s it closes and the till takes the keyboard and the mouse as before; plug it back: it opens again
   on DISPLAY2.
7. HID scanners: a US-emulating scanner and an AZERTY-emulating one on a French AZERTY Windows with the
   Arabic input language active; the learn mode ("Apprendre mon lecteur") on the test page's
   `DZPOS-TEST-0123`; a scanner with an AIM prefix.
8. Serial scanner (a Henex/Honeywell-class scanner in USB-COM mode): codes reach the till and the count
   screen.
9. USB drawer adapter (virtual COM): ESC/POS bytes and DTR pulse.
10. Printer status and doctor: paper out on a TCP or COM receipt printer (DLE EOT), a Windows queue set
    offline, a driver replaced by "Generic / Text Only".
11. Data collector (POSLUX DT-92 class): its export file imported into an open count.
12. RAM with the customer screen open and the scale running: idle RAM ≤ 250 MB on the reference PC
    (spec §2).
13. A4 label sheets on a laser and an inkjet printer (the PC's A4 printer of Réglages › Documents):
    "Planche A4…" from "Imprimer des étiquettes", one planche of each format (3 × 7, 3 × 8, 4 × 10,
    5 × 13) on real Avery-type sheets; a partly used sheet started at position 23; an AR+FR sheet
    (position 1 is the top right label) and a French one (top left); every label inside its cut, the
    EAN-13s scanned back at the till, no blank page between two planches; the same sheet saved as a
    PDF prints the same. Note in millimetres how far a printer's labels drift from their cuts, if they
    do.

Line 1: `bitmap_black_is_zero` is the `bitmapBlackIsZero` field of the label printer's profile in
`%APPDATA%\dev.dzbuild.pos\hardware.json`; write down the value that printed right for this model so
the preset can carry it.

Line 3: the till decodes weight and price barcodes in Rust with the shop's formats (Réglages ›
Matériel › Balance › "Codes-barres poids et prix"; the pack's defaults otherwise). Use the "Scannez une
étiquette de balance" box there first: it shows the product, the weight and, for a price label, the
label's price beside the till's amount.

Line 12: read the RAM in the Task Manager's Details tab: add `dzbuild-pos.exe` and all its
`msedgewebview2.exe` children, after 10 minutes idle on the sale screen with the scale connected and
the customer screen showing the welcome.

## Record

Run by: (to fill) · Date: (to fill) · Reference PC (Windows version, keyboard layouts): (to fill) ·
Build version and installer SHA-256: (to fill)

| #   | Device (model, port)                | Firmware  | Result    | Notes     |
| --- | ----------------------------------- | --------- | --------- | --------- |
| 1   | Xprinter XP-365B, USB               | (to fill) | (to fill) | (to fill) |
| 2   | ZPL printer                         | (to fill) | (to fill) | (to fill) |
| 3   | Label scale                         | (to fill) | (to fill) | (to fill) |
| 4   | Checkout scale, USB-serial          | (to fill) | (to fill) | (to fill) |
| 5   | Pole display, ESC/POS then CD5220   | (to fill) | (to fill) | (to fill) |
| 6   | Second monitor                      | (to fill) | (to fill) | (to fill) |
| 7   | HID scanners (US, AZERTY, AIM)      | (to fill) | (to fill) | (to fill) |
| 8   | Serial scanner, USB-COM             | (to fill) | (to fill) | (to fill) |
| 9   | USB drawer adapter                  | (to fill) | (to fill) | (to fill) |
| 10  | Receipt printers for status, doctor | (to fill) | (to fill) | (to fill) |
| 11  | Data collector                      | (to fill) | (to fill) | (to fill) |
| 12  | RAM (MB, idle)                      | n/a       | (to fill) | (to fill) |
| 13  | A4 label sheets, laser and inkjet   | n/a       | (to fill) | (to fill) |

## Automated cover over simulated devices

What `cargo test --workspace` and `pnpm exec vitest run` already prove with the memory and file
doubles. It is interim evidence only and does not replace a line of the run above.

- **End to end:** app `commands::m4_smoke::a_weighed_morning_balances`: a continuous scale on an
  in-memory COM3 weighs 1.250 kg, the till reads the stable weight, a weighed label goes to a file
  label printer and its EAN-13 is read back by `products_find_by_code`, one sale of two weighed lines
  (300,00 DA), the pole display on an in-memory COM2 shows the change, the PLU file lists the item,
  the stock falls from 5 kg to 2.5 kg, the audit journal and the document chains verify, and the label
  is journaled once. `src/app/m4-smoke.test.tsx`: the hardware hub and the labels page, every tab, in
  Arabic and French without a missing key; the customer screen right to left with the three thanks.

1. `pos-hw` `labels::tspl::tests::tspl_40x25_fr_ean13`, `tspl_bitmap_polarity_follows_the_profile`,
   `two_up_rows_repeat_the_label_and_round_the_copies_up`, `labels::layout` tests (no overlap on any
   preset size, golden previews), `registry` `labels_print_to_the_label_printer_in_its_language`,
   `the_label_test_page_prints_on_an_unsaved_printer`; app `commands::labels_tests`.
2. `pos-hw` `labels::zpl::tests::zpl_58x40_ar_fr_promo`,
   `code128_and_qr_use_their_zpl_commands_and_carets_are_refused`.
3. `pos-core` `catalog::scale_codes` tests (default layouts, weight codes round trip, price labels,
   verifier layouts), `catalog::scale_items` tests (the PLU table); app
   `commands::labels_tests::scale_items_and_the_plu_file`; `scale-codes.test.tsx`.
4. `pos-hw` `scale::protocol` and `scale::service` tests (Toledo, CAS, continuous, stability, tare,
   zero, unplug and reconnect, probe); app `commands::devices_tests`; `till-hardware.test.tsx` (F11,
   live weight), `hardware-hub.test.tsx` (detect the scale).
5. `pos-hw` `display::vfd` and `display::service` tests (columns, accents, paid screen, Arabic frames
   in the Latin language, test pattern, unplugged display); app `commands::devices_tests`.
6. App `customer_display` tests (no second monitor opens no window, the named monitor wins);
   `customer-display.test.tsx`, `m4-smoke.test.tsx`.
7. `scanner.test.tsx` (AZERTY and Arabic layouts by physical keys, AIM prefix),
   `hardware-hub.test.tsx` (learn mode on `DZPOS-TEST-0123`).
8. `pos-hw` `scanner` tests (suffixes, prefixes, unplug and reconnect); `till-hardware.test.tsx`
   (a serial scanner's code goes through the same lookup).
9. `pos-hw` `registry` `a_usb_drawer_adapter_opens_by_bytes_or_dtr`,
   `a_cash_sale_opens_the_drawer_adapter_and_not_the_receipt_printer`.
10. `pos-hw` `status` tests (DLE EOT bytes, spooler bits, the doctor), `registry`
    `the_doctor_reports_a_printer_windows_no_longer_lists`, `a_network_printer_tells_its_own_status`.
11. `pos-hw` `collector` tests; `pos-core` `stock::counts` collector tests; app
    `commands::labels_tests::a_collector_file_fills_the_open_count_and_reports_bad_lines`.
12. None. It needs the installed app on the reference PC.
13. App `commands::label_sheets_tests` (the plan of a partly used sheet, EAN-13 modules, the Arabic
    guard, printed and PDF sheets journaled only after the print host succeeded); `label-sheet.test.tsx`
    (cells in millimetres from the start edge, bars, the `label_sheet` page), `labels.test.tsx` (the
    start position and the print through the print host).

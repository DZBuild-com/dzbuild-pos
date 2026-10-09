# M1 real-till smoke

The sale screen, the printers, the drawer and the scanner on a real till. Automated tests cannot reach
the WebView2 accelerator keys, a real printer and drawer or a real scanner, so these 12 lines are run by
hand before each release that changes the sale flow or printing.

## How to run it

Install the setup on a Windows 10 or 11 PC with an 80 mm ESC/POS printer (USB, Windows driver "Generic /
Text Only" or the vendor's), a drawer on its RJ11 port and a USB barcode scanner; then a 58 mm printer.
Record each line as OK / KO, with a photo of the receipts for the lines that print (1, 4, 5, 6, 7, 9, 10).
A KO line opens an issue, with an automated regression test where one is possible, before the release.

| # | Check | Expected |
|---|---|---|
| 1 | Fresh install, wizard in Arabic, then the printer step: "Détecter" / "اكتشاف الطابعات" | The printer shows as "likely a receipt printer"; the test page prints Arabic shaped right to left, Latin, digits, a Code 128 and cuts |
| 2 | In the sale screen press F1, F3, F5, F7, F12, Ctrl+P, Ctrl+F | F-keys do the POS action (help, nothing for F3, discount, held tickets, payment); no reload, no devtools, no browser print or find dialog |
| 3 | Windows keyboard set to French (AZERTY), then to Arabic; scan 10 products | Every barcode is read exactly (no `&é"'`, no Arabic letters); a scan while the quantity field has focus never changes the quantity |
| 4 | French sale of 450,00 DA paid 500 DA cash, receipt on | Text-mode receipt with accents and "DA", droit de timbre 5,00 (net 455,00), change 45,00; the drawer opens after the cut |
| 5 | Arabic sale; then a French sale containing a product named only in Arabic | Both print as raster, right-aligned Arabic, Western digits, nothing cut on 58 mm or 80 mm |
| 6 | Bilingual receipt (Réglages › Matériel › second language Arabic) | Labels in both languages, product names in the ticket's language |
| 7 | Unplug the printer, sell | The sale completes and the change shows at once; a toast "not printed" offers a retry; plug in, retry → the original prints once; "Réimprimer" later prints DUPLICATA |
| 8 | F9 no-sale drawer as a cashier, reason "Change request" | The drawer opens; Réglages › Journal › Tiroir shows the entry with the cashier and the reason |
| 9 | Return with the ticket barcode (F8, scan the receipt), then an exchange | Exact refund shown before confirming, manager PIN asked, BR number printed; the exchange charges only the difference |
| 10 | Close the till with a count 1 000 DA short | Forced recount, then manager PIN; the Z prints (Z number, per-rate taxes, methods, variance) and a backup appears in Réglages › Sauvegardes |
| 11 | 1024×768 and 1366×768, light and dark themes, Arabic UI | No horizontal scroll; the cart, totals and "Encaisser" stay visible; RTL mirrors the layout, amounts and barcodes stay LTR |
| 12 | Kill `dzbuild-pos.exe` in the Task Manager right after "Valider", restart | The ticket is either fully there (numbered, in the history, stock moved) or absent; the numbering has no gap; the journal verifies |

## Record

- Run by, date, till (Windows version), 80 mm printer (model, driver, port), 58 mm printer, drawer,
  scanner (model, layout): (to fill).

| # | Result (OK / KO, width for printed lines) | Photo |
|---|---|---|
| 1 | | |
| 2 | | not needed |
| 3 | | not needed |
| 4 | | |
| 5 | | |
| 6 | | |
| 7 | | |
| 8 | | not needed |
| 9 | | |
| 10 | | |
| 11 | | not needed |
| 12 | | not needed |

# Changelog

## 1.1.0 — 2026-10-10

No database change.

### Features
- **zakat**: the shop enters the nisab of the current Hijri year itself, with a note on its source; the statement keeps the figure it was computed with. Nothing is computed until the nisab is entered.
- **payments**: Settings › Payments chooses how the till takes payment: one fixed method that finishes the sale on Pay, or a list of allowed methods with the first one pre-filled.

### Fixes
- **till**: product search shows only results for the text in the field, Enter adds the first result, Esc closes the list without opening the menu; decimals follow the product's unit on every path.
- **till**: after a line is removed the remaining lines keep their own names and totals; the edit window closes only once the removal went through; the selection stays on the same line.
- **till**: a recalled ticket comes back with a line selected; serials follow a lower quantity.

## 1.0.0 — 2026-10-09

First public release: the free long-term-support (LTS) edition for Windows 10 and 11, 64-bit and 32-bit.

- **till**: fast sale screen with barcode scanner, scale, cash drawer, customer display, held tickets, returns and exchanges, mixed payments, stamp duty on cash.
- **back office**: products, variants, lots, stock ledger, inventories, purchases and suppliers, customers and credit (kridi), promotions and loyalty, cash sessions and Z reports.
- **documents**: receipts, invoices, delivery notes, quotes and credit notes with gap-free numbering, printable in Arabic, French or English, A4 or thermal.
- **trades**: 25 activity templates (grocery, restaurant, pharmacy, butcher, phones, jewellery, wholesale, services and more) chosen in the setup wizard.
- **several tills**: one main PC and tills on the shop network, each keeps selling when the network is down.
- **data**: local backups, USB copies and guided restore; signed regulatory data (VAT rates, stamp duty scale, regulated prices, wilayas).
- **release**: online and offline setups for 64-bit and 32-bit Windows, with one `SHA256SUMS.txt`.

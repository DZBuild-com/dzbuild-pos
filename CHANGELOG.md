# Changelog

## 1.0.0 — 2026-10-09

First long-term-support (LTS) version of the free edition. No database change.

### Features
- **release**: 32-bit Windows setups (online and offline) next to the 64-bit ones. One `SHA256SUMS.txt` covers both, and 32-bit installs receive their own updates.
- **shell**: every page opens at the top and has a back-to-top button.
- **about**: the About page shows the free edition and the optional link to a DZBuild online store.

### Changed
- **urls**: all links shown in the app point to dzbuild.com/pos (download, support, updates).
- **i18n**: simpler wording on every screen and help page, in Arabic, French and English.
- **release**: the licence issuing tool is no longer part of a free-edition build.

### Fixes
- **i18n**: the direction of debt amounts on the Arabic dashboard tiles.

### Removed
- **backup**: the DZBuild online backup service. Local backups, the USB copy and restore are unchanged. The old tables stay unused in existing databases.

## 0.9.0 — 2026-10-08

The free edition. No database change.

### Changed
- **licence**: the app no longer needs a licence. Every PC can sell right after installation. The activation screen, the read-only mode and Settings › Licence are gone. The licence module stays in the source and is turned off by `free = true` in `src-tauri/license.toml`.
- **release**: a free-edition build no longer requires a sales contact in `license.toml`.

## 0.8.0 — 2026-10-04

Trade templates and per-PC licensing. The database moves to schema 9 after an automatic backup.

### Features
- **packs**: 25 activity templates chosen in the setup wizard, each with its modules, till mode and starting data in three languages. Modules can be turned on or off in Settings › Activity and modules; a module turned off hides its screens and keeps its data. New roles: technician, accountant, waiter, employee.
- **restaurant**: floor plans with live table status, table tickets with modifiers, transfer and merge, kitchen tickets per station, pre-bill, split bills, voids and comps with a reason, takeaway numbers, daily menu.
- **pharmacy**: search by INN and generics, expiry board, import of the Chifa reference list, controlled medicines register.
- **butcher**: carcass cutting yield with cost carried to the cuts, cutting templates, waste log, yield and waste reports.
- **phones**: serials and IMEI on sales and returns, flexy top-ups with float and commissions per operator, repair tickets from intake to pickup.
- **instalments**: instalment sales on credit with a plain-language contract, schedule, collections, reminders and an overdue list.
- **wholesale**: customer prices, minimum quantities, packs, delivery note used as invoice, delivery rounds and van sales.
- **orders**: customer orders and layaway with deposits and reserved stock, appointment book.
- **loyalty**: loyalty cards and points with expiry, rewards as vouchers, gift vouchers sold and accepted as payment.
- **jewellery**: gold price board, pieces priced by weight, fineness and making charge, hallmark on lines and labels, jeweller register.
- **staff**: commission rate, salary advances, shifts from cash sessions, zakat statement.
- **till**: price ceiling on regulated products, department keys, variant picker, custom payment types, BaridPay QR on the customer screen, document language per customer, remappable keys.
- **app**: signed regulatory data packs, business-day cut-off for shops open after midnight, personal-data tools (consents, access log, export, anonymisation), hourly backups while a till is open, Code 39 and ITF labels.
- **lan**: a till can sell controlled medicines, jewellery, IMEI phones, top-ups and vouchers while the main PC keeps the registers.
- **licence**: per-PC licences (7-day trial or lifetime) checked offline, with a read-only mode that keeps reports, exports, backups and the fiscal archive available. Replaced by the free edition in 0.9.0.

### Fixes
- **packs**: a module turned on by hand now gets its starting data.
- **lan**: a booking refused by the main PC no longer blocks the till's later entries.
- **core**: hourly backups follow the shop's retention setting.

### Performance
- **ui**: trade screens load on demand, so the till screen code stays under 300 kB.

## 0.7.0 — 2026-10-02

First release: the till, the back office, documents, hardware, several tills and the DZBuild link.

### Features
- **documents**: quotes, proformas, delivery notes and invoices, conversions between them, credit notes as the only correction, the décret 05-468 mentions.
- **documents**: A4 and A5 printing and PDF offline in Arabic, French, English or bilingual, duplicates, statements, warranty certificates, document archive.
- **fiscal**: VAT, stamp duty, G50 helper, IFU turnover, G9 and the sales and purchase registers; fiscal archive with a SHA-256 manifest and a verifier.
- **hardware**: TSPL and ZPL label printers with a designer, label templates, reprint queue and A4 label sheets.
- **hardware**: weight and price barcodes, serial scales with tare and zero, PLU file.
- **hardware**: pole display, customer screen, serial scanners, cash drawers on USB or COM, printer status and a printer troubleshooter.
- **lan**: a main PC and tills on the shop network, paired with a 6-digit code. Each till sells offline with its own numbering and syncs when the network returns.
- **lan**: Settings › Network to manage tills, their Z reports and the firewall check; offline Z with manager approval.
- **dzbuild**: optional link to a DZBuild online store: sync of products, stock and sales, online orders, order QR on receipts.
- **ui**: on-screen AZERTY and Arabic keyboard for touch screens.
- **app**: demo shop, import from another PC at first start, guided tour.
- **ui**: offline help in three languages (F1 on any screen).
- **app**: diagnostic screen and ZIP, last crash shown in About.
- **app**: signed updates with stable and beta channels, installed only when every cash drawer is closed.
- **ui**: backups screen with schedule, retention, second copy, USB copies, restore preview and integrity check; guided restore after a failed start.
- **release**: online and offline setups in three languages with the firewall rules for several tills.

### Fixes
- **app**: if an update does not install, the app restarts and says so at the next start.
- **app**: a till sets its local copy aside only when that copy is damaged or incomplete.
- **app**: closing a main PC while tills are connected asks first, then makes a backup.

### Performance
- **ui**: back office screens load on demand; light mode and text size settings.

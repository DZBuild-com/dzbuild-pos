# 01 — DZBuild POS: Master Feature Specification

> **Product:** DZBuild POS (working names "DZBuild Caisse", "كاشير DZBuild"). A free, offline-first, installable Windows desktop POS plus gestion commerciale / gestion de stock for Algerian shops. Linking a DZBuild account (dzbuild.com) through OAuth is optional.
> **Date:** 2026-09-28 (updated the same day with report 01 and the trilingual decision). **Status:** research synthesis, ready for architecture and backlog grooming.
> **Built from:** `research/01` to `research/09`, read in full. Report 01 (FR/AR/EN web and Ouedkniss vendor sweep) was recovered on 2026-09-28, after the first version of this spec. Its feature catalogue (01 §4.2, §9) and vendor feature lists are now merged in: rows first sourced "01 via 09" cite 01 directly, and the features that were missing were added as new rows.
> **Language policy (owner decision, 2026-09-28):** natively trilingual. Arabic (RTL), French and English are equal first-class languages everywhere (§0.6).
> **Companion:** [`00-RESEARCH-REPORT.md`](00-RESEARCH-REPORT.md) covers the market map, legal summary, hardware matrix, stack and risks.

---

## 0. How to read this spec

### 0.1 What "complete" means here

This catalogue is the union of every feature found in any research file, deduplicated:

- **01**: the master feature catalogue (§9.1–§9.11), the richest vendor feature sets (§4.2), the editor, restaurant, vertical and SaaS feature lists (§3, §4.3–§4.5), the regulatory points (§11) and the opportunities (§12). 01 has no feature IDs; its items are traced by section references such as `01§9.4` (§41.5).
- **03**: all 547 rows of the global benchmark (CHK, PAY, RET, DSC, TAX, DOC, PRD, PRC, INV, PUR, CRM, CRD, LOY, GFT, LAY, EMP, CSH, RPT, RST, OMN, OFF, MST, HW, IMP, BCK, AUD, SEC, API, L10N, UX, ONB, ADM, AI). That includes the 22 rows marked not applicable in Algeria; they are listed in §40 so nothing is silently dropped.
- **09**: all 50 additions (`DZ-CHK-*`, `DZ-PRM-*`, `DZ-STK-*`, `DZ-DOC-*`, `DZ-FIN-*`, `DZ-CMP-*`, `DZ-INT-*`, `UX-DZ-*`).
- **02**: the vertical needs (§3.1–3.17), the local habits (§4) and the consolidated checklist (§7).
- **04**: the legal MUST/SHOULD list (§15) and every product implication in §2–§14.
- **05**: the hardware checklist (§13) and the integration details.
- **06**: the DZBuild integration features and the proposed APIs.
- **07**: architecture rules and performance budgets, as non-functional requirements.

When several sources describe the same feature, it appears **once**. The *Src* column lists every source ID, so you can trace a row back. A script check (end of file) confirms that all 597 source IDs from 03 and 09 appear in this file.

### 0.2 Release tags

| Tag | Meaning |
|---|---|
| **[MVP]** | First public release. Includes every legal must-have and every table-stakes feature for the core verticals. |
| **[V1]** | First major update after MVP. |
| **[V2+]** | Later roadmap. Includes "opt" (only if demand appears) and "watch" (depends on a regulation or partner that doesn't exist yet). |
| **[N/A]** | Not applicable to an offline Windows POS in Algeria. Listed only in §40. |

Release tags follow the research recommendation (03's M/V1/V2, 09's M/V1/V2). Where two sources disagree, the **earlier** release wins if a legal text requires the feature (for example a warranty certificate is MVP for phone shops because of décret 13-327, although 03 put warranties in V1). The row notes when a feature is MVP only for some verticals.

### 0.3 Vertical tags

`ALL` means every shop type. Otherwise the row names the verticals that need it.

| Code | Vertical | Code | Vertical |
|---|---|---|---|
| SUP | Supérette, épicerie, alimentation générale | OPT | Optique |
| FRS | Boucherie, charcuterie, poissonnerie, fruits & légumes (weighed goods) | INS | Vente à tempérament (électroménager, meubles, phones) |
| BAK | Boulangerie, pâtisserie, pastry supplies | PRS | Pressing and "service ticket" trades (repair, tailoring, cordonnerie, car wash) |
| RST | Restaurant, café, fast-food, pizzeria, salon de thé | SAL | Salon de coiffure, esthétique, barbershop |
| PHA | Pharmacie (officine) and parapharmacie | JEW | Bijouterie (gold, silver, precious metals) |
| QNC | Quincaillerie, matériaux de construction, peinture | GYM | Salle de sport and subscriptions (school, hammam, parking) |
| AUT | Pièces détachées auto | KSK | Kiosque, bureau de tabac, multiservices, flexy |
| CLO | Vêtements, chaussures, sacs | CNS | Returnable containers: butane gas, beverage crates |
| TEL | Téléphonie, électronique, informatique, repair, flexy | IMP | Importateur (import for resale) |
| COS | Cosmétiques | VAN | Vente ambulante, van sales, DSD |
| LIB | Librairie, papeterie | MFG | Production workshop (atelier) |
| WHS | Grossiste, demi-gros, distribution | | |

### 0.4 Source codes

| Code | Meaning |
|---|---|
| `03:CHK-001` … `03:AI-010` | Feature ID in `research/03-global-pos-features.md`. The `03:` prefix keeps these apart from this spec's own IDs, which reuse some of the same module prefixes (e.g. `PAY-008` here is not `03:PAY-008`). |
| `DZ-CHK-01`, `UX-DZ-01`, … | Feature ID in `research/09-gaps-and-additions.md` |
| `02§3.3`, `04§4`, `05§4.2`, `06§6.2`, `07§5.2`, `09§5.1` | Section of the research file |
| `01§9.1`, `01§4.2 (KERS)` | Section of `research/01-algeria-pos-vendors.md`, with the vendor in brackets where useful |
| `owner decision 2026-09-28` | Product-owner decision: natively trilingual AR/FR/EN (§0.6) |

### 0.5 Module map

| § | Module | Prefix |
|---|---|---|
| 1 | Platform, installation, updates, administration | PLT |
| 2 | Offline operation, data safety, backup and restore | DAT |
| 3 | Multi-till LAN and multi-store | LAN |
| 4 | Users, roles, permissions, security | USR |
| 5 | Audit trail, fiscal journal, loss prevention | AUD |
| 6 | Personal-data compliance (loi 18-07 / 25-11) | PDP |
| 7 | Localisation, accessibility, UX | UX |
| 8 | Onboarding, help, training, support | ONB |
| 9 | Company profile and fiscal settings | CFG |
| 10 | Sale screen (checkout) | SAL |
| 11 | Payments and tenders | PAY |
| 12 | Returns, exchanges, voids | RET |
| 13 | Pricing and price lists | PRC |
| 14 | Discounts, promotions, legal soldes | PRM |
| 15 | Taxes and droit de timbre | TAX |
| 16 | Commercial documents and templates | DOC |
| 17 | Product catalogue | PRD |
| 18 | Labels and barcodes | LBL |
| 19 | Inventory and warehouses | STK |
| 20 | Purchasing and suppliers | PUR |
| 21 | Customers and CRM | CRM |
| 22 | Customer credit (kridi) and receivables | CRD |
| 23 | Instalment sales (vente à tempérament) | TMP |
| 24 | Loyalty, gift cards, store credit | LOY |
| 25 | Orders, layaway, repairs, service tickets, appointments | ORD |
| 26 | Employees and HR lite | EMP |
| 27 | Cash management and register closing | CSH |
| 28 | Treasury, cheques, expenses, accounting export, zakat | FIN |
| 29 | Restaurant and hospitality | RST |
| 30 | Vertical packs | VRT |
| 31 | Omnichannel, delivery, e-commerce (COD) | OMN |
| 32 | DZBuild integration | DZB |
| 33 | Hardware | HW |
| 34 | Import / export engine | IMX |
| 35 | APIs, messaging, third-party integrations | API |
| 36 | AI and emerging features | AI |
| 37 | Reports and états catalogue | RPT |
| 38 | Import/export matrix (entity × format) | — |
| 39 | Non-functional requirements | NFR |
| 40 | Not applicable and prohibited features | EXC |
| 41 | Counts and coverage check | — |

### 0.6 Language policy: natively trilingual (owner decision, 2026-09-28)

DZBuild POS is **natively trilingual**: Arabic (RTL), French and English are equal, first-class languages. It is not an Arabic-first product and not an "Arabic + French" product. The rows below are **[MVP]**, except the video tutorials (ONB-010) and the quick card (UX-035), which are V1 features that must ship in all three languages when they ship.

| Area | Requirement | Rows |
|---|---|---|
| UI | All three languages complete; switch at runtime without restart; language saved per user; mirrored RTL/LTR layouts | UX-001, UX-003, UX-041 |
| Documents | Receipts, invoices, quotes, proformas, BL, credit notes, statements and labels printable in any of the three languages, chosen per document, per document type or per customer; optional bilingual AR + FR layout (AR + EN and FR + EN as options) | UX-004, UX-042, DOC-013, LBL-005 |
| Amount in letters | Arabic, French and English, with correct DZD grammar | DOC-004 |
| Master data | Product, category, unit and tax names in AR/FR/EN; printed in the document language with a fallback | PRD-008, TAX-001, ONB-008 |
| Reports and exports | Reports, dashboards and XLSX/CSV/PDF exports in the three languages; imports accept headers in any of them | UX-043, IMX-005, IMX-006 |
| Texts and messages | Receipt texts, legal mentions, WhatsApp/SMS/email templates per language, picked by the customer's language | UX-044 |
| Search | Arabic and Latin normalisation; one query matches the AR, FR and EN names | UX-032 |
| Numerals and formats | Western digits 0–9 by default; Arabic-Indic digits optional; DZD and date formats per language | UX-005, UX-014 |
| Help and onboarding | Setup wizard, help centre, user manual, quick card and video subtitles in the three languages | ONB-001, ONB-004, ONB-010, UX-035 |
| Quality gate | Every translation complete at release; CI fails on any missing or empty key; one glossary; pseudo-localisation; golden documents and reports in each language | NFR-020, NFR-023, NFR-027 |
| Legal guard | Consumer information on food and regulated goods must stay available in Arabic (décret 13-378): a warning when it is printed in French or English only | DOC-034 |

Tamazight (UX-002, V2+) and Darija wording (UX-037, V1) come on top of the three core languages.

---
## 1. Platform, installation, updates, administration (PLT)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PLT-001 | Native installable Windows desktop app (not web). Targets Windows 10 1809+ and Windows 11 x64; optional i686 build for old 32-bit Windows 10 PCs. Windows 7/8.1 are out (Tauri 2.12 and Rust ≥ 1.78 dropped them). | [MVP] | ALL | 07§2.1, 09§2.2, 09§10.1, 05§7, 01§12 (8) |
| PLT-002 | Free forever: no licence server, no activation, no phone-home, no kill switch, no feature paywalls. Linking DZBuild adds services but never unlocks core features. | [MVP] | ALL | 03:SEC-010, 03:ADM-009, 06§5.1, 06§8, 01§9.8, 01§6 |
| PLT-003 | No artificial limits: unlimited products, users, registers, stores, documents and sales history (competitors cap products or documents per month). | [MVP] | ALL | 03:PRD-001, 03:EMP-015, 03:MST-009, 03:RPT-029, 02§2.1, 09§2.2 |
| PLT-004 | Online setup `-setup.exe` (NSIS, ≤ 15 MB) with the embedded WebView2 bootstrapper. | [MVP] | ALL | 07§13.1 |
| PLT-005 | Offline full installer (~127 MB, WebView2 offline runtime) for shops without internet and USB distribution by resellers. | [MVP] | ALL | 07§13.1, UX-DZ-11, 01§6 |
| PLT-006 | Portable (USB) mode for trials and emergency use. | [V1] | ALL | UX-DZ-11 |
| PLT-007 | Installer: perMachine by default, perUser fallback, silent `/S` install, FR/AR/EN installer languages. | [MVP] | ALL | 07§13.1 |
| PLT-008 | Installer adds a Windows Firewall inbound rule (Private profile) for the LAN host role. | [MVP] | ALL | 07§6.2 |
| PLT-009 | Code-signed installer, binaries and updates (OV certificate on a cloud HSM, RFC 3161 timestamps, yearly renewal with the same subject). Microsoft Store as a second channel (Algeria is a Store market). | [MVP] | ALL | 03:SEC-009, 07§13.2, 09§10.4, 01§7, 01§12 (2) |
| PLT-010 | Auto-update with mandatory signature verification, stable and beta channels, staged rollout (10 → 50 → 100 %), kill switch in the manifest, update check at startup and every 6 h when online. | [MVP] | ALL | 03:ADM-001, 07§13.3, 01§9.8 |
| PLT-011 | Updates never install during an open shift; offered at clôture de caisse or next launch. | [MVP] | ALL | 07§13.3 |
| PLT-012 | LAN host downloads each update once and serves it to the tills (signatures still verified). | [V1] | ALL | 07§13.3 |
| PLT-013 | Offline update package installable from USB. | [V1] | ALL | 03:ADM-002 |
| PLT-014 | Roll back to the previous version. | [V1] | ALL | 03:ADM-003 |
| PLT-015 | Export/import all settings to clone a configuration onto a new PC. | [V1] | ALL | 03:ADM-004 |
| PLT-016 | Module toggles: hide unused modules (restaurant, serials, lots, instalments, verticals, even the sale screen for a storage-only depot) to keep small shops' UI simple. | [MVP] | ALL | 03:ADM-007, 02§8, 01§8 |
| PLT-017 | Opt-in telemetry and crash reporting: out-of-process minidumps stored locally, uploaded only after explicit consent. | [MVP] | ALL | 03:ADM-008, 07§14 |
| PLT-018 | Rolling log files kept 14 days, with personal data (phone numbers) redacted. | [MVP] | ALL | 07§14 |
| PLT-019 | "Exporter un diagnostic" ZIP (logs, app/OS/WebView2 versions, schema version, integrity result, printers, settings; no DB content unless ticked), shareable by WhatsApp or email. | [MVP] | ALL | 07§14 |
| PLT-020 | Single instance, system tray, autostart on the host PC; closing the window minimises to tray on the host. | [MVP] | ALL | 07§10, 07§17 |
| PLT-021 | Host-role watchdog: the embedded server restarts with backoff after a panic. | [MVP] | ALL | 07§14 |
| PLT-022 | Optional Windows service "DZBuild POS Server" for back-office PCs where nobody logs in. | [V2+] | ALL | 07§17 |
| PLT-023 | WebView2 presence/health check at startup with a repair link; enforced minimum WebView2 version. | [MVP] | ALL | 07§18 (R2) |
| PLT-024 | Regulatory data packs, signed and updatable without reinstalling: tax rates and validity dates, timbre brackets, regulated-price ceilings, zakat nisab, 58/69 wilayas and communes, holidays, fiscal deadlines ("LF update pack"). | [MVP] | ALL | DZ-CMP-02, 04§3.1, 04§15, 09§11 |
| PLT-025 | Plugin/module system for vertical packs and third-party extensions. | [V2+] | ALL | 03:API-007 |
| PLT-026 | In-app release notes in AR/FR/EN after each update, a public changelog, and the per-version 2027 conformity statement published with each release. | [MVP] | ALL | 01§12 (2), 01§1 (13) |

## 2. Offline operation, data safety, backup and restore (DAT)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| DAT-001 | Sell with no internet at all. | [MVP] | ALL | 03:OFF-001 |
| DAT-002 | *Everything* works offline: stock levels, negative-stock alerts, reports, loyalty, gift cards, closing, printing (fixes the offline gaps of Loyverse, Square and Toast). | [MVP] | ALL | 03:OFF-002, 03:LOY-006, 03§4 (G2) |
| DAT-003 | Nothing at the till blocks on the network: no licence check, OAuth refresh is non-blocking, long offline grace, internet shutdowns (bac exams) have no effect. | [MVP] | ALL | 05§8.1, 06§5.4 |
| DAT-004 | Crash-safe embedded DB: SQLite WAL with `synchronous=FULL` on money paths, one atomic transaction per sale (receipt, journal, stock, cash), single writer thread plus reader pool. Survives power cuts. | [MVP] | ALL | 03:OFF-009, 05§8.2, 07§5.2 |
| DAT-005 | `quick_check` at startup, full `integrity_check` weekly and before backups; automatic restore from the last good snapshot if corruption is found. | [MVP] | ALL | 05§8.2, 07§5.2 |
| DAT-006 | Manual backup to a file. | [MVP] | ALL | 03:BCK-001 |
| DAT-007 | Automatic scheduled backups with rotation: at every Z close, hourly (keep 24), daily (keep 30), monthly (keep 12). | [MVP] | ALL | 03:BCK-002, 07§5.5, 01§9.8 |
| DAT-008 | Backup to USB stick, external drive or network folder; removable-drive detection and a one-click "Sauvegarder sur clé USB". | [MVP] | ALL | 03:BCK-003, 07§5.5, 02§4.8, 01§9.8 |
| DAT-009 | Backups made only with the SQLite backup API or `VACUUM INTO`; the live DB is never raw-copied, never run from a USB stick, never shared over SMB. | [MVP] | ALL | 05§8.2, 05§9 |
| DAT-010 | Every backup is re-opened read-only and passes `integrity_check` before it counts as good. | [MVP] | ALL | 07§5.5 |
| DAT-011 | One-click restore wizard listing backups with date, size and sales count, with preview and integrity check. | [MVP] | ALL | 03:BCK-005, 07§5.5, 03§4 (G5), 01§9.8 |
| DAT-012 | Automatic backup before every update or migration. | [MVP] | ALL | 03:BCK-008, 07§5.1 |
| DAT-013 | "Move to a new PC" wizard. | [MVP] | ALL | 03:BCK-010 |
| DAT-014 | Upgrade a single-PC database to LAN multi-user (host) mode. | [MVP] | ALL | 03:BCK-006, 01§9.8 |
| DAT-015 | LAN host pulls backups from every till. | [V1] | ALL | 07§5.5 |
| DAT-016 | Encrypted cloud backup to the DZBuild account: client-side encryption (age/AES-GCM, key from passphrase with Argon2id), DZBuild stores ciphertext only. **Removed in 1.0.0 (free edition): no cloud storage; backups stay local, on the second copy and on USB sticks.** | [V1] | ALL | 03:BCK-004, 06§6.2, 06§7, 07§5.5 |
| DAT-017 | Integrity-check and repair tools for support. | [V1] | ALL | 03:BCK-009, 01§9.8 |
| DAT-018 | Encryption at rest (SQLCipher, per-install key in Windows Credential Manager/DPAPI); on by default for new installs. | [V1] | ALL | 03:SEC-003, 07§5.3, 01§9.8 |
| DAT-019 | UPS awareness: on battery, show a banner and force a checkpoint and backup. | [V1] | ALL | 05§8.2 |
| DAT-020 | Antivirus-exclusion guidance for the data folder (installer note and help page). | [MVP] | ALL | 05§8.2 |
| DAT-021 | Point-in-time recovery from a journal. | [V2+] | ALL | 03:BCK-007 |
| DAT-022 | Legal retention: fiscal documents never hard-deleted; kept ≥ 10 years (documents) and ≥ 6 years after the year (data and technical documentation); archive per fiscal year. | [MVP] | ALL | 04§2.7, 04§6.1, 04§15 |
| DAT-023 | Backup to a cloud-synced folder chosen by the user (Google Drive or OneDrive desktop folder), always encrypted and verified like other targets. | [V1] | ALL | 01§4.1 (Dipposoft), 01§9.8 |
| DAT-024 | Optional automatic backup when the app closes, on top of Z-close and scheduled backups. | [MVP] | ALL | 01§4.1 (SMART-Com), 01§9.8 |
| DAT-025 | Uninstall and reinstall never delete data: the data folder lives outside the program folder and a reinstall re-attaches it; deleting data needs an explicit, confirmed choice. | [MVP] | ALL | 01§4.1 (Smart Barcode), 01§9.8 |

## 3. Multi-till LAN and multi-store (LAN)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| LAN-001 | Several registers/terminals per shop ("monoposte ou multi-caisses"). | [MVP] | ALL | 03:CHK-002, 05§9 |
| LAN-002 | Multi-till on a LAN with no internet: the same app runs as *standalone*, *host* (embedded HTTP/WebSocket server) or *client* (till). | [MVP] | ALL | 03:OFF-007, 05§9, 07§6.2 |
| LAN-003 | Tills keep selling when the host or the LAN is down: local replica plus outbox, idempotent push when the host returns. | [MVP] | ALL | 05§9, 07§6.2, 07§18 (R8), 01§1 (9), 01§9.1 |
| LAN-004 | Automatic discovery (mDNS `_dzpos._tcp`, UDP broadcast) with manual IP fallback; one-time pairing by QR or PIN. | [MVP] | ALL | 05§9, 07§6.2 |
| LAN-005 | Per-till document numbering series (prefix per till) so numbers stay unique offline. | [MVP] | ALL | 05§9, 07§5.4 |
| LAN-006 | Master data (products, prices, customers, users, settings) is host-authoritative and pushed to tills (delta pulls, session changesets or WebSocket). | [MVP] | ALL | 07§6.2, 01§4.2 (TifawtTech) |
| LAN-007 | Transactions created on the till with UUIDv7 IDs, stock as the sum of movements, so merges never conflict. | [MVP] | ALL | 07§5.4, 07§6.2 |
| LAN-008 | Conflict resolution for offline master-data edits: stock as deltas, host-wins with field-level audit, event log. | [V1] | ALL | 03:OFF-005, 07§6.2 |
| LAN-009 | Per-till 256-bit pairing token; optional self-signed TLS pinned at pairing; TLS for all network traffic. | [MVP] | ALL | 03:SEC-005, 07§6.2 |
| LAN-010 | "Promote this till to host" manual failover. | [V1] | ALL | 07§18 (R8) |
| LAN-011 | Unlimited registers and stores, free. | [MVP] | ALL | 03:MST-009 |
| LAN-012 | Several stores under one account (through the DZBuild link). | [V2+] | ALL | 03:MST-001 |
| LAN-013 | Per-store settings (receipt, taxes, prices). | [V2+] | ALL | 03:MST-002 |
| LAN-014 | Central catalogue push from HQ; stores push transactions to HQ. | [V2+] | ALL | 03:MST-003, 03:OFF-008 |
| LAN-015 | Inter-store transfers. | [V2+] | ALL | 03:MST-004 |
| LAN-016 | Consolidated multi-store reporting. | [V2+] | ALL | 03:MST-005, 03:RPT-022 |
| LAN-017 | Per-location performance insights. | [V2+] | ALL | 03:MST-007 |
| LAN-018 | Several legal entities (multi-company) in one install. | [V2+] | ALL | 03:MST-008, 09§3.2 (GECO SOFT), 01§9.8 |
| LAN-019 | Owner view on the shop LAN, no cloud needed: an Android companion or a browser page served by the host shows live sales, cash, stock and alerts over local Wi-Fi. | [V1] | ALL | 01§1 (10), 01§4.2 (AlgoStock), 01§9.9 |
| LAN-020 | Central purchasing for several stores: HQ consolidates needs, orders from suppliers and dispatches to the shops. | [V2+] | ALL | 01§3.4 (Inabex ArreniA) |

## 4. Users, roles, permissions, security (USR)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| USR-001 | User accounts with PIN or password; unlimited staff accounts. | [MVP] | ALL | 03:EMP-001, 03:EMP-015 |
| USR-002 | Roles with granular per-action permissions: refund, discount above x %, price override, open drawer, delete line, void, see cost, run reports, export, settings, stock adjustment, delete customer credit. | [MVP] | ALL | 03:EMP-002, 03:SEC-001, 07§11, 01§9.8 |
| USR-003 | Default role set: admin/owner, manager/supervisor, cashier, technician, accountant, waiter, employee (as in AlgoStock and Vyvex). | [MVP] | ALL | 02§2.1, 02§3.14, 01§9.8 |
| USR-004 | Manager override: one-time approval by manager PIN. | [MVP] | ALL | 03:EMP-003, 01§3.12 (Planète CAISSE) |
| USR-005 | Minimal-rights cashier profile. | [MVP] | ALL | 03:EMP-004 |
| USR-006 | Several cashiers on one session; fast user switch by PIN or badge. | [MVP] | ALL | 03:CHK-003 |
| USR-007 | Badge login: barcode/QR staff card, 125 kHz RFID reader (HID wedge), magnetic card, iButton. | [V1] | ALL | 03:EMP-012, 03:HW-014, 05§10 |
| USR-008 | USB fingerprint-reader login (ZKTeco ZK4500/SLK20R class). | [V2+] | ALL | 05§10 |
| USR-009 | Restrict users to specific registers or stores. | [V1] | ALL | 03:EMP-011 |
| USR-010 | Password hashing; PIN lockout after failed attempts. | [MVP] | ALL | 03:SEC-002 |
| USR-011 | Register lock screen: auto-lock after idle, unlock with PIN. | [MVP] | ALL | 03:CHK-006, 03:SEC-004 |
| USR-012 | Hide cost and margin from cashiers. | [MVP] | ALL | 03:PRC-009 |
| USR-013 | Back-office access control separate from till access. | [MVP] | ALL | 03:AUD-007 |
| USR-014 | Export permission (controls data exfiltration). | [MVP] | ALL | 03:SEC-012 |
| USR-015 | Duplicate a role or user profile as the starting point for a new one. | [MVP] | ALL | 01§3.12 (Planète CAISSE), 01§9.8 |

## 5. Audit trail, fiscal journal, loss prevention (AUD)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| AUD-001 | Tamper-resistant audit log of sensitive actions: void, line delete, discount, price override, refund, drawer open, stock adjustment, settings change, login/logout. | [MVP] | ALL | 03:AUD-001, 07§11 |
| AUD-002 | Change history per record (who, when, old → new). | [MVP] | ALL | 03:AUD-002 |
| AUD-003 | "Conforme, inaltérable" is the only mode: validated receipts and invoices can't be edited or deleted; cancellations are reversal documents (void, avoir); no hidden or dual ("non facturable") stock. | [MVP] | ALL | 03:AUD-003, DZ-CMP-01, 09§5.5, 04§2.1 |
| AUD-004 | Append-only fiscal journal with SHA-256 hash chain (`prev_hash`), periodic signed checkpoints, and a "Vérifier l'intégrité" tool that walks the chain and flags gaps. Required for the 2027 publisher engagement. | [MVP] | ALL | 03:AUD-005, 04§6.1, 07§11, 01§11, 01§12 (10) |
| AUD-005 | Perpetual grand totals per register that can never be reset (Z counters). | [MVP] | ALL | 04§6.1 |
| AUD-006 | System-clock change detection and logging. | [MVP] | ALL | 04§6.1 |
| AUD-007 | Audit trail of configuration changes (tax rates, prices, numbering, users). | [MVP] | ALL | 04§6.1 |
| AUD-008 | Receipt voids require authentication. | [MVP] | ALL | 03:AUD-008, 01§9.1 |
| AUD-009 | Drawer-opened-without-sale events logged; "no sale" opens limited. | [MVP] | ALL | 03:CSH-010, 05§5 |
| AUD-010 | Fiscal archive export per period: all documents, journal, hash chain, schema documentation and integrity manifest (CSV/JSON + PDF). | [MVP] | ALL | 04§5.6, 04§6.1, 04§15 |
| AUD-011 | Publisher-engagement pack for 1 Jan 2027: signed "engagement de l'éditeur" template plus technical documentation (data dictionary, algorithms, export formats), ready for CPF 20 ter / 51 sexies requests. | [MVP] | ALL | 04§6.1, 09§2.1 (P6), 01§11, 01§12 (10) |
| AUD-012 | Pluggable exporter for the dematerialised accounting-entry format the pending arrêté will define. | [V1] | ALL | 04§6.1 |
| AUD-013 | CCTV text overlay: send each receipt line over TCP to a Hikvision-compatible NVR "POS" input. | [V2+] | ALL | 03:AUD-006, DZ-INT-03, 09§7.4, 01§9.1 |

## 6. Personal-data compliance, loi 18-07 as amended by loi 25-11 (PDP)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PDP-001 | Local-first storage in Algeria by default; nothing leaves the PC unless the merchant links DZBuild. | [MVP] | ALL | 03:SEC-008, 04§7 (art. 44–45), 02§4.7 |
| PDP-002 | Explicit, revocable marketing consent flag per customer (SMS, WhatsApp, loyalty); invoices rely on legal obligation/contract. | [MVP] | ALL | 03:CRM-006, 04§7 (art. 7) |
| PDP-003 | Automated personal-data access log: every read, write, export, erase of customer PII with date, time, user and reason. | [MVP] | ALL | 04§7 (art. 41 bis 3) |
| PDP-004 | Data-subject tools: export a customer's data, rectify, anonymise/erase (respecting fiscal retention). | [MVP] | ALL | 04§7 (art. 34–36) |
| PDP-005 | DPO contact field and a processing-register template in settings. | [MVP] | ALL | 04§7 (art. 41 bis, 41 bis 2) |
| PDP-006 | Printable privacy notice and optional ticket footer (controller identity, purposes, rights). | [V1] | ALL | 04§7 (art. 32) |
| PDP-007 | Pre-filled ANPDP declaration guide for the merchant's customer file. | [V1] | ALL | 04§7 (art. 12–14) |
| PDP-008 | Encryption and role-based access for customer PII. | [MVP] | ALL | 04§7 (art. 38), DAT-018 |
| PDP-009 | Sensitive-data guard: no health or religion fields by default (pharmacy pack handles prescriptions with care). | [MVP] | ALL, PHA | 04§7 (art. 18) |
| PDP-010 | DZBuild cloud acts as sous-traitant: DPA in the terms, breach notification to ANPDP within 5 days, Algerian hosting or ANPDP authorisation for transfers. | [V1] | ALL | 04§7 (art. 39, 45 bis 8–10), 06§7 |

## 7. Localisation, accessibility, UX (UX)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| UX-001 | Natively trilingual UI: Arabic, French and English are equal first-class languages. Every screen, dialog, message, error, document and label template, report, export header, notification, help page and the installer ships complete in all three; there is no "main" language and no untranslated fallback shown to users. | [MVP] | ALL | 03:L10N-001, 02§4.8, 01§9.8, owner decision 2026-09-28 |
| UX-002 | Tamazight (Latin) UI as a community translation. | [V2+] | ALL | UX-DZ-10 |
| UX-003 | Full RTL layout for Arabic and LTR for French and English (mirrored layouts, logical CSS properties, direction-aware icons); switch language at runtime without restart. | [MVP] | ALL | 03:L10N-002, 02§2.1 (AlgoStock), 01§4.2 (AlgoStock) |
| UX-004 | Document language chosen per document, per document type and per customer: receipts, invoices, quotes, proformas, BL, credit notes, statements, dunning letters and labels print in Arabic, French or English. Each customer has a preferred document language (shop default otherwise); the cashier can override it at print or reprint time; the language is stored with the document so reprints match. | [MVP] | ALL | 03:L10N-003, 01§9.4, owner decision 2026-09-28 |
| UX-005 | DZD formatting with one house style per language (override ICU): `1 234,56 DA` (FR), `1 234,56 دج` (AR), `1,234.56 DZD` (EN), configurable; integer centimes internally. | [MVP] | ALL | 03:L10N-004, 04§8.1, 09§2.2 |
| UX-006 | Optional "prix en centimes" display (250 DA → "25 000") on customer display and labels for older customers. | [V1] | ALL | UX-DZ-08, 04§8.1 |
| UX-007 | Optional Hijri date on tickets and reports (Umm al-Qura) with a ±1-day correction. | [V1] | ALL | 03:L10N-005, UX-DZ-09, 04§10 |
| UX-008 | Dark theme. | [MVP] | ALL | 03:L10N-006, 01§9.8 |
| UX-009 | Large touch targets (≥ 44 px) and font scaling. | [MVP] | ALL | 03:L10N-007 |
| UX-010 | Full keyboard-only operation. | [MVP] | ALL | 03:L10N-008 |
| UX-011 | High-contrast, colour-blind-safe status colours. | [V1] | ALL | 03:L10N-009 |
| UX-012 | Screen-reader labels (WCAG 2.2 AA). | [V2+] | ALL | 03:L10N-010 |
| UX-013 | Selectable themes. | [V2+] | ALL | 03:L10N-011, 01§9.8 |
| UX-014 | Numerals: Western digits 0–9 by default in all three languages (UI, documents, reports, labels); Arabic-Indic digits (٠–٩) optional for Arabic display and printing, per user and per document type; both digit sets accepted as input everywhere. | [MVP] | ALL | 03:L10N-012, UX-DZ-02, owner decision 2026-09-28 |
| UX-015 | Touch-first register with large tiles, for all-in-one terminals. | [MVP] | ALL | 03:UX-001, 01§9.1 |
| UX-016 | Barcode-first flow: scanner input captured globally, no clicks needed. | [MVP] | ALL | 03:UX-002 |
| UX-017 | Keyboard shortcuts F1–F12 (pay, search, qty, discount, customer, park) in a central, user-remappable hotkey map. | [MVP] | ALL | 03:UX-003, 07§9, 01§9.1 |
| UX-018 | Keymap presets that mimic popular local tills (e.g. DS RAPIDE: F1 hold, F2/F3 quantity, F4 search, F5 delete line, F6 clear, F9 pay, F10 history, F12 cancel, Ctrl+D/R/C/B), selectable at setup, so switchers keep their muscle memory. | [V1] | ALL | UX-DZ-05, 01§4.2 (DS RAPIDE), 01§12 (3) |
| UX-019 | Layout customisable per user or register. | [V1] | ALL | 03:UX-004 |
| UX-020 | Retail, restaurant, wholesale and mixed sale screens, switchable in one app (café + épicerie hybrids). | [MVP] | ALL | 03:UX-005, 03§4 (G9), 01§12 (9) |
| UX-021 | Usable from 1024×768 (4:3 15″) and 1366×768. | [MVP] | ALL | 03:UX-006, 05§6.3 |
| UX-022 | Instant search: < 100 ms on 100k SKUs. | [MVP] | ALL | 03:UX-007 |
| UX-023 | Inline undo instead of modal confirmations. | [MVP] | ALL | 03:UX-008 |
| UX-024 | Status bar: online/offline, sync, host link, printer, drawer, user. | [MVP] | ALL | 03:UX-009 |
| UX-025 | Contextual help tooltips. | [V1] | ALL | 03:UX-010 |
| UX-026 | Command palette (Ctrl+K) in the back office. | [V1] | ALL | 03:UX-011 |
| UX-027 | Notification centre: low stock, expiring lots, debts due, instalments late, backup status, fiscal deadlines. | [V1] | ALL | 03:UX-012 |
| UX-028 | One app with role-based modes (till and back office), not two apps. | [MVP] | ALL | 03:UX-013 |
| UX-029 | Mobile-friendly back office through the DZBuild web app. | [V2+] | ALL | 03:UX-014 |
| UX-030 | "Mode simple": big photo tiles, three buttons (Vendre / Crédit / Fermer caisse), 18–20 px base font, 56 px targets; back office hidden until unlocked. For low-literacy and older owners. | [V1] | ALL | UX-DZ-01, 09§9 |
| UX-031 | AZERTY-aware numeric entry: in qty/price fields map unshifted `& é " ' ( - è _ ç à` to 1–0. | [MVP] | ALL | UX-DZ-02 |
| UX-032 | Search normalisation across scripts: Arabic (أ/إ/آ→ا, ة→ه, ى→ي, strip tashkeel and tatweel, Arabic-Indic digits), Latin (accent- and case-insensitive French and English), Arabizi aliases ("7lib" ↔ حليب) through a user-editable alias field; one query matches the AR, FR and EN names of a product, customer or supplier. | [MVP] | ALL | UX-DZ-03, 03:CHK-011, 01§9.2, owner decision 2026-09-28 |
| UX-033 | Bidi isolation (FSI/PDI) around amounts, barcodes, phone numbers and Latin names inside Arabic lines on screen, receipts and PDFs. | [MVP] | ALL | UX-DZ-04 |
| UX-034 | Sound cues: scan OK/error, drawer open, credit limit reached. | [MVP] | ALL | UX-DZ-06 |
| UX-035 | Printable one-page quick card (AR/FR/EN) generated from the current keymap and settings. | [V1] | ALL | UX-DZ-07 |
| UX-036 | On-screen numpad and keyboard (AZERTY + Arabic) for touch terminals. | [MVP] | ALL | 05§6.3 |
| UX-037 | Darija-friendly labels and wording options. | [V1] | ALL | 02§8, 01§4.5 (Codeshade, LogiTools) |
| UX-038 | Standard sale in three taps or fewer (UX acceptance KPI). | [MVP] | ALL | 03:CHK-041 |
| UX-039 | DZBuild visual identity: amber `#F59E0B`, brand yellow `#FFC300`, Cairo (Arabic) + Inter (Latin), fonts bundled offline. | [MVP] | ALL | 06§2 |
| UX-040 | Display density per user: compact (dense numbers, more rows, as in DS RAPIDE) or comfortable. | [V1] | ALL | 01§4.2 (DS RAPIDE), 01§9.8 |
| UX-041 | UI language per user: each user profile stores Arabic, French or English and the app switches at login or PIN switch; each till has a default for its login screen; two cashiers sharing a till can work in different languages; the customer display follows the document language. | [MVP] | ALL | owner decision 2026-09-28, 01§9.8 |
| UX-042 | Bilingual document layouts: Arabic + French on one receipt, invoice, quote, BL, statement or label (paired headers and legal mentions, an Arabic RTL block and a French LTR block, both names of each product), selectable per document type or per customer; AR + EN and FR + EN pairs as options. | [MVP] | ALL | owner decision 2026-09-28, 03:DOC-006, 04§9, 01§9.4 |
| UX-043 | Reports, dashboards and exports in any of the three languages: titles, column headers, units, month and day names, payment-method and reason labels follow the user's language, with a language choice at export (XLSX, CSV, PDF). | [MVP] | ALL | owner decision 2026-09-28, 01§9.7 |
| UX-044 | Language-aware text templates: receipt header and footer, thank-you text, return policy, legal mentions, and WhatsApp/SMS/email messages (receipts, debt reminders, "ready" notices, statements) kept per language; the customer's preferred language picks the template. | [MVP] | ALL | owner decision 2026-09-28, 01§12 (7) |

## 8. Onboarding, help, training, support (ONB)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| ONB-001 | Setup wizard in Arabic, French or English (language picked on the first screen): business type → preset categories, taxes, fiscal regime, receipt layout, document languages, hardware. | [MVP] | ALL | 03:ONB-001 |
| ONB-002 | Demo / sample data mode, switchable and erasable. | [MVP] | ALL | 03:ONB-002, UX-DZ-11 |
| ONB-003 | Training mode: sales not posted, clearly watermarked. | [V1] | ALL | 03:ONB-003, 03:CHK-007 |
| ONB-004 | Help centre, user manual and in-app help, available offline in Arabic, French and English at release. | [MVP] | ALL | 03:ONB-004, owner decision 2026-09-28 |
| ONB-005 | Community forum. | [V1] | ALL | 03:ONB-005 |
| ONB-006 | Live chat support (24/7 only if demand appears). | [V2+] | ALL | 03:ONB-006 |
| ONB-007 | Migration assistance from other POS (wizard plus support). | [V1] | ALL | 03:ONB-007, 01§6 |
| ONB-008 | Business-type templates with default categories, units, taxes and settings for every vertical pack in §30, with names preloaded in AR/FR/EN (superette template ships regulated-price products). | [MVP] | ALL | 03:ONB-008, 09§11 |
| ONB-009 | Hardware wizard: detect printers, test print, drawer kick, scanner test barcode, scale test. | [MVP] | ALL | 03:ONB-009, 03:HW-020 |
| ONB-010 | Video tutorials (Darija, Arabic and French audio) with Arabic, French and English subtitles. | [V1] | ALL | 03:ONB-010, 02§4.8 |
| ONB-011 | "Get help" button that starts a remote-support session (separate AnyDesk/RustDesk executable, not linked into the POS). | [V1] | ALL | DZ-INT-06, 09§2.1 (P7), 01§9.8 |
| ONB-012 | "Contacter le support" button that opens WhatsApp (or copies the number) with the app version and a diagnostic summary, plus published support hours. | [V1] | ALL | 01§1 (12), 01§6 |
| ONB-013 | Reseller and installer kit for hardware shops (the main sales channel): offline installer, demo data, training material in AR/FR/EN, certification and a public list of approved installers. | [V1] | ALL | 01§1 (15), 01§6, 01§4.3 (Kiober) |

## 9. Company profile and fiscal settings (CFG)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| CFG-001 | Company profile: raison sociale or nom/prénom, forme juridique, nature d'activité, capital social, address, phone/fax/email, RC (or N° carte auto-entrepreneur), NIF, NIS, AI, NIN, RIB/CCP, logo, social handles. | [MVP] | ALL | 03:ADM-005, 03:TAX-008, 04§15 (1) |
| CFG-002 | Fiscal regime switch: IFU / Réel / Auto-entrepreneur / Simplifié. Drives invoice layout (IFU and AE hide TVA, invoice TTC) and the report set. | [MVP] | ALL | 03:TAX-011, 04§5 |
| CFG-003 | BL-as-invoice authorisation (decision n° and date); BL-as-invoice mode refused when empty. | [MVP] | WHS | 04§2.1 |
| CFG-004 | Identifier validation: NIF/NIS 15 digits (18 for secondary establishment), NIN 18 digits, RIP and CCP key checks. | [MVP] | ALL | 04§2.2, 04§11.2, 02§3.5 |
| CFG-005 | Document numbering: independent gapless series per document type, year and register/till, with configurable prefixes (e.g. `FAC-2026-000001`, `T02-2026-000123`). | [MVP] | ALL | 03:ADM-006, 03:DOC-012, 04§2.7, 01§9.4 |
| CFG-006 | Fiscal calendar with editable deadlines and reminders: G50 (20th), G50 ter, G12 (30/06), G12 bis (20/01), IFU instalments (1–15 Sept, 1–15 Dec), G29 (30/04), ERA (30/09), TFPC, 1 Jan 2027 engagement. | [V1] | ALL | 04§10 |
| CFG-007 | Holidays and weekend (Friday–Saturday) calendar; Islamic holidays editable each year (moon sighting). | [V1] | ALL | 04§10 |
| CFG-008 | Business-day cut-off time so after-midnight Z closes go to the right day; time zone Africa/Algiers (UTC+1, no DST). | [MVP] | ALL | 04§10, 09§10.2 |
| CFG-009 | Opening-hours profiles per day and a "Ramadan mode" with shift templates. | [V1] | ALL | 04§10 |
| CFG-010 | Wilaya/commune dataset: 69 wilayas and 1,541 communes (loi 26-06, décret 26-206 spellings), plus a 58-wilaya code mapping for carriers still on the old list. | [MVP] | ALL | 04§12.3, 09§5.10 |

---
## 10. Sale screen (SAL)

### 10.1 Session and register

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| SAL-001 | Open a register session (shift) before selling, with the counted opening float (fond de caisse). | [MVP] | ALL | 03:CHK-001, 03:CSH-001, 02§4.4, 01§9.1 |
| SAL-002 | Business day (work period) separate from shifts; the Z report is per business day. | [MVP] | ALL | 03:CHK-005 |
| SAL-003 | Parked and parallel sales: several open carts at once; hold a ticket while the customer fetches an item ("خاصية الإنتظار"), retrieve it later. | [MVP] | ALL | 03:CHK-004, 03:CHK-032, 02§4.5, 01§1 (7), 01§9.1 |

### 10.2 Adding items

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| SAL-010 | Product grid with categories, colours and images; configurable tile layout; favourites pages and quick keys with pictures (bread, milk, eggs, café, thé). | [MVP] | ALL | 03:CHK-010, 03:CHK-027, 02§3.1, 02§3.4, 01§9.1 |
| SAL-011 | Universal search: name (AR/FR/EN), SKU, barcode, internal/manufacturer/supplier/old reference, OEM reference, DCI, laboratory, ISBN. | [MVP] | ALL | 03:CHK-011, 02§3.5, 02§3.6, 02§3.7, 01§9.1 |
| SAL-012 | Barcode scan adds the item (HID wedge or serial scanner, layout-proof). | [MVP] | ALL | 03:CHK-012 |
| SAL-013 | Camera (webcam) barcode scanning. | [V2+] | ALL | 03:CHK-013 |
| SAL-014 | Weight- or price-embedded EAN-13 (prefixes 20–29) decoded at checkout, configurable per scale brand. | [MVP] | SUP FRS BAK | 03:CHK-014, 02§3.3, 05§4.2, 01§9.1 |
| SAL-015 | Live weight read from a serial/USB checkout scale, with stable/unstable indicator and tare. | [V1] | FRS SUP BAK | 03:CHK-015, 05§4.3, 01§9.1 |
| SAL-016 | Quantity entry by numpad, "qty ×" multiplier, decimal quantities (3 decimals for kg, metres, m²). | [MVP] | ALL | 03:CHK-016, 02§3.3, 01§9.1 |
| SAL-017 | Price override on a line (permission-gated, logged). | [MVP] | ALL | 03:CHK-017 |
| SAL-018 | Custom/open item: typed name and price, no catalogue entry. | [MVP] | ALL | 03:CHK-018, 01§9.1 |
| SAL-019 | Department keys / "calculatrice" mode: open-price sale by department ("Légumes", "Pain", "Flexy"), numeric keypad first, no catalogue needed (replaces standalone cash registers). | [MVP] | SUP KSK FRS | DZ-CHK-01, 09§3.3, 01§3.8 (DELTA POS generic codes) |
| SAL-020 | Line notes and order notes. | [MVP] | ALL | 03:CHK-019 |
| SAL-021 | Assign a salesperson (vendeur) per line or per sale, for commissions. | [V1] | ALL | 03:CHK-020 |
| SAL-022 | Product quick-info on long press: stock in other locations, prices, description. | [V1] | ALL | 03:CHK-021 |
| SAL-023 | Variant picker popup (size/colour matrix). | [MVP] | CLO COS TEL | 03:CHK-022 |
| SAL-024 | Modifier popup: forced and optional groups (sauce, fromage, taille). | [MVP] | RST | 03:CHK-023, 02§3.4 |
| SAL-025 | Serial/IMEI or lot prompt when selling a tracked item. MVP for phone and electronics packs. | [V1] | TEL INS PHA | 03:CHK-024 |
| SAL-026 | Combo/menu builder at sale time. | [V1] | RST | 03:CHK-025 |
| SAL-027 | Upsell / suggested items. | [V2+] | ALL | 03:CHK-026 |
| SAL-028 | Price-check mode (look up without selling). | [MVP] | ALL | 03:CHK-028 |
| SAL-029 | Repeat last line or last sale. | [MVP] | ALL | 03:CHK-029 |
| SAL-030 | Regulated-price guard: block or warn when the sale price exceeds the legal ceiling; show "subventionné". | [MVP] | SUP | DZ-CHK-02, 09§5.3 |
| SAL-031 | Channel field per sale (comptoir, téléphone, WhatsApp, Facebook, DZBuild, Yassir, other) with per-channel commission. | [V1] | ALL | DZ-CHK-03 |
| SAL-032 | Used-goods ("occasion") flag on a line, with the 3-month minimum warranty. | [V1] | TEL INS | DZ-CHK-05, 09§5.1 |
| SAL-033 | Container deposit line (consigne), empty-for-full exchange. | [V1] | CNS | DZ-CHK-06 |
| SAL-034 | Gold sale line: weight × gold price per gram (by titre) + façon; old-gold trade-in line (weight, titre, loss %); buyer ID captured. | [V1] | JEW | DZ-CHK-07 |
| SAL-035 | Membership check-in at the till (scan card/RFID → validity, remaining visits). | [V2+] | GYM | DZ-CHK-08 |
| SAL-036 | Audible scan beep (ok/error) and a big "last scanned item" banner for busy tills. | [MVP] | ALL | DZ-CHK-09 |
| SAL-037 | Customer balance, debt, credit limit and points visible at the till before validating a credit sale. | [MVP] | ALL | 03:CRM-008, 02§4.1 |
| SAL-038 | Merge two open tickets into one (the customer adds items from another cart). | [V1] | ALL | 01§4.2 (DS RAPIDE), 01§9.1 |

### 10.3 Cart operations

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| SAL-040 | Change quantity, remove a line (line void logged). | [MVP] | ALL | 03:CHK-030 |
| SAL-041 | Cancel the whole sale (permission-gated, logged). | [MVP] | ALL | 03:CHK-031 |
| SAL-042 | Attach a customer: search, lookup by phone, scan loyalty card, create inline. | [MVP] | ALL | 03:CHK-033 |
| SAL-043 | Checkout validations: customer required for credit sales; buyer name for warranted items; NIF/RC/AI for B2B invoices and wholesale. | [MVP] | ALL | 03:CHK-034, 04§2.2, 09§5.1 |
| SAL-044 | Sale type / dining option: sur place, à emporter, livraison. | [MVP] | RST | 03:CHK-035, 03:RST-013, 01§9.10 |
| SAL-045 | Gift line: zero price with a reason. | [V1] | ALL | 03:CHK-036 |
| SAL-046 | Convert the cart to devis, bon de commande, facture proforma, facture or bon de livraison. | [MVP] | ALL | 03:CHK-038 |
| SAL-047 | Customer-facing display mirrors the cart (second monitor or VFD). | [MVP] | ALL | 03:CHK-039, 01§9.1 |
| SAL-048 | Live totals: subtotal, discounts, taxes, timbre, total, item count. | [MVP] | ALL | 03:CHK-040 |

### 10.4 Finishing the sale

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| SAL-050 | Payment screen with quick-cash buttons (exact, 200/500/1000/2000 DA) and change due. | [MVP] | ALL | 03:CHK-050, 04§8.1 |
| SAL-051 | One-click payment validation. | [MVP] | ALL | 03:CHK-051 |
| SAL-052 | Receipt choice: print, none, WhatsApp link/PDF, email, SMS (email/SMS need connectivity or a gateway). | [MVP] | ALL | 03:CHK-052, 01§9.1 |
| SAL-053 | Kick the cash drawer on cash payment. | [MVP] | ALL | 03:CHK-053 |
| SAL-054 | Issue an invoice at payment for a B2B customer; one-click "convert ticket → facture" afterwards. | [MVP] | ALL | 03:CHK-054, 04§0 (3) |
| SAL-055 | Ship later / deliver later. | [V1] | ALL | 03:CHK-055 |
| SAL-056 | Reprint the last receipt; searchable receipt history (reprints marked DUPLICATA). | [MVP] | ALL | 03:CHK-056 |
| SAL-057 | Live timbre display for cash tenders, with a hint to pay by card/QR near the bracket cliffs (30,000 / 100,000 DA). | [MVP] | ALL | 04§4.4 |

## 11. Payments and tenders (PAY)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PAY-001 | Cash (espèces). | [MVP] | ALL | 03:PAY-001 |
| PAY-002 | Card on a standalone bank TPE (CIB / Edahabia) recorded by hand: method, authorisation number, last 4 digits; end-of-day reconciliation against the TPE batch report. | [MVP] | ALL | 03:PAY-003, 05§11, 04§11, 01§3.1 (Onyx) |
| PAY-003 | Integrated card terminal (ECR, e.g. Ingenico Concert) only where the bank and GIE Monétique allow it. | [V2+] | ALL | 03:PAY-002, 03:HW-011, 05§11, 09§5.8 |
| PAY-004 | Split payment across several tenders (mixed: part cash, part QR, rest on credit). | [MVP] | ALL | 03:PAY-004, 02§4.6, 01§9.1 |
| PAY-005 | Partial payment: pay part now, the rest later (credit, layaway, instalments). | [MVP] | ALL | 03:PAY-005 |
| PAY-006 | Custom payment types, configurable. | [MVP] | ALL | 03:PAY-006 |
| PAY-007 | Algerian tender set out of the box: Espèces; Chèque (n°, bank, date, drawer); Carte CIB; Carte Edahabia; QR/BaridiMob (transaction or SMS reference); Virement bancaire/CCP (order date, execution date, banks); Prélèvement CCP; Crédit client; Avoir/bon d'achat; Contre-remboursement (COD); payment link. | [MVP] | ALL | 04§11.2, 02§4.6, 01§9.1 |
| PAY-008 | Payment reference capture for every non-cash tender (cheque n°, bank, transfer ID, QR code). | [MVP] | ALL | 03:PAY-028 |
| PAY-009 | QR payment: show the merchant's static BaridPay / CCP Business QR on the customer display and ticket; confirm manually (no public API). | [V1] | ALL | 03:PAY-007, 02§4.6 |
| PAY-010 | Gift card as a tender. | [V1] | ALL | 03:PAY-010 |
| PAY-011 | Store credit as a tender. | [MVP] | ALL | 03:PAY-011 |
| PAY-012 | Loyalty points as a tender. | [V1] | ALL | 03:PAY-012 |
| PAY-013 | On-account / customer credit tender with a credit limit (the digital carnet de crédit). | [MVP] | ALL | 03:PAY-013, 01§9.1 |
| PAY-014 | eWallet / prepaid balance top-up ("deposit money"). | [V1] | ALL | 03:PAY-014 |
| PAY-015 | Tips / gratuity presets, tip after payment. | [V2+] | RST | 03:PAY-015 |
| PAY-016 | Cash rounding (nearest 5 or 10 DA). | [MVP] | ALL | 03:PAY-016 |
| PAY-017 | Change-handling rules (no change on card tenders, maximum overpay). | [MVP] | ALL | 03:PAY-017 |
| PAY-018 | Foreign-currency cash with an exchange rate (opt). | [V2+] | ALL | 03:PAY-018 |
| PAY-019 | Surcharge or fee per payment method (opt). | [V2+] | ALL | 03:PAY-019 |
| PAY-020 | Restrict a payment method to identified customers. | [MVP] | ALL | 03:PAY-022 |
| PAY-021 | Write off small differences within a limit. | [V1] | ALL | 03:PAY-023 |
| PAY-022 | Settle a customer's old debts from the till: pick sales or invoices, full or partial (versement). | [MVP] | ALL | 03:PAY-027, 02§4.1 |
| PAY-023 | Timbre de quittance computed on the cash part only (split-tender rule configurable); exempt for electronic payments, and for cheque/transfer when the receipt carries the cheque/transfer details. | [MVP] | ALL | 04§4.3, 04§4.4 |
| PAY-024 | Cash-ceiling warnings: ≥ 1,000,000 DA for precious metals, vehicles, antiques; ≥ 5,000,000 DA for real estate (décret 15-153). | [V1] | JEW INS | 04§11.1, 09§4.3 |
| PAY-025 | Auto-reconciliation of tenders with accounting journals. | [V2+] | ALL | 03:PAY-026 |
| PAY-026 | Online payment links (Chargily Pay V2, Slick-Pay, SATIM e-payment) as an optional tender when online. | [V2+] | ALL | DZ-INT-04, 04§11.2, 09§5.9, 01§9.9 |

## 12. Returns, exchanges, voids (RET)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| RET-001 | Refund selected items or the whole receipt. | [MVP] | ALL | 03:RET-001 |
| RET-002 | Find the original sale by scanning the receipt barcode/QR, or by number, date or customer. | [MVP] | ALL | 03:RET-002 |
| RET-003 | Return without a receipt (permission-gated). | [MVP] | ALL | 03:RET-003, 01§9.1 |
| RET-004 | Exchange in one transaction (return + new items, pay or refund the difference); exchange is the norm in clothing shops (استبدال). | [MVP] | ALL | 03:RET-004, 02§3.8 |
| RET-005 | Mandatory return reason codes. | [MVP] | ALL | 03:RET-005 |
| RET-006 | Per returned line: restock, or write off as damaged (سلع تالفة). | [MVP] | ALL | 03:RET-006, 02§4.3 |
| RET-007 | Refund method: cash, original tender, store credit / exchange voucher, gift card. | [MVP] | ALL | 03:RET-007, 03:PAY-024 |
| RET-008 | Cross-location returns. | [V2+] | ALL | 03:RET-008 |
| RET-009 | Return-window policy (days) with a warning when exceeded. | [V1] | ALL | 03:RET-009 |
| RET-010 | Block refunds above the sold quantity; track quantity already returned. | [MVP] | ALL | 03:RET-010 |
| RET-011 | Void a completed sale in the same session vs refund later, both with authentication. | [MVP] | ALL | 03:RET-011, 01§9.1 |
| RET-012 | Credit note (avoir) in its own gapless series, referencing the original invoice number and date, negative lines, reason code. | [MVP] | ALL | 03:RET-012, 04§2.6, 01§9.4 |
| RET-013 | Return to supplier (RMA) generated from a customer return. | [V2+] | ALL | 03:RET-013 |
| RET-014 | Gift receipt (no prices). | [V1] | ALL | 03:RET-014 |
| RET-015 | Pharmacy "Retours" sale regime. | [V2+] | PHA | 02§3.5 |

## 13. Pricing and price lists (PRC)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRC-001 | Several sale prices per product: at least 4 named levels by default (détail, demi-gros, gros, professionnel/revendeur), unlimited and configurable (local products offer 3–6; COMMSoft offers 13). | [MVP] | ALL | 03:PRD-061, 02§4.2, 01§9.1 |
| PRC-002 | Named price lists assigned to customers or customer groups; customer type (retail, wholesale, garage, contractor) picks the price automatically. | [MVP] | ALL | 03:PRC-001, 02§4.2, 02§3.7, 01§9.1 |
| PRC-003 | Per-customer tariffs and discounts. | [MVP] | WHS QNC AUT | 02§2.1 (GestiumPRO), 02§4.2 |
| PRC-004 | Quantity-break pricing (minimum quantity for the gros price). | [MVP] | WHS | 03:PRC-002, 02§3.12, 01§9.1 |
| PRC-005 | Switch price list on the fly at the till. | [MVP] | ALL | 03:PRC-010 |
| PRC-006 | Date-bounded prices. | [V1] | ALL | 03:PRC-003 |
| PRC-007 | Formula price lists (cost + x %, or another list − y %); multi-criteria price tables. | [V1] | ALL | 03:PRC-004, 02§2.1 |
| PRC-008 | Time-of-day price lists (happy hour). | [V1] | RST | 03:PRC-006, 03:DSC-012, 03:RST-019, 01§9.10 |
| PRC-009 | Switch menu or price list per service (lunch, dinner, terrace). | [V1] | RST | 03:PRC-007 |
| PRC-010 | Batch-specific pricing. | [V2+] | PHA | 03:PRC-008 |
| PRC-011 | Separate online price list for products published to DZBuild. | [V1] | ALL | 06§5.5 |
| PRC-012 | Regulated-price data: product flag "prix réglementé", ceiling price and retail margin (oil, sugar, milk sachet, semolina), shipped as an updatable data pack. | [MVP] | SUP | 09§5.3, 04§9 |

## 14. Discounts, promotions, legal soldes (PRM)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRM-001 | Manual line discount (% or DA). | [MVP] | ALL | 03:DSC-001, 02§4.2, 01§9.1 |
| PRM-002 | Manual whole-order discount. | [MVP] | ALL | 03:DSC-002, 01§9.1 |
| PRM-003 | Predefined named discounts ("Staff 10 %"). | [MVP] | ALL | 03:DSC-003 |
| PRM-004 | Maximum discount per role; manager approval above the limit. | [MVP] | ALL | 03:DSC-004 |
| PRM-005 | Customer-group discounts. | [MVP] | ALL | 03:DSC-013 |
| PRM-006 | "You saved X DA" on receipt and customer display. | [MVP] | ALL | 03:DSC-019 |
| PRM-007 | Automatic rule-based discounts (no code). | [V1] | ALL | 03:DSC-005 |
| PRM-008 | Coupons and discount codes (single/multi-use, expiry, batch-generated). | [V1] | ALL | 03:DSC-006 |
| PRM-009 | Buy X get Y. | [V1] | ALL | 03:DSC-007 |
| PRM-010 | Tiered/volume promotions (quantity breaks, spend thresholds). | [V1] | ALL | 03:DSC-008 |
| PRM-011 | Mix & match / bundle price ("any 3 for 1000 DA", "2+1", lots). | [V1] | SUP COS | 03:DSC-009, 02§3.1, 01§9.1 |
| PRM-012 | Scheduled promotions (date range, days, hours): Ramadan, Aïd, rentrée. | [V1] | ALL | 03:DSC-011 |
| PRM-013 | Stacking, priority and exclusivity rules. | [V1] | ALL | 03:DSC-014 |
| PRM-014 | Discount tags by barcode (scan a "−20 %" sticker). | [V1] | ALL | 03:DSC-015 |
| PRM-015 | Receipt deals (threshold discount on the total). | [V1] | ALL | 03:DSC-018 |
| PRM-016 | Free item / gift with purchase. | [V1] | ALL | 03:DSC-020 |
| PRM-017 | Free goods (unités gratuites, UG) promotions on sale and on purchase. | [V1] | PHA WHS | 02§3.5, 02§3.12 |
| PRM-018 | Next-order coupon printed on the receipt. | [V2+] | ALL | 03:DSC-010 |
| PRM-019 | Multi-location promotions. | [V2+] | ALL | 03:DSC-016 |
| PRM-020 | Promo codes shared with the DZBuild online store. | [V2+] | ALL | 06§3.2 |
| PRM-021 | Legal campaign types (solde, promotion, liquidation, déballage) storing the **prix pratiqué auparavant**; soldes limited to the configured windows (2×/year, 6 weeks, Jan–Feb and Jul–Aug, wali dates per wilaya), warning outside them. | [MVP] | CLO ALL | DZ-PRM-01, 09§5.2 |
| PRM-022 | Declaration export per campaign (XLSX/PDF): designation, quantity, old price, new price, % reduction. | [MVP] | CLO ALL | DZ-PRM-02 |
| PRM-023 | Stock-age guard: soldes only on goods held ≥ 3 months (uses receipt dates per lot). | [V1] | CLO | DZ-PRM-06 |
| PRM-024 | Compliance warning in the promotion UI (undeclared soldes/promos can be stopped and goods seized). | [MVP] | ALL | 09§5.2 |
| PRM-025 | Prize-draw guard: no draw tied to a purchase or payment (design rule). | [MVP] | ALL | DZ-CMP-03 |
| PRM-026 | Crossed-out previous price shown on the sale screen, customer display and receipt while a promotion or solde runs. | [V1] | ALL | 01§4.5 (LogiTools), 01§9.1 |

## 15. Taxes and droit de timbre (TAX)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| TAX-001 | Tax codes as data: {code, label AR/FR/EN, rate, exempt flag, valid_from, valid_to}; 19 %, 9 %, 0 %, exempt; per product or category; end-dated exemptions (e.g. 31/12/2026). | [MVP] | ALL | 03:TAX-001, 04§3.1 |
| TAX-002 | Tax-inclusive (TTC) or tax-exclusive (HT) pricing; HT/TVA/TTC shown distinctly on invoices. | [MVP] | ALL | 03:TAX-002, 04§2.2, 01§11 |
| TAX-003 | Non-VAT regimes (IFU, auto-entrepreneur): TVA hidden, invoices TTC only (showing TVA is illegal for IFU). | [MVP] | ALL | 03:TAX-011, 04§2.2 |
| TAX-004 | Tax-exempt customers. | [MVP] | ALL | 03:TAX-005 |
| TAX-005 | Rounding rule configurable per line or per document; integer centimes, half-up to 0.01 DA, totals per rate reconcile with the G50. | [MVP] | ALL | 03:TAX-009, 04§3.2 |
| TAX-006 | Droit de timbre engine (Code du timbre art. 100, DGI circular 14/2025): ≤ 300 DA exempt; tranches = ceil(amount/100); rate 1 / 1.5 / 2 DA by the bracket of the **whole** amount (not progressive); minimum 5 DA; no ceiling; cash only; formula configurable. | [MVP] | ALL | 03:TAX-007, 04§4, 02§4.7, 01§9.4 |
| TAX-007 | Timbre on credit sales applies when the cash payment receipt (quittance) is issued, not at invoicing. | [MVP] | ALL | 04§4.4 |
| TAX-008 | Uniform 50 DA stamp on simple receipts of cash deposits (art. 100-II). | [V1] | ALL | 04§4.3 |
| TAX-009 | Exempt turnover tracked and reported per month (G50 "CA exonéré" boxes). | [MVP] | ALL | 04§3.1 |
| TAX-010 | Compound or multiple taxes on one line. | [V1] | ALL | 03:TAX-003 |
| TAX-011 | Specific taxes printed distinctly, flagged inside or outside the TVA base: tyres (750/450 DA), lubricants (outside), tobacco additional tax (75 DA/pack, inside), TIC. | [V1] | AUT KSK | 04§2.2, 04§14 |
| TAX-012 | Fiscal positions (tax mapping by customer type, eat-in vs takeaway). | [V1] | RST | 03:TAX-004, 03:RST-024 |
| TAX-013 | Purchase side: TVA deductible tracked per supplier invoice; invoices > 1,000,000 DA TTC paid in cash flagged non-deductible (CTCA 30). | [MVP] | ALL | 04§3.3 |
| TAX-014 | IFU tracker: annual CA vs 8,000,000 DA (5,000,000 DA for AE) with alerts at 70/90/100 %; IFU estimate at 5 % / 12 % / 0.5 % with minimum 30,000 / 10,000 DA; margin base for regulated products. | [V1] | ALL | 04§5.1 |
| TAX-015 | IFU 5 % withholding on sales to natural persons reselling through digital platforms (CIDTA 282 sexies A). | [V2+] | WHS | 04§5.1 |
| TAX-016 | E-invoicing / fiscal-device certification: watch item (no Algerian clearance obligation found as of 2026-09-28). | [V2+] | ALL | 03:TAX-010, 04§6.2, 01§4.5 (eFawtara, Fisira) |
| TAX-017 | Withholding lines on invoices (retenue à la source): configurable rate and base, printed distinctly and reported. | [V2+] | ALL | 01§9.4 |

## 16. Commercial documents and templates (DOC)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| DOC-001 | Full document set, each in its own gapless series: ticket de caisse, facture, avoir, devis, facture proforma, bon de commande (client and fournisseur), bon de livraison, bon de réception, bon de retour, bon de transfert, facture récapitulative mensuelle, bon de transaction commerciale. | [MVP] | ALL | 03:DOC-005, 04§15 (3), 01§9.4 |
| DOC-002 | Invoice with every décret 05-468 art. 3 mention (seller and buyer identity, legal form, capital, RC, NIS, NIF, AI, payment mode and due date, date and sequential number, designation, qty, PU HT, total HT, nature and rate of each tax, TTC in figures **and in letters**), stamp and signature box. | [MVP] | ALL | 04§2.1, 04§15 (4), 01§9.4, 01§11 |
| DOC-003 | Mandatory-field validation blocks invoice printing when a legal mention is missing. | [MVP] | ALL | 04§0 (1) |
| DOC-004 | Amount in words in Arabic, French and English with correct DZD grammar (dinars and centimes; Arabic dual and plural forms; French "cent/vingt" agreement), printed in the document language, or in both languages on bilingual layouts. In-house converter with unit tests per language (existing libraries are wrong for DZD). | [MVP] | ALL | 03:DOC-007, 04§8.2, owner decision 2026-09-28 |
| DOC-005 | Ticket de caisse content: seller name, address, RC, NIF, NIS, AI; ticket number; date and time; cashier; lines; tax breakdown (réel only); TTC; tenders; timbre; thanks text in the document language (UX-044). | [MVP] | ALL | 04§2.3 |
| DOC-006 | Proforma and devis in non-fiscal series, printed "FACTURE PROFORMA — ne constitue pas une facture". | [MVP] | ALL | 04§2.5 |
| DOC-007 | Cancelled invoices kept in the sequence and reprinted with a diagonal "FACTURE ANNULÉE" mark. | [MVP] | ALL | 04§2.1 |
| DOC-008 | Transport costs in the margin, price supplements (credit interest, commissions) and consignation lines on invoices. | [MVP] | ALL | 04§2.1 (art. 7–9), 04§15 (4) |
| DOC-009 | Bon de transfert with origin and destination addresses, driver name and capacity, stamp. | [MVP] | WHS ALL | 04§2.1 (art. 12–13) |
| DOC-010 | Bon de livraison as invoice substitute (authorised merchants only): authorisation decision n°/date, deliverer name and ID; monthly facture récapitulative referencing all BL numbers and dates. | [V1] | WHS | 04§2.1 (art. 14–17), 01§9.4 |
| DOC-011 | Bon de transaction commerciale (décret 16-66) for purchases from farmers, fishermen and artisans without RC; buyer signature box; "ANNULÉ" on cancellation. | [V1] | FRS BAK SUP | 04§2.4 |
| DOC-012 | Warranty certificate printed from the sale per serial/IMEI line with the décret 13-327 art. 6 mentions (guarantor, RC, buyer name, invoice/ticket n° and date, type/brand/serial, price, duration, representative). | [MVP] | TEL INS | DZ-CHK-04, 09§5.1, 01§3.8 (COMMSoft) |
| DOC-013 | Documents rendered correctly in each language: Arabic RTL with shaped text and bidi isolation, French and English LTR, the same legal content in all three; bilingual layouts per UX-042. | [MVP] | ALL | 03:DOC-006, 04§9, owner decision 2026-09-28, 01§9.4 |
| DOC-014 | Receipt header/footer, logo, contact info, social handles, DZBuild store QR ("Commandez en ligne"). | [MVP] | ALL | 03:DOC-001, 06§5.5 |
| DOC-015 | Customer address and notes printed on the receipt. | [MVP] | ALL | 03:DOC-003 |
| DOC-016 | Receipt number printed as barcode/QR (for returns). | [MVP] | ALL | 03:DOC-008 |
| DOC-017 | PDF export and print preview of every document; silent A4 printing (no dialog). | [MVP] | ALL | 03:DOC-013, 07§7.3 |
| DOC-018 | "DUPLICATA" marker on reprints. | [MVP] | ALL | 03:DOC-014 |
| DOC-019 | Terms, return policy or warranty text on receipts and invoices. | [MVP] | ALL | 03:DOC-016 |
| DOC-020 | Paper formats: thermal 58/80 mm, A4, A5; choose per document type. | [MVP] | ALL | 02§3.7, 05§1.2, 01§9.1 |
| DOC-021 | Digital documents: share receipts/invoices/statements as PDF or link by WhatsApp, email, SMS. | [V1] | ALL | 03:DOC-004, 02§4.1, 01§9.4 |
| DOC-022 | Visual template designer (drag and drop) for 58/80 mm, A4, A5; user-editable HTML templates. | [V1] | ALL | 03:DOC-002, 07§7.3, 01§9.4 |
| DOC-023 | Custom fields on documents (vehicle plate, warranty months, chantier). | [V1] | ALL | 03:DOC-015 |
| DOC-024 | Batch print or batch email of documents (monthly statements). | [V1] | ALL | 03:DOC-017 |
| DOC-025 | Dot-matrix output profile: text mode, fixed pitch 80/136 columns, continuous forms (A4, A5, 12″), N copies captioned ORIGINAL / DUPLICATA / TRIPLICATA. | [V1] | WHS ALL | DZ-DOC-01, 09§7.1 |
| DOC-026 | Paged, numbered, non-editable register printouts (registre des ventes/achats, livre d'inventaire, jeweller client register). | [V1] | ALL | 04§5.1, 09§5.4, 09§5.7 |
| DOC-027 | Facture complémentaire (supplementary invoice) referencing the original invoice, for price supplements or omitted lines. | [V1] | ALL WHS | 01§3.8 (COMMSoft), 01§9.4 |
| DOC-028 | Two credit-note kinds, both referencing the original invoice: avoir marchandise (goods returned, stock re-entered) and avoir financier (price reduction or commercial gesture, no stock movement). | [MVP] | ALL | 01§3.8 (COMMSoft), 01§9.4, 04§2.6 |
| DOC-029 | Printed bon d'entrée / bon de sortie de stock for adjustments, internal consumption, samples and gifts; also the "bon pour" slip some vendors list (exact use to confirm in field interviews). | [V1] | ALL | 01§9.4 |
| DOC-030 | Bon de route (delivery route slip): driver, vehicle plate, destinations, list of BLs and goods; printed from a delivery or a tour. | [V1] | WHS VAN ALL | 01§4.2 (KERS), 01§9.4, 01§12 (4) |
| DOC-031 | Invoice several delivery notes of one customer in one facture (BL grouping), with each BL number and date referenced. | [MVP] | WHS ALL | 01§4.1 (StockCom), 01§3.12 (BBS), 01§9.4 |
| DOC-032 | Verification QR code on invoices and tickets (seller NIF, number, date, total, journal hash); optional today, ready if the DGI requires one. | [V1] | ALL | 01§4.5 (Almawarid, eFawtara, Fisira), 01§9.4 |
| DOC-033 | Scanned company stamp (cachet) and signature image placed on PDF invoices, quotes and statements. | [V1] | ALL | 01§4.5 (eFawtara), 01§9.4 |
| DOC-034 | Arabic consumer-information guard (décret 13-378): when a consumer-facing label or ticket for food or regulated goods is printed in French or English only, warn and offer the bilingual AR + FR layout; configurable per product category. | [MVP] | SUP FRS BAK COS PHA | 04§9, owner decision 2026-09-28 |

---
## 17. Product catalogue (PRD)

### 17.1 Basics

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-001 | Hierarchical categories (families / sub-families). | [MVP] | ALL | 03:PRD-002, 01§9.2 |
| PRD-002 | Several barcodes per product (multi-barcode). | [MVP] | ALL | 03:PRD-003, 02§3.6, 01§9.1 |
| PRD-003 | Automatic SKU and internal EAN-13 / EAN-8 generation (internal prefix; GS1 Algeria prefix is 613 for registered codes). | [MVP] | ALL | 03:PRD-004, 05§3.2, 02§3.7, 01§9.2, 01§3.8 (DELTA POS) |
| PRD-004 | Product images (bulk import in IMX-018). | [MVP] | ALL | 03:PRD-005, 01§3.8 (DELTA POS) |
| PRD-005 | Brand, supplier reference, tags and extensible custom attributes (dimensions, material, compatibility, packaging, season). | [MVP] | ALL | 03:PRD-006, 02§3.6, 02§3.8, 01§9.2 |
| PRD-006 | Product types: stockable, consumable, service (repair labour, photocopies, pressing), non-inventory. | [MVP] | ALL | 03:PRD-007, 01§9.2 |
| PRD-007 | Active / archived; "available at the till" toggle. | [MVP] | ALL | 03:PRD-008 |
| PRD-008 | Product names in Arabic, French and English, each searchable; documents and labels print the name in the document language, with a configurable fallback when that name is empty. | [MVP] | ALL | 03:PRD-011, 01§9.2, owner decision 2026-09-28 |
| PRD-009 | Categories restricted per register. | [V1] | ALL | 03:PRD-009 |
| PRD-010 | Long description, allergens, nutritional info. | [V1] | RST BAK | 03:PRD-010 |
| PRD-011 | Several references linked to one article: internal, manufacturer, supplier, alternative and old codes; counter search by technical designation. | [MVP] | QNC AUT | 02§3.6, 02§3.7 |

### 17.2 Variants and modifiers

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-020 | Variants from an attribute matrix (size × colour × season), each with its own SKU, barcode, price, cost and stock; create all sizes/colours from one screen. | [MVP] | CLO COS TEL | 03:PRD-020, 02§3.8, 01§9.1 |
| PRD-021 | Modifiers / add-ons: priced, grouped, min/max, forced. | [MVP] | RST | 03:PRD-021, 01§9.10 |
| PRD-022 | Conversational modifiers (sans / extra / léger / à part). | [V1] | RST | 03:PRD-022, 01§9.10 |
| PRD-023 | Modifiers that deduct ingredient stock. | [V1] | RST | 03:PRD-023 |

### 17.3 Units of measure

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-030 | Sell by weight, length, area or volume with decimal quantities. | [MVP] | FRS QNC BAK | 03:PRD-030 |
| PRD-031 | Several units with conversions: buy by carton/fardeau/couronne, stock by piece, sell by piece, sachet, mètre, kg, m², m³, sac, palette, tonne, lot, boîte. | [MVP] | ALL | 03:PRD-031, 02§3.6, 02§3.1, 01§9.1 |
| PRD-032 | Packaging barcodes: scanning the carton adds 24 units. | [MVP] | SUP WHS | 03:PRD-032 |
| PRD-033 | Different price per unit vs per pack. | [MVP] | SUP WHS QNC | 03:PRD-033, 02§3.6, 01§9.1 |
| PRD-034 | Area ↔ box rounding (tiles sold by m², stocked in boxes). | [V1] | QNC | 02§3.6 |

### 17.4 Composites, kits, recipes, production

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-040 | Composite / bundle items with stock derived from components (packs, school kits, gift boxes). | [MVP] | ALL | 03:PRD-040, 01§9.1 |
| PRD-041 | Recipes / bill of materials: ingredients deducted on sale; recipe costing. | [V1] | RST BAK MFG | 03:PRD-041, 03:RST-023, 02§3.2, 02§3.4, 01§9.1 |
| PRD-042 | Production or assembly orders (raw materials → finished goods). | [V1] | BAK MFG | 03:PRD-042, 02§2.1 (POWER PRODUCTION), 01§9.2 |
| PRD-043 | Combos / menus. | [V1] | RST | 03:PRD-043 |
| PRD-044 | Disassembly (unbundling): break a kit or composite item into its components, moving stock and cost. | [V1] | ALL MFG | 01§3.12 (BBS), 01§3.8 (COMMSoft) |
| PRD-045 | Production routings (gammes), work orders and real production cost (materials + labour + overheads) vs standard cost. | [V2+] | MFG | 01§3.8 (COMMSoft), 01§3.5 (Devoro) |
| PRD-046 | Cutting plans (plans de découpe) for sheet or length materials. | [V2+] | MFG QNC | 01§3.8 (COMMSoft) |

### 17.5 Serials, lots, expiry, warranty

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-050 | Serial number / IMEI tracking, captured on receipt and on sale. | [MVP] | TEL INS | 03:PRD-050, 02§3.9, 01§9.2 |
| PRD-051 | Lot / batch tracking. Raised from V1 to MVP: expiry lists are table stakes for local competitors, and décret 13-378 requires lot and DLC/DDM labelling. | [MVP] | PHA COS SUP BAK | 03:PRD-051, 02§1, 04§9, 01§9.2 |
| PRD-052 | Expiry dates (DLC/DDM, sell-by/eat-by) per lot. | [MVP] | PHA COS SUP | 03:PRD-052, 02§1, 04§9, 01§9.2 |
| PRD-053 | FEFO: sales deduct the lot with the nearest expiry first; near-expiry alerts (30 days default, configurable); block or warn on expired items. | [MVP] | PHA COS SUP | 03:PRD-053, 02§3.5, 04§9, 01§9.2 |
| PRD-054 | Warranty per serial (sale date, duration: 6 months new / 3 months used minimum, +30 days per immobilisation); find a serial across all locations. | [V1] | TEL INS AUT | 03:PRD-054, 09§5.1, 01§9.2 |

### 17.6 Prices and costs

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-060 | Cost, sale price, margin and markup auto-calculation; sale price derived from purchase price + margin per family or per item. | [MVP] | ALL | 03:PRD-060, 02§4.2 |
| PRD-061 | Minimum sale price (floor) with a warning below cost. | [MVP] | ALL | 03:PRD-062 |
| PRD-062 | Bulk price update (± % by category, supplier or brand) for price-change days. | [MVP] | ALL | 03:PRD-063 |
| PRD-063 | Price and cost history per product. | [V1] | ALL | 03:PRD-064 |

### 17.7 Catalogue tools

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PRD-070 | Bulk edit in a spreadsheet-like grid. | [MVP] | ALL | 03:PRD-070 |
| PRD-071 | Duplicate a product. | [MVP] | ALL | 03:PRD-071 |
| PRD-072 | Merge duplicate products (after an import). | [V1] | ALL | 03:PRD-075 |
| PRD-073 | Shared catalogue across stores with per-store price and stock overrides. | [V2+] | ALL | 03:PRD-074 |
| PRD-074 | Scale PLU number (1–99999) and scale hotkey mapped per product. | [MVP] | FRS SUP | 05§4.1 |

## 18. Labels and barcodes (LBL)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| LBL-001 | Label designer with presets 20×40 (1-up and 2-up), 30×20, 40×30, 40×60, 45×58, 50×30, 58×40, 50×80, 60×100, 100×100, 100×150 mm, plus custom sizes (beats Almawarid's single 40×30 format). | [MVP] | ALL | 03:DOC-011, 05§2.2, 05§13, 01§9.2 |
| LBL-002 | TSPL output for Xprinter/TSC-class printers, Arabic rendered as `BITMAP`; gap/black-mark, density and speed settings. | [MVP] | ALL | 05§2.3, 05§13 |
| LBL-003 | ZPL output (Zebra, four default formats) and EZPL (Godex); EPL/DPL emulation. | [V1] | ALL | 03:DOC-011, 05§2.3, 05§13 |
| LBL-004 | Barcode symbologies on labels and A4: EAN-13, EAN-8, UPC-A, Code128, Code39, ITF, QR (bwip-js class). | [MVP] | ALL | 05§2.3, 07§7.5, 01§9.2 |
| LBL-005 | Shelf labels, price tags and product stickers in Arabic, French or English, or bilingual AR + FR; displayed price = TTC price the customer pays. | [MVP] | ALL | 03:DOC-011, 04§9, owner decision 2026-09-28, 01§9.4 |
| LBL-006 | Batch printing from received purchase lines, from a count, or from a selection. | [MVP] | ALL | 03:PUR-017, 05§13 |
| LBL-007 | Label reprint queue: every price edit, bulk update, promotion start or end queues the affected labels. | [MVP] | ALL | DZ-PRM-04 |
| LBL-008 | "Was / now" labels for soldes. | [V1] | CLO | DZ-PRM-03 |
| LBL-009 | Internal codes default to EAN-13/Code128 so cheap 1D scanners work; QR only where 2D scanners are used. | [MVP] | ALL | 05§3.1 |
| LBL-010 | Labels per variant (size/colour) and for unbarcoded goods (imported cosmetics, loose épicerie items). | [MVP] | CLO COS SUP | 02§3.8, 02§3.10, 02§1 |
| LBL-011 | Product QR codes (DZBuild product page link). | [V2+] | ALL | 03:PRD-072 |
| LBL-012 | Electronic shelf labels sync (opt). | [V2+] | SUP | 03:PRD-073, 03:HW-016 |
| LBL-013 | A4 label sheets (planches) for laser and inkjet printers: presets (e.g. 3×7, 3×8, 4×10, 5×13) and a start position to reuse partly used sheets. | [MVP] | ALL | 01§9.2 |

## 19. Inventory and warehouses (STK)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| STK-001 | Stock decremented in real time on sale, incremented on return, in the same transaction as the sale. | [MVP] | ALL | 03:INV-001 |
| STK-002 | Stock on hand per location / warehouse. | [MVP] | ALL | 03:INV-002 |
| STK-003 | Several warehouses: shop floor, dépôt, van. | [MVP] | ALL | 03:INV-003, 02§7, 01§9.2 |
| STK-004 | Stock per variant and per lot/serial. | [MVP] | ALL | 03:INV-035 |
| STK-005 | Transfers between locations (send, in transit, receive) printed as bon de transfert. | [MVP] | ALL | 03:INV-005, 04§2.1, 01§9.2 |
| STK-006 | Multi-line transfer and adjustment documents. | [MVP] | ALL | 03:INV-033 |
| STK-007 | Stock adjustments with reason codes: damage, loss, theft, expiry, own use, samples, correction. | [MVP] | ALL | 03:INV-006, 02§4.3, 01§9.2 |
| STK-008 | Internal consumption / own use. | [MVP] | ALL | 03:INV-031 |
| STK-009 | Full and partial (cycle) stock counts. | [MVP] | ALL | 03:INV-007, 01§9.2 |
| STK-010 | Counting by barcode scanner, printable count sheets, blind count (expected qty hidden). | [MVP] | ALL | 03:INV-008, 01§9.2 |
| STK-011 | Count reconciliation: expected vs counted, variance in qty and value, approve and post. | [MVP] | ALL | 03:INV-009 |
| STK-012 | Import counts from data collectors (CSV/TXT `barcode;qty`, POSLUX DT-92 class). | [MVP] | ALL | 05§3.1, 05§13 |
| STK-013 | Low-stock alerts and daily notification. | [MVP] | ALL | 03:INV-012, 01§9.2 |
| STK-014 | Reorder point / min-max per item and location. | [MVP] | ALL | 03:INV-013, 01§9.2 |
| STK-015 | Movement ledger per item: every change with who, when, why and document link (append-only). | [MVP] | ALL | 03:INV-017, 07§5.4, 01§9.2 |
| STK-016 | Costing: weighted average (CMUP/PMP) in MVP; FIFO in V1. | [MVP] | ALL | 03:INV-019, 01§9.2 |
| STK-017 | Negative-stock policy: allow, warn or block, per product or global (soft warning by default). | [MVP] | ALL | 03:INV-021, 07§6.2, 01§9.1 |
| STK-018 | Opening-stock import (per warehouse, lots, serials, costs). | [MVP] | ALL | 03:INV-034, 03:IMP-010 |
| STK-019 | Keep selling during a count (snapshot at count start, sales delta applied). | [V1] | ALL | 03:INV-010 |
| STK-020 | Bin / shelf locations (stock par rayon, emplacement). | [V1] | AUT QNC PHA | 03:INV-004, 02§3.7, 01§9.2 |
| STK-021 | Reorder suggestions and auto-generated draft purchase orders. | [V1] | ALL | 03:INV-014, 01§9.2 |
| STK-022 | Landed costs (freight, customs, fees) allocated to received items. | [V1] | IMP AUT | 03:INV-020, 03:PUR-010, 02§3.7, 01§9.3 |
| STK-023 | Committed / reserved stock (orders, layaways, special orders). | [V1] | ALL | 03:INV-022 |
| STK-024 | Virtual / forecast stock (on hand + incoming − reserved). | [V1] | ALL | 03:INV-023 |
| STK-025 | Back-orders: sell out-of-stock, fulfil later. | [V1] | ALL | 03:INV-028 |
| STK-026 | Waste and breakage tracking; invendus and losses. | [V1] | RST BAK | 03:INV-032, 02§3.2 |
| STK-027 | Legal inventory states: available, reserved, damaged, own use, samples, consignment, in transit (the lawful alternative to "non-billable" stock). | [V1] | ALL | 03:INV-036, 09§5.5 |
| STK-028 | Supplier lead times. | [V1] | ALL | 03:INV-040 |
| STK-029 | Stock value at any past date. | [V1] | ALL | 03:INV-041 |
| STK-030 | Year-end inventory ("livre d'inventaire"): frozen count date, counted vs book qty, CMUP valuation, variance posting, paged numbered print, archived with the fiscal journal. | [V1] | ALL | DZ-STK-01, 09§5.7 |
| STK-031 | Collaborative count from several devices, merged (Android scanner app on the LAN). | [V2+] | ALL | 03:INV-011, 01§9.2 |
| STK-032 | Demand forecasting (Ramadan, Aïd, rentrée seasonality). | [V2+] | ALL | 03:INV-015 |
| STK-033 | Inventory rebalancing across stores. | [V2+] | ALL | 03:INV-016 |
| STK-034 | Pick lists and packing slips. | [V2+] | WHS | 03:INV-030 |
| STK-035 | Channel reserve quantities (hold stock back from online). | [V2+] | ALL | 03:INV-037 |
| STK-036 | Consignment stock (dépôt-vente). | [V2+] | CLO | 03:INV-038, 02§3.8 |
| STK-037 | Inventory period closing with backdating. | [V2+] | ALL | 03:INV-039 |
| STK-038 | Van stock: van as a depot with end-of-route reconciliation. | [V2+] | VAN WHS | DZ-STK-06, 09§4.5, 01§4.4 (PMO Pro) |
| STK-039 | LIFO and last-purchase-price valuation for management reports only; the accounts stay on CMUP/FIFO (SCF follows IAS 2; to verify). | [V2+] | ALL | 01§3.8 (COMMSoft), 01§3.12 (BBS), 01§9.2 |

## 20. Purchasing and suppliers (PUR)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| PUR-001 | Supplier directory: contacts, NIF, NIN, RC, AI, address, RIB, payment terms. | [MVP] | ALL | 03:PUR-001, 04§3.3 |
| PUR-002 | Purchase orders: create, PDF/email/WhatsApp, statuses draft / sent / partial / received. | [MVP] | ALL | 03:PUR-002, 01§12 (7) |
| PUR-003 | Receive (full or partial) against a PO, updating costs (CMUP). | [MVP] | ALL | 03:PUR-005, 01§9.3 |
| PUR-004 | Direct goods receipt without a PO (bon de réception), including scan-based container reception. | [MVP] | ALL | 03:PUR-006, 02§3.8 |
| PUR-005 | Supplier invoices and accounts payable, with the payment method recorded per invoice (for TVA deduction rules). | [MVP] | ALL | 03:PUR-007, 04§3.3, 01§9.3 |
| PUR-006 | Purchases on credit; supplier payments (versements fournisseur, partial) and supplier balance; supplier statement. | [MVP] | ALL | 03:PUR-008, 02§4.1, 01§9.3 |
| PUR-007 | Supplier opening balances. | [MVP] | ALL | 03:IMP-011 |
| PUR-008 | Returns to supplier (bon de retour), including expired goods. | [MVP] | ALL | 03:PUR-009, 02§4.3, 01§9.3 |
| PUR-009 | PO autofill from low stock / reorder suggestions. | [V1] | ALL | 03:PUR-003 |
| PUR-010 | Add PO lines by scan or CSV/XLSX import. | [V1] | ALL | 03:PUR-004, 03:IMP-012 |
| PUR-011 | Supplier price lists; last purchase price per supplier. | [V1] | ALL | 03:PUR-011, 03:PRC-005 |
| PUR-012 | Several suppliers per product; preferred supplier. | [V1] | ALL | 03:PUR-012 |
| PUR-013 | Foreign-currency purchases with an exchange rate. | [V1] | IMP | 03:PUR-016, 01§9.3 |
| PUR-014 | Vendor performance reports. | [V2+] | ALL | 03:PUR-013 |
| PUR-015 | RFQ / alternative RFQs. | [V2+] | WHS | 03:PUR-015 |
| PUR-016 | PO shipment tracking. | [V2+] | IMP | 03:PUR-019 |
| PUR-017 | Electronic supplier invoice/BL import from wholesalers (e.g. UPROMEDIC API for pharmacies): dedup, automatic lot creation. | [V2+] | PHA | 02§3.5 |
| PUR-018 | Internal purchase request (demande d'achat) with approval before the purchase order. | [V2+] | WHS MFG | 01§3.6 (BIG), 01§9.3 |
| PUR-019 | Supplier due dates and overdue-payable alerts (notification centre and dashboard). | [V1] | ALL | 01§4.2 (KERS), 01§9.3 |
| PUR-020 | On goods receipt, propose (or apply by rule) new sale prices per price level from the new cost and each item's margin rule, then queue the labels. | [V1] | ALL | 01§3.12 (Adisoft), 01§9.3 |

---
## 21. Customers and CRM (CRM)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| CRM-001 | Customer database: name, phone (main key in Algeria), email, address with wilaya/commune, legal IDs (NIF, RC, AI, NIS, NIN) for B2B. | [MVP] | ALL | 03:CRM-001, 04§15 (2), 01§9.5 |
| CRM-002 | Purchase history per customer. | [MVP] | ALL | 03:CRM-002, 01§9.5 |
| CRM-003 | Customer notes and preferences. | [MVP] | ALL | 03:CRM-003 |
| CRM-004 | Customer groups, types (particulier, revendeur, garage, entrepreneur, restaurant), tags, segments. | [MVP] | ALL | 03:CRM-004, 02§3.6, 02§3.7, 01§9.5 |
| CRM-005 | Favourite and blacklist flags; blacklisted customers blocked from credit. | [MVP] | ALL | 02§2.1 (المتجر), 02§4.1 |
| CRM-006 | Wholesale identity check: "revendeur" type requires NIF, RC, AI and address, a "vérifié le …" date, and deep links to the CNRC and nifenligne portals (CIDTA 183 ter). | [MVP] | WHS | 04§5.3 |
| CRM-007 | Customer custom fields. | [V1] | ALL | 03:CRM-005 |
| CRM-008 | Duplicate detection and merge. | [V1] | ALL | 03:CRM-014 |
| CRM-009 | Customer analytics: top customers, frequency, RFM. | [V1] | ALL | 03:CRM-012 |
| CRM-010 | Caller ID: open the customer on an incoming call (phone-order restaurants). | [V2+] | RST | 03:CRM-009, 03:HW-019, 01§9.9 |
| CRM-011 | Customer feedback capture. | [V2+] | ALL | 03:CRM-010 |
| CRM-012 | Email / SMS / WhatsApp campaigns through the DZBuild cloud (consent required). | [V2+] | ALL | 03:CRM-011 |
| CRM-013 | Birthday rewards and reminders. | [V2+] | ALL | 03:CRM-013 |
| CRM-014 | Clienteling: profile and recommendations at the counter. | [V2+] | ALL | 03:CRM-015 |
| CRM-015 | Wanted-product requests: log a customer's request for an out-of-stock or unlisted item, notify them by SMS/WhatsApp when it arrives, and feed the list to purchasing. | [V1] | ALL | 01§4.1 (Superette-Grossiste), 01§9.1 |
| CRM-016 | Several addresses per customer (billing, delivery sites) with a default per document type. | [V1] | ALL WHS | 01§9.5 |

## 22. Customer credit (kridi) and receivables (CRD)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| CRD-001 | Sell on account ("à crédit", full or partial) with a per-customer credit ceiling. | [MVP] | ALL | 03:CRD-001, 02§4.1, 01§9.1 |
| CRD-002 | Credit-limit behaviour configurable: warning or hard block; alert before a contractor exceeds the ceiling. | [MVP] | ALL QNC | 03:CRD-002, 02§3.6, 01§9.1 |
| CRD-003 | Opening balances imported from the paper notebook (دفتر الكريدي) for customers. | [MVP] | ALL | 03:IMP-011, 02§4.1 |
| CRD-004 | Versements: partial repayments of any amount at any time, allocated to chosen sales/invoices or oldest first, with a printed receipt (timbre applied on cash). | [MVP] | ALL | 03:CRD-003, 02§4.1, 04§4.4 |
| CRD-005 | Running balance and customer statement of account (relevé, كشف حساب): print, PDF, WhatsApp share. | [MVP] | ALL | 03:CRD-004, 03:DOC-018, 02§4.1, 01§9.5 |
| CRD-006 | Receivables ageing (aged debts) per customer. | [MVP] | ALL | 03:CRD-004, 02§4.1, 01§9.5 |
| CRD-007 | Deposits and advances (acompte, arrhes). | [MVP] | ALL | 03:CRD-005, 01§9.1 |
| CRD-008 | On-account sales count as sales at creation; layaway counts only when fully paid. | [MVP] | ALL | 03:CRD-008 |
| CRD-009 | Debt reminders by WhatsApp or SMS with a statement link or PDF. | [V1] | ALL | 03:CRD-006, 02§4.1, 01§9.1 |
| CRD-010 | Payment terms and due dates (échéances). | [V1] | ALL | 03:CRD-007 |
| CRD-011 | Dunning letters (lettre de relance → mise en demeure) by ageing bucket, printable and WhatsApp-able. | [V1] | WHS ALL | DZ-DOC-02, 01§4.1 (Multisphere) |
| CRD-012 | Automatic credit block on overdue debts (unpaid beyond N days or an ageing bucket), independent of the ceiling, with manager override. | [V1] | ALL WHS | 01§3.12 (BBS), 01§9.1 |

## 23. Instalment sales, vente à tempérament (TMP)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| TMP-001 | Instalment contract generated and printed from the customer file. | [V1] | INS TEL | 03:PAY-025, 02§3.14 |
| TMP-002 | Down payment plus automatic monthly schedule, printable; instalment price separate from cash price. | [V1] | INS | 02§3.14 |
| TMP-003 | One collection ledger for cash, CCP transfer, CCP direct debit (prélèvement) and cheque. | [V1] | INS | 02§3.14 |
| TMP-004 | Client scoring before signing: flags duplicate accounts and past delays. | [V1] | INS | 02§3.14 |
| TMP-005 | Arrears and late-payment dashboard. | [V1] | INS | 02§3.14 |
| TMP-006 | Guarantor data and supporting documents (ID, work certificate, pay slips, CCP number). | [V1] | INS | 02§3.14 |
| TMP-007 | Payment reminders (SMS/WhatsApp) before and after due dates. | [V1] | INS | 02§3.14 |
| TMP-008 | Late-payment penalties and early settlement. | [V2+] | INS | 02§3.14 |
| TMP-009 | Guarantees recorded as neutral documents; no "guarantee cheque" workflow (criminal risk for customers). | [MVP] | INS | 02§3.14, 02§8 |

## 24. Loyalty, gift cards, store credit (LOY)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| LOY-001 | Issue store credit (from returns, exchange vouchers) and redeem it. | [MVP] | ALL | 03:GFT-004 |
| LOY-002 | Points programme: earn per DA, redeem. | [V1] | ALL | 03:LOY-001, 01§9.1 |
| LOY-003 | Loyalty cards with barcode. | [V1] | ALL | 03:LOY-002, 01§9.1 |
| LOY-004 | Rewards: free product, discount, or points converted into a printed voucher (chèque fidélité). | [V1] | ALL | 03:LOY-005, 01§4.2 (SAFE POS) |
| LOY-005 | Sell and redeem physical gift cards / gift vouchers (barcode). | [V1] | ALL SAL | 03:GFT-001, 02§3.16, 01§9.1 |
| LOY-006 | Gift card balance check, reload, expiry. | [V1] | ALL | 03:GFT-003 |
| LOY-007 | Per-product point values or multipliers. | [V2+] | ALL | 03:LOY-003 |
| LOY-008 | Loyalty tiers (silver/gold). | [V2+] | ALL | 03:LOY-004 |
| LOY-009 | Loyalty unified across stores and the DZBuild online store. | [V2+] | ALL | 03:LOY-007 |
| LOY-010 | Digital eGift cards. | [V2+] | ALL | 03:GFT-002 |
| LOY-011 | One customer-card engine for loyalty, credit (kridi account), prepaid consumption and privilege/discount cards, on barcode or 125 kHz RFID cards. | [V1] | ALL | 01§4.2 (TifawtTech, KERS), 01§9.1 |

## 25. Orders, layaway, repairs, service tickets, appointments (ORD)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| ORD-001 | Quotations (devis) converted into orders, delivery notes and invoices in one click. | [MVP] | ALL | 03:LAY-004, 02§3.6, 01§9.4 |
| ORD-002 | Draft orders. | [MVP] | ALL | 03:LAY-005 |
| ORD-003 | Layaway: deposit, stock reserved, instalments, counted as a sale only when fully paid. | [MVP] | ALL | 03:LAY-001 |
| ORD-004 | Special orders: sell out-of-stock, order it, notify the customer; statuses Not ordered → Ordered → Received → Ready for pickup → Delivered; bakery special orders (weddings, custom cakes) with arrhes and pickup date. | [V1] | ALL BAK | 03:LAY-002, 02§3.2 |
| ORD-005 | Work orders / repairs: intake slip (وصل دخول), device state, accessories left, assigned technician, parts from stock, labour, statuses, customer notification, net profit per repair. | [V1] | TEL | 03:LAY-003, 02§3.9, 01§4.4 |
| ORD-006 | Generic service tickets (pressing, repair, tailoring, cordonnerie, optician lab orders, car wash): intake, numbered tag per item (waterproof tag option), promised date, statuses (reçu / en cours / prêt / livré), deposit and balance, "ready" SMS/WhatsApp, uncollected-items report after N days, damage notes/photos. | [V1] | PRS TEL OPT | DZ-STK-04, 09§4.1, 02§3.15, 01§3.12 (INNOVA Clean) |
| ORD-007 | Warranty claims (SAV): log the claim, choose repair / replace / refund, optional joint inspection within 10 days, 30-day execution timer, warranty end date extended by the immobilisation (+30 days). | [V1] | TEL INS AUT | 09§5.1 |
| ORD-008 | Reservations / holds on stock for a customer. | [V1] | ALL | 03:LAY-006 |
| ORD-009 | Appointment booking (agenda RDV) for services. | [V2+] | SAL OPT | 03:LAY-007, 02§3.16 |
| ORD-010 | Goods on loan or on approval (prêt, à condition): stock issued to a customer, then returned or invoiced; list of open loans. | [V2+] | ALL | 01§3.1 (Onyx "loan management"; scope to confirm) |

## 26. Employees and HR lite (EMP)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| EMP-001 | Time clock: clock in/out, hours worked. | [V1] | ALL | 03:EMP-005, 01§9.11 |
| EMP-002 | Commissions per sale, per product or per service (vendeurs, coiffeurs, service providers), with tiered commission scales. | [V1] | ALL WHS SAL | 03:EMP-008, 02§3.12, 02§3.16, 01§4.2 (PME PRO), 01§3.12 (INNOVA) |
| EMP-003 | Employee cash advance from the till. | [V1] | ALL | 03:EMP-014, 01§9.11 |
| EMP-004 | Import clock-in logs from ZKTeco pointeuses (TCP/UDP 4370) into hours and lateness. | [V2+] | ALL | 05§10 |
| EMP-005 | "Salaires & avances" lite: employees, advances, monthly pay record, IRG estimate (2026 barème, CIDTA art. 104), G50 / G50 ter helper. Full payroll stays out of scope. | [V2+] | ALL | DZ-FIN-04, 03:EMP-009, 09§5.11, 01§9.11 |
| EMP-006 | Scheduling and time-off. | [V2+] | ALL | 03:EMP-006, 01§9.11 |
| EMP-007 | Hourly workload analysis. | [V2+] | ALL | 03:EMP-010 |
| EMP-008 | Staff tasks. | [V2+] | ALL | 03:EMP-013 |

## 27. Cash management and register closing (CSH)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| CSH-001 | Cash in / cash out (pay in, pay out) with reason and note, shown in the closing report. | [MVP] | ALL | 03:CSH-002 |
| CSH-002 | Expense payouts from the till by category (rent, electricity, wages, transport, supplies). | [MVP] | ALL | 03:CSH-015, 02§4.4 |
| CSH-003 | Expected vs counted cash, with the variance (écart) shown. | [MVP] | ALL | 03:CSH-003, 02§4.4, 01§9.1 |
| CSH-004 | Blind close: expected cash hidden from staff. | [MVP] | ALL | 03:CSH-004 |
| CSH-005 | Acceptable-difference threshold, forced recount, manager sign-off. | [MVP] | ALL | 03:CSH-005 |
| CSH-006 | Denomination counting helper (notes 200/500/1000/2000 DA, coins 5–200 DA); accepts a banknote-counter breakdown. | [MVP] | ALL | 03:CSH-006, 05§12, 01§9.1 |
| CSH-007 | X report (mid-shift, no reset) and Z report (end of day, sequential, non-editable, "conforme"). | [MVP] | ALL | 03:CSH-007, 02§4.4, 04§5.6, 01§9.1 |
| CSH-008 | Reconciliation per payment method (cash, card vs TPE batch, cheque, transfer, QR, credit). | [MVP] | ALL | 03:CSH-008, 05§11 |
| CSH-009 | Cash control per day, month and terminal. | [MVP] | ALL | 03:CSH-014 |
| CSH-010 | Timbre collected per session, reconciled with cash tender totals. | [MVP] | ALL | 04§4.5 |
| CSH-011 | Z close triggers a verified backup and a WAL checkpoint. | [MVP] | ALL | 07§5.2, 07§5.5 |
| CSH-012 | Blind cash drops / safe drops during a shift. | [V1] | ALL | 03:CSH-009 |
| CSH-013 | Drawer must be closed before the next sale (drawer sensor read through `DLE EOT`). | [V1] | ALL | 03:CSH-010, 05§5, 05§13 |
| CSH-014 | Shift report pushed to the owner (WhatsApp, email, Telegram or the DZBuild dashboard) when online. | [V1] | ALL | 03:CSH-011, 02§3.5, 02§4.8 |
| CSH-015 | Cash vs treasury comparison and seller audits. | [V1] | ALL PHA | 02§3.5 (Silwane) |
| CSH-016 | Close a drawer remotely from the back office. | [V2+] | ALL | 03:CSH-013 |
| CSH-017 | Several drawers per register; one drawer per cashier. | [V2+] | ALL | 03:CSH-017 |

## 28. Treasury, cheques, expenses, accounting export, zakat (FIN)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| FIN-001 | Expense logging by category (rent, electricity, transport, wages), from the till or the back office. | [MVP] | ALL | 03:PUR-018, 02§4.4 |
| FIN-002 | Other income entries (revenus divers). | [V1] | ALL | 02§2.1 (Factor Facile) |
| FIN-003 | Treasury accounts: several tills, a safe (coffre), banks and CCP accounts with balances and transfers between them (till to till, till to safe, till to bank). | [V1] | ALL | 09§3.2, 09§3.1, 01§4.1 (PRO SOLUTION), 01§4.2 (TifawtTech), 01§9.6 |
| FIN-004 | Bank deposit tracking (cash taken from till to bank). | [V1] | ALL | 03:CSH-016 |
| FIN-005 | Cheque and traite (bill of exchange) portfolio: received/issued, due date, bank, status (en portefeuille, remis, encaissé, impayé), deposit slips (bordereau de remise), due-date alerts. | [V1] | WHS ALL | DZ-FIN-01, 01§9.6 |
| FIN-006 | Accountant export: sales and purchase journals, VAT ledger, CSV/XLSX, mapped to SCF accounts (defaults to verify: 700, 600, 30, 411, 401, 4457, 4456, 53, 512, 709/609). | [V1] | ALL | 03:IMP-018, 03:ADM-010, DZ-FIN-03, 01§9.6 |
| FIN-007 | Zakat report: stock (chosen valuation) + cash + collectable receivables − debts due, compared with the official nisab (1447 AH: 2,295,000 DA, editable data pack); 2.5 % after a lunar year; not presented as a fatwa. | [V1] | ALL | DZ-FIN-05, 09§5.6, 02§4.4, 01§9.7 |
| FIN-008 | Bank-statement import for reconciliation (CSV/XLSX first; AFB/CFONB fixed-width if Algerian banks export it). | [V2+] | ALL | DZ-FIN-02 |
| FIN-009 | Exchange-rate gain/loss on foreign-currency supplier payments. | [V2+] | IMP | 01§4.1 (ProPOS), 01§3.8 (COMMSoft), 01§9.3 |
| FIN-010 | Manual bank reconciliation (pointage) of bank and CCP account entries against the statement. | [V1] | ALL | 01§3.7 (SmartCom), 01§4.1 (SOFTEL), 01§9.6 |
| FIN-011 | Fixed-asset register (investissements) with straight-line depreciation feeding net profit. | [V2+] | ALL | 01§4.2 (TifawtTech), 01§9.6 |
| FIN-012 | Cheque printing on Algerian bank cheque forms: amount in letters in Arabic or French as the bank form requires, payee, date, place; per-bank layouts with calibration. | [V1] | ALL | 01§3.11 (Winalco Smart Check) |

---
## 29. Restaurant and hospitality (RST)

Enabled by the restaurant/café module toggle (PLT-016). Shares the catalogue, stock, cash and reports with retail.

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| RST-001 | Floor plans: rooms, drag-and-drop tables, shapes, seats. | [MVP] | RST | 03:RST-001, 02§3.4, 01§9.10 |
| RST-002 | Live table status: free, occupied, bill printed, time since order. | [MVP] | RST | 03:RST-002 |
| RST-003 | Open tickets per table; predefined tickets. | [MVP] | RST | 03:RST-003 |
| RST-004 | Transfer an order between tables; merge tables. | [MVP] | RST | 03:RST-004 |
| RST-005 | Split the bill by item, by seat, or evenly (covers). | [MVP] | RST | 03:RST-005, 01§9.10 |
| RST-006 | Kitchen and bar tickets routed by category to section printers (grill, pizza, bar), with cancellation tickets; buzzer kitchen printers supported. | [MVP] | RST | 03:RST-009, 03:DOC-009, 02§3.4, 05§1.6, 01§9.1 |
| RST-007 | Pre-bill (addition) before payment. | [MVP] | RST | 03:DOC-010 |
| RST-008 | Void / comp / gift with a reason. | [MVP] | RST | 03:RST-020 |
| RST-009 | Order number printed on takeaway tickets. | [MVP] | RST | 02§3.4 |
| RST-010 | Merge split checks back. | [V1] | RST | 03:RST-006 |
| RST-011 | Seat numbers and guest counts. | [V1] | RST | 03:RST-007 |
| RST-012 | Courses and course firing (hold / fire). | [V1] | RST | 03:RST-008 |
| RST-013 | Kitchen display (KDS) on a second PC or Android tablet on the LAN: timers, colour by station, routing, bump, recall. | [V1] | RST | 03:RST-010, 03:HW-012, 01§9.10 |
| RST-014 | Separate kitchen and bar preparation displays. | [V1] | RST | 03:RST-028 |
| RST-015 | 86 items: out-of-stock toggle synced to every till and screen; auto-86 from stock. | [V1] | RST | 03:RST-011 |
| RST-016 | Delivery management: phone-order customer lookup by number and address, driver assignment, delivery fee, status, cash collected by the driver. | [V1] | RST | 03:RST-014, 02§3.4 |
| RST-017 | Aggregator orders: manual Yassir entry screen (menu mapping, commission %, courier status); API integration only if a partner API appears. | [V1] | RST | 03:RST-015, DZ-INT-01, 09§6.6 |
| RST-018 | Waiter handheld ordering (Android app on the LAN sends the order straight to the kitchen). Local vendors market it (Smart Resto, Instagram ads). | [V1] | RST | 03:RST-021, 02§3.4, 01§9.10 |
| RST-019 | Staff meals recorded against stock. | [V1] | RST | 02§3.4 |
| RST-020 | Bar tabs (no card pre-authorisation). | [V2+] | RST | 03:RST-012 |
| RST-021 | QR self-ordering at the table or a kiosk, served from the LAN. | [V2+] | RST | 03:RST-016, 01§9.9 |
| RST-022 | Table reservations. | [V2+] | RST | 03:RST-017, 01§9.9 |
| RST-023 | Allergens shown on menus and kitchen tickets. | [V2+] | RST | 03:RST-018 |
| RST-024 | Pizza builder / half-and-half. | [V2+] | RST | 03:RST-022, 01§9.10 |
| RST-025 | Event → action automation engine (rules). | [V2+] | RST | 03:RST-026 |
| RST-026 | Order-ready screen, pager or order-number call (fast-food). | [V2+] | RST | 03:RST-027, 01§9.10 |
| RST-027 | Digital menu by QR code: read-only menu page with photos and prices in AR/FR/EN, served on the LAN or through the DZBuild store. | [V1] | RST | 01§4.3 (El Eulma, Menu Max), 01§9.9 |

Also used by restaurants: modifiers (PRD-021/022/023), combos (PRD-043, SAL-026), recipes and costing (PRD-041), happy hour (PRC-008), service price lists (PRC-009), eat-in/takeaway tax (TAX-012), dining options (SAL-044), tips (PAY-015).

## 30. Vertical packs (VRT)

Each pack is a template (ONB-008) that switches modules on and adds the rows below. Features shared with other modules are referenced, not repeated.

### 30.1 Pack-specific features

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| VRT-001 | Superette template: regulated staples preloaded with ceilings (oil 1 L 125 / 2 L 250 / 5 L 650 DA; sugar 90/95 DA/kg; milk sachet 25 DA/L; semolina table), quick keys for unbarcoded fast movers, department keys. | [MVP] | SUP | 09§5.3, 02§3.1 |
| VRT-002 | Flexy counter: record mobile top-ups (Mobilis, Djezzy, Ooredoo) as a service line with a float balance; operator auto-detection; offers. | [V1] | SUP TEL KSK | 02§3.1, 02§3.9, 01§4.4 |
| VRT-003 | Flexy sending through USSD (USB GSM modem) or an API. | [V1] | TEL KSK | 02§3.9, DZ-INT-02 |
| VRT-004 | Digital and scratch cards (Idoom, recharge cards) with PIN log and stock. | [V1] | TEL KSK | 02§3.9, 09§4.5 |
| VRT-005 | Bill-payment and operator services counter (Mobilis bill payment, Ooredoo Maxy and similar) recorded as service lines with commission. | [V2+] | TEL KSK | 01§4.4 (Easy Life Flexy) |
| VRT-010 | Price per kg and tare on the customer display; decimal weights. | [MVP] | FRS | 02§3.3 |
| VRT-011 | Carcass purchase → cutting yield (achat carcasse → découpe, yield per cut). | [V2+] | FRS | 02§3.3, 09§4.2 |
| VRT-020 | Daily production vs sales: planned vs produced vs sold vs invendus, to reduce waste. | [V1] | BAK | 02§3.2 |
| VRT-021 | Regulated bread price flag (current text unverified; data pack). | [V1] | BAK | 02§3.2, 09§5.3 |
| VRT-030 | Parapharmacie fully covered in MVP through lots, expiry, FEFO, promotions and loyalty. | [MVP] | PHA | 02§3.5 |
| VRT-031 | Officine pack: sales regimes Comptoir, Chifa, CASNOS, Crédit, Instance, Retours. | [V2+] | PHA | 02§3.5 |
| VRT-032 | Chifa: CNAS-format file generation, catalogue sync, rejection tracking; CNAS/CASNOS bordereaux (closure, submission, verification); assisted Chifa entry by barcode or price; Chifa card reader. | [V2+] | PHA | 02§3.5, 05§10, 01§4.4 (sCom_pharm) |
| VRT-033 | Medicine search by DCI, laboratory, registration number; national nomenclature of approved medicines. | [V2+] | PHA | 02§3.5, 01§4.4 (sCom_pharm) |
| VRT-034 | Prescriptions (ordonnances), patient history, interactions, renewal alerts. | [V2+] | PHA | 02§3.5, 01§4.4 (sCom_pharm) |
| VRT-035 | Psychotropes / stupéfiants register; lot-level and psychotrope user privileges. | [V2+] | PHA | 02§3.5 |
| VRT-036 | Order assistant (shortage prediction); laboratory margins, discounts and UG. | [V2+] | PHA | 02§3.5 |
| VRT-037 | Regulated medicine margins (PPA/SHP prices, wholesaler and pharmacist margins) and TVA exemptions (CTCA art. 41 per vendor). | [V2+] | PHA | 02§3.5, 01§3.8 (COMMSoft) |
| VRT-038 | Stock-state transmission to the ministry platform (pharmaceutical wholesalers and importers). | [V2+] | PHA WHS | 04§14 |
| VRT-039 | Officinal and magistral preparations: formula, ingredients deducted from stock, preparation register, price. | [V2+] | PHA | 01§4.4 (sCom_pharm) |
| VRT-040 | Chantier tracking: devis, BL, invoices and returns of unused materials grouped per job site and contractor. | [V1] | QNC | 02§3.6 |
| VRT-041 | Truck delivery with pallet loading; pallet deposit (consigne). | [V1] | QNC | 02§3.6 |
| VRT-042 | Paint tint formulas. | [V2+] | QNC | 02§3.6 |
| VRT-050 | Search by OEM reference or brand / model / year; user-maintained equivalence groups and vehicle compatibility. | [V1] | AUT | 02§3.7 |
| VRT-051 | Cross-reference catalogue import (TecDoc-like; commercial licence costly) (opt). | [V2+] | AUT | 02§3.7 |
| VRT-052 | Used / second-hand parts; core (consigne) returns. | [V2+] | AUT | 02§3.7 |
| VRT-060 | Stock-by-variant grid (size × colour) for viewing, counting and receiving. | [MVP] | CLO | 02§3.8 |
| VRT-070 | IMEI verification at intake and sale. | [V1] | TEL | 02§3.9 |
| VRT-071 | Second-hand device buyback: condition, IMEI, buyback price, seller ID; resale as "occasion". | [V1] | TEL | 02§3.9 |
| VRT-080 | Tester write-off reason and brand-level sales reporting. | [V1] | COS | 02§3.10 |
| VRT-090 | School-list packs per level for the rentrée, built from the official supply list. | [V1] | LIB | 02§3.11 |
| VRT-091 | Per-page services (photocopy, printing) with counters. | [V1] | LIB KSK | 02§3.11, 09§4.5 |
| VRT-092 | ISBN field; returns to publishers; school and administration invoices on credit. | [V1] | LIB | 02§3.11 |
| VRT-100 | Delivery rounds (tournées): route, BL per stop, collection of versements on the route. | [V1] | WHS VAN | 02§3.12, 01§9.5 |
| VRT-101 | Sales-team order taking on Android phones (offline pre-sale), synced to the POS. | [V2+] | WHS VAN | 02§3.12, 09§4.5, 01§9.5 |
| VRT-102 | GPS position of sales reps and delivery drivers, and electronic proof of delivery (signature or photo). | [V2+] | WHS VAN | 01§3.5 (Devoro), 01§3.9 (GestiumPRO), 01§9.5 |
| VRT-103 | Allocation quotas per customer for scarce or subsidised products (pharma distribution, staples). | [V2+] | WHS PHA | 01§3.8 (COMMSoft) |
| VRT-110 | Optical patient file with prescription (OD/OG sphère, cylindre, axe, addition). | [V2+] | OPT | 02§3.13 |
| VRT-111 | Frames, lenses and articles stock; devis → commande → facture; lab orders with status; deposit and balance at pickup. | [V2+] | OPT | 02§3.13 |
| VRT-112 | CNAS convention dossier tracking (80 % coverage, insured pays 20 %). | [V2+] | OPT | 02§3.13 |
| VRT-120 | Service price grid per garment type and service (lavage, repassage, nettoyage à sec). | [V1] | PRS | 02§3.15, 09§4.1 |
| VRT-130 | Services and products on one ticket; packages and subscriptions for salons. | [V1] | SAL | 02§3.16 |
| VRT-140 | Jewellery piece attributes: weight (g, 2–3 decimals), titre (e.g. 750‰ = 18k), stones; unique piece tag and label. | [V1] | JEW | 09§4.3 |
| VRT-141 | Gold-price board: daily price per gram per titre; automatic repricing of every piece (sale price = weight × price + façon). | [V1] | JEW | DZ-PRM-05, 09§4.3 |
| VRT-142 | Old-gold buyback and exchange of broken jewellery against cash or new pieces (weight, titre, loss %, seller ID). | [V1] | JEW | 09§4.3 |
| VRT-143 | Legal jeweller client register (CII art. 359, LF 2026 art. 62): nature, number, weight, titre, price, buyer identity and address; paged uneditable print; ≥ 5-year retention. | [V1] | JEW | DZ-STK-03, 09§5.4 |
| VRT-144 | Jewellery activity authorisation number and AML fields in settings. | [V1] | JEW | DZ-CMP-04 |
| VRT-145 | Melting batches (fonte): input pieces → metal weight out, loss. | [V2+] | JEW | DZ-STK-02 |
| VRT-150 | Memberships: members with photo; plans (monthly, per session, N-visit cards); expiry alerts; freeze; renewals. Reusable for private schools, hammams, parking. | [V2+] | GYM | DZ-STK-07, 09§4.4 |
| VRT-151 | Entry/exit log with access control (ZKTeco fingerprint/face/RFID over TCP 4370, turnstile relay). | [V2+] | GYM | DZ-INT-05, 09§7.5 |
| VRT-152 | Product sales (supplements, drinks) and member debt on the same account. | [V2+] | GYM | 09§4.4 |
| VRT-160 | Newspaper returns (invendus) to the distributor. | [V2+] | KSK | 09§4.5 |
| VRT-170 | Returnable containers: container types with deposit amounts, container ledger per customer (empties balance), deposit line on invoices. | [V1] | CNS | DZ-STK-05, DZ-CHK-06, 04§2.1, 01§9.2 |
| VRT-180 | Landed-cost calculator for importers: FOB/CIF → customs value, customs duties, DAPS (30–200 %), import TVA, domiciliation tax (0.5 %, min 20,000 DA), précompte IBS 2 %, transit and transport, feeding CMUP. | [V1] | IMP | 04§13 |
| VRT-181 | Micro-importer (auto-entrepreneur) IFU paid at customs per import (customs value + duties + 30 % flat margin). | [V2+] | IMP | 04§5.4 |
| VRT-190 | Veterinary / agri-input shops: lots and expiry for regulated products (low priority). | [V2+] | — | 09§4.5 |

### 30.2 Pack composition (what each template switches on)

| Pack | Default modules and key features | MVP? |
|---|---|---|
| Superette / épicerie | VRT-001, SAL-014, SAL-019, SAL-030, PRC-012, PRD-051–053, CRD-*, PRM-011, LBL-010 | MVP |
| Boucherie / fruits & légumes | SAL-014, SAL-015, VRT-010, PRD-074, HW scale rows (§33.4), CRD-* | MVP (scale serial V1) |
| Boulangerie / pâtisserie | PRD-041/042, ORD-004, VRT-020, STK-026, SAL-010 | MVP core, production V1 |
| Restaurant / café / fast-food | §29 (incl. RST-027 QR menu), PRD-021–023, PRD-043, TAX-012 | MVP core, KDS V1 |
| Parapharmacie | VRT-030, PRD-051–053, PRM-017, LOY-* | MVP |
| Pharmacie (officine + Chifa) | VRT-031–039, PRD-051–053, CSH-015 | V2+ |
| Quincaillerie / matériaux | PRD-011, PRD-031–034, PRC-003, CRD-002, ORD-001, VRT-040/041, DOC-023 | MVP core, chantier V1 |
| Pièces auto | PRD-011, STK-020, PRC-002, VRT-050–052, STK-022, PRD-054 | MVP core, equivalences V1 |
| Vêtements / chaussures | PRD-020, SAL-023, VRT-060, RET-004, PRM-021–023, LBL-008/010 | MVP |
| Téléphonie / réparation / flexy | PRD-050, SAL-025, DOC-012, ORD-005, ORD-007, VRT-002–004, VRT-070/071, SAL-032 | MVP core, repairs V1 |
| Cosmétiques | PRD-020, PRD-051–053, LBL-010, VRT-080 | MVP |
| Librairie / papeterie | VRT-090–092, PRD-040 | V1 |
| Grossiste / distribution | PRC-001–004, CRM-006, DOC-009/010, DOC-030/031, STK-003, VRT-100–103, FIN-005, CRD-011/012, RPT-085 | MVP core, tournées V1 |
| Vente à tempérament | §23, PRD-050, DOC-012 | V1 |
| Optique | VRT-110–112, ORD-006 | V2+ |
| Pressing / services | ORD-006, VRT-120 | V1 |
| Salon | VRT-130, ORD-009, EMP-002, LOY-005 | V1 |
| Bijouterie | VRT-140–145, SAL-034, PAY-024, TMP-* | V1 |
| Salle de sport / abonnements | VRT-150–152, SAL-035 | V2+ |
| Kiosque / tabac / multiservices | SAL-019, VRT-002–005, VRT-091, VRT-160, TAX-011 | MVP core, flexy V1 |
| Consignes (gaz, boissons) | VRT-170, SAL-033 | V1 |
| Importateur | VRT-180, PUR-013, STK-022 | V1 |
| Vente ambulante | STK-038, VRT-100/101 | V2+ |
| Atelier de production | PRD-041/042, PRD-044–046, PUR-018 | V1 |

---
## 31. Omnichannel, delivery, e-commerce (OMN)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| OMN-001 | Sales orders taken at the POS and fulfilled later. | [V1] | ALL | 03:OMN-001 |
| OMN-002 | Local delivery with delivery notes and a delivery status. | [V1] | ALL | 03:OMN-004 |
| OMN-003 | Invoices with payment follow-up (online payment link when available). | [V1] | ALL | 03:OMN-010 |
| OMN-004 | Click and collect / pickup in store (DZBuild Pickup-from-Store orders). | [V2+] | ALL | 03:OMN-002, 06§1.4 |
| OMN-005 | Ship from store / buy in store, ship home. | [V2+] | ALL | 03:OMN-003 |
| OMN-006 | Marketplace integrations. | [V2+] | ALL | 03:OMN-006 |
| OMN-007 | Courier integration with labels and tracking: Yalidine family, ZR Express (Procolis), Ecotrack family, Maystro, Noest, through DZBuild's free dzship API (works without a DZBuild store). | [V2+] | ALL | 03:OMN-007, 04§12.2, 06§1.4, 01§9.9 |
| OMN-008 | COD workflow: shipment creation → label (100×150) → tracking → status updates → COD remittance reconciliation (net of courier fees) → returns re-stocked; per-wilaya fees, home vs stop-desk. | [V2+] | ALL | 04§12.2 |
| OMN-009 | E-commerce law (loi 18-05) support: invoice for every online sale, never validate an out-of-stock online order (real-time stock), return within 4 working days and refund within 15 days tracked. | [V2+] | ALL | 04§12.1 |
| OMN-010 | Unified returns across channels. | [V2+] | ALL | 03:OMN-008 |

## 32. DZBuild integration (DZB)

The POS works fully without DZBuild (PLT-002, DAT-001). Linking adds catalogue publishing, the online-orders inbox and a combined dashboard (the cloud backup shipped in 0.7 was removed in 1.0.0, free edition).

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| DZB-001 | "Connecter ma boutique en ligne" wizard: system browser (never an embedded WebView), OAuth 2.0 authorization code + PKCE S256, loopback redirect on `127.0.0.1` with an ephemeral port, one-shot listener with a 10-minute timeout, localized success page; states up front that only the store owner can authorise. | [V1] | ALL | 03:API-008, 06§5.3, 06§5.4, 07§12 |
| DZB-002 | Track A (works with today's platform): a DZBuild-run "POS Link" broker holds the client secret and install tokens server-side, issues short-lived device tokens with rotating refresh tokens, relays API calls, and receives webhooks for the desktop to pull (long-poll or SSE). | [V1] | ALL | 06§5.2 |
| DZB-003 | Track B (target): first-party public client on DZBuild's authorization server (RFC 8414 metadata, loopback with any port, PKCE S256 only, refresh rotation with reuse detection, revocation endpoint, device authorization grant so the owner approves a till from the DZBuild Android app, optional DPoP). | [V2+] | ALL | 06§5.3, 06§6.1 |
| DZB-004 | Token handling: refresh token only in Windows Credential Manager (or DPAPI file), access token in memory only; no token ever in the DB, logs, crash dumps, exports or backups. | [V1] | ALL | 06§5.4, 07§12 |
| DZB-005 | Proactive refresh at 80 % of lifetime with a single-flight mutex; on `invalid_grant` show "Reconnexion requise" and keep selling offline. | [V1] | ALL | 06§5.4 |
| DZB-006 | Unlink: revoke, delete the credential, keep local data (optionally purge mapping tables); handle `app.uninstalled` / `link.revoked` by stopping sync with a banner. | [V1] | ALL | 06§5.4, 06§5.6 |
| DZB-007 | Multi-store mapping: one consent covers up to 10 stores; each shop (point de vente) maps to one DZBuild store. | [V1] | ALL | 06§5.4 |
| DZB-008 | Robust client: native HTTP only (no CORS), TLS 1.2+, `User-Agent` and `X-Request-Id`, clock-skew correction from the server `Date` / `ping`, deterministic `Idempotency-Key` per outbox item, `Retry-After` backoff, ~100 of 120 req/min reserved for sync, error mapping (401 re-link, 403 plan upgrade, 402 WhatsApp wallet, 409 stock mode, 422 key reuse). | [V1] | ALL | 06§3.1.5, 06§5.4 |
| DZB-009 | Offline-first sync tables (`dz_link`, `dz_map`, `outbox`, `inbox_events`) with hash-based skipping of no-op updates. | [V1] | ALL | 06§5.5 |
| DZB-010 | Automatic background sync when online, a manual "Synchroniser" button and a clear last-synced status. | [V1] | ALL | 03:OFF-003, 03:OFF-004 |
| DZB-011 | Store profile pulled on link and daily (name, logo, language, colours, `public_url`) for the receipt header and the "Commandez en ligne" QR. | [V1] | ALL | 06§5.5 |
| DZB-012 | Catalogue publishing: per-product "Publier en ligne" toggle; categories, variants, prices (optional online price list), images uploaded in the background (10/min); plan cap shown (Pro = 300 active products), extra products pushed as drafts; first-link import from DZBuild. | [V1] | ALL | 03:OMN-005, 03:API-006, 06§1.3, 06§5.5, 01§9.9 |
| DZB-013 | Stock sync: the POS is the source of truth for physical stock; real-time deltas coalesced per 30 s plus a nightly reconcile; buffer rule (never expose the last N units online); echo suppression for online-order movements; stock-deduction mode asked at link time. | [V1] | ALL | 06§5.5 |
| DZB-014 | Online-orders inbox: polling every 1–2 min plus relayed events; pick, print label, confirm; status updates (confirmed → processing → shipped), send to delivery, cancel; a till claims an order to avoid double preparation. | [V1] | ALL | 06§5.5, 06§6.2 |
| DZB-015 | Customer lookup by phone at the till, showing DZBuild `fraud_score` / `is_banned` before a credit sale. | [V1] | ALL | 06§5.5 |
| DZB-016 | Buyer WhatsApp notifications (`shipped`, `desk_ready`) sent from the POS (one wallet credit each). | [V1] | ALL | 06§3.1.8 |
| DZB-017 | Combined "Boutique + En ligne" dashboard from DZBuild analytics. | [V1] | ALL | 06§5.5 |
| DZB-018 | Shipping reference data (wilayas, communes, rates) cached for offline use. | [V1] | ALL | 06§5.5 |
| DZB-019 | Growth loop: "Créez votre boutique en ligne" call to action for unlinked users (UTM `dzbuild-pos`). | [V1] | ALL | 06§5.7 |
| DZB-020 | 2FA enforced on the DZBuild account used for linking. | [V1] | ALL | 03:SEC-011 |
| DZB-021 | In-store sales and Z reports uploaded to DZBuild (needs the proposed `/v1/pos/sales` and `/v1/pos/closures`). | [V2+] | ALL | 06§6.2 |
| DZB-022 | Owner's live sales and multi-store dashboard on the DZBuild web and mobile app. | [V2+] | ALL | 03:RPT-023, 01§9.9 |
| DZB-023 | Device registry: linked tills with name, version, last seen; revocable from a "Caisse / POS" page in the DZBuild dashboard. | [V2+] | ALL | 06§6.2, 06§6.4 |
| DZB-024 | "Se connecter avec DZBuild" identity (existing OIDC provider). | [V2+] | ALL | 06§3.3 |
| DZB-025 | Launch from the DZBuild app directory: `dz_launch` link opens the POS (`dzbuild-pos://`) or offers the installer. | [V2+] | ALL | 06§3.1.7, 06§6.4 |
| DZB-026 | Copilot / MCP read-only POS tools ("combien j'ai vendu en boutique aujourd'hui ?"). | [V2+] | ALL | 06§6.4 |
| DZB-027 | Opt-in, identity-less usage events. | [V2+] | ALL | 06§5.7 |
| DZB-028 | Google Sheets export when linked. | [V2+] | ALL | 09§8 |

**Dependencies on DZBuild (proposed in 06 §6, none exist today):** `updated_since` on products and orders; an event feed (long-poll/SSE); batch product upsert with `external_id`; `barcode` on variant options and combinations; batch inventory adjustments with `reason=pos_sale` kept out of the undo history; inventory levels; locations; `pos/sales`, refunds and closures; pre-signed media uploads; `stock_deduction`, `timezone`, `currency` and `plan_limits` on `GET /v1/store`; backup endpoints; device endpoints; order claim; new events (`product.*`, `inventory.level_changed`, `order.updated`, `customer.*`, `store.plan_changed`, `pos.sale.*`, `device.revoked`, `link.revoked`); new scopes (`offline_access`, `inventory:*`, `pos:sales:*`, `locations:*`, `backups:*`, `devices:self`, `events:read`); an exemption from the "no 'dzbuild' in app names" rule for the first-party POS.

## 33. Hardware (HW)

### 33.1 Receipt and document printers

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| HW-001 | ESC/POS thermal printers, 58 mm (384 dots) and 80 mm (576 dots), plus 76 mm impact kitchen printers, over Windows spooler RAW, TCP 9100, COM, Bluetooth SPP and the USB printer class. | [MVP] | ALL | 03:HW-001, 05§1.3, 07§7.1 |
| HW-002 | Windows-driver (GDI) printing fallback so "driver-only" cheap printers still work. | [MVP] | ALL | 03:HW-002, 05§1.2 |
| HW-003 | A4/A5 laser and inkjet printers for invoices and quotes. | [MVP] | ALL | 03:HW-003 |
| HW-004 | Network (LAN/Wi-Fi) kitchen printers shared by several tills. | [MVP] | RST | 03:HW-004, 05§9 |
| HW-005 | Arabic receipts rendered to a 1-bit raster at printer width and sent as `GS v 0` bands with configurable band height; fast text mode for Latin-only receipts (CP858/WPC1252). The user picks "Texte" or "Image (arabe)" per printer. | [MVP] | ALL | 05§1.5, 07§7.2 |
| HW-006 | Per-printer profiles (width, dots/line, `GS v 0` vs `ESC *`, cut command, drawer pin, code-page map, density, max band) with presets for Xprinter, Rongta, Smart POS, POSLUX, Epson, Sunmi; `FS .` sent to leave Chinese mode. | [MVP] | ALL | 05§1.4, 05§1.5 |
| HW-007 | Logo, QR code, EAN-13/Code128 barcodes, cut and beep on receipts. | [MVP] | ALL | 05§1.4, 05§13 |
| HW-008 | Printer status polling (paper out, cover open, offline) via `DLE EOT`. | [MVP] | ALL | 05§1.4, 05§13 |
| HW-009 | "Printer doctor": detects a vendor driver replaced by Windows Update and prints a test page. | [MVP] | ALL | 05§1.6 |
| HW-010 | Automatic Status Back events, NV logo, Bluetooth receipt printers. | [V1] | ALL | 05§13 |
| HW-011 | Print-head maintenance reminder. | [V1] | ALL | 05§1.6 |
| HW-012 | Dot-matrix printers (Epson LX-300/LX-350/LQ-680/LQ-2090) for multi-copy forms; Arabic through driver graphics mode, or a laser printer recommended for Arabic documents. | [V1] | WHS ALL | DZ-DOC-01, 09§7.1 |

### 33.2 Label printers

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| HW-013 | Label printers (Xprinter XP-365B/233B/420B, Smart POS S58, Godex, Zebra, TSC) driven raw through the spooler (see §18 for the formats). | [MVP] | ALL | 03:HW-009, 05§2.1 |

### 33.3 Scanners and data collectors

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| HW-020 | Barcode scanners as HID keyboard wedge or serial/COM, 1D and 2D. | [MVP] | ALL | 03:HW-005, 05§3.1 |
| HW-021 | Layout-proof decoding: physical key codes, bursts under ~50 ms, Enter/Tab/CR-LF suffix, so AZERTY and Arabic Windows layouts don't corrupt scans; per-device layout setting checked with a test barcode. | [MVP] | ALL | 05§3.2 |
| HW-022 | Global scan capture on the sale screen (hidden buffer): a scan never lands in a quantity or price field. | [MVP] | ALL | 05§3.2 |
| HW-023 | USB-COM (CDC) scanner mode and HID POS scanner mode (Windows.Devices.PointOfService). | [V1] | ALL | 05§3.2 |
| HW-024 | Handheld Android devices / PDAs for inventory, as a LAN companion app. | [V2+] | ALL | 03:HW-013, 05§3.1 |

### 33.4 Scales

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| HW-030 | Label-printing scale barcodes (Rongta RLS1000/1100, ACLAS LS6, Dibal D900): configurable EAN-13 20–29 parser (weight, price or count; PLU length 4/5/6; optional internal check digit; implied decimals; check-digit validation; qty = price / unit price with discrepancy flag). | [MVP] | FRS SUP BAK | 03:HW-010, 05§4.2 |
| HW-031 | PLU export (CSV/XLSX) in the layouts of the Rongta, ACLAS and Dibal PC tools. | [V1] | FRS SUP | 05§4.1 |
| HW-032 | Direct PLU push to scales: Rongta TCP/IP, ACLAS SDK (DLL sidecar), Dibal. | [V2+] | FRS SUP | 05§4.1, 01§3.12 (INNOVA) |
| HW-033 | Serial checkout scales with pluggable protocols: Mettler Toledo 8217 (9600 7E1), CAS ECR (ENQ/ACK/DC1), Adam, SMA, continuous-output regex; auto-baud probing; CH340/PL2303 virtual COM. | [V1] | FRS SUP BAK | 03:HW-008, 05§4.3 |
| HW-034 | Counting and price-computing scales on serial (CAS PR-C, Preciamolen class). | [V2+] | FRS QNC | 09§7.6 |

### 33.5 Cash drawer, displays, terminals

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| HW-040 | Cash drawer kicked through the printer's RJ11/RJ12 port (`ESC p` pin 2, pin 5 fallback; real-time `DLE DC4`), plus a manual button (logged). | [MVP] | ALL | 03:HW-006, 05§5, 07§7.1, 01§9.1 |
| HW-041 | USB drawer adapter (virtual COM: write a byte or toggle DTR). | [MVP] | ALL | 05§5 |
| HW-042 | Second-monitor customer display: borderless full-screen window with cart, total, change, QR (BaridPay, DZBuild store), full Arabic. | [MVP] | ALL | 03:HW-007, 05§6.2 |
| HW-043 | VFD pole display 2×20 (ESC/POS, CD5220, eMAX/Aedex, UTC, ADM787/788, DSP-800 dialects), Latin text and Western digits. | [MVP] | ALL | 03:HW-007, 05§6.1 |
| HW-044 | Ads / slideshow playlist on the customer display. | [V1] | ALL | 05§13 |
| HW-045 | All-in-one touch terminals from 1024×768 (Smart POS, POSLUX, HFPOS, ENIGMA, SMARTPOS A3). | [MVP] | ALL | 03:HW-015, 05§6.3, 09§7.6 |
| HW-046 | Hardware bridge built into the desktop app (no separate IoT box). | [MVP] | ALL | 03:HW-018 |
| HW-047 | Runs on low-end hardware: 4 GB RAM, 3rd/4th-gen Core i3/i5, Celeron J1900, HDD (see NFR-001). | [MVP] | ALL | 03:HW-022, 05§6.3 |

### 33.6 Other peripherals

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| HW-050 | USB GSM modem (Huawei E3531/K3772) as an offline SMS/USSD gateway through AT commands (`AT+CMGF`, `AT+CMGS`, `AT+CUSD`), with HiLink mode switching per model. | [V1] | ALL KSK TEL | DZ-INT-02, 09§7.3 |
| HW-051 | Caller-ID modem. | [V2+] | RST | 03:HW-019 |
| HW-052 | Price-checker kiosk. | [V2+] | SUP | 03:HW-021 |
| HW-053 | Self-order kiosks and pagers (no Algerian listing found). | [V2+] | RST | 09§7.6 |

Covered elsewhere: RFID/badge and fingerprint login (USR-007/008), ZKTeco pointeuses (EMP-004), gym access (VRT-151), Chifa card reader (VRT-032), TPE (PAY-002/003), CCTV overlay (AUD-013), UPS (DAT-019), banknote counters (CSH-006), data collectors (STK-012), electronic shelf labels (LBL-012).

## 34. Import / export engine (IMX)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| IMX-001 | Import from CSV. | [MVP] | ALL | 03:IMP-001, 01§9.2 |
| IMX-002 | Import from Excel (.xlsx, .xls, .ods). | [MVP] | ALL | 03:IMP-002, 02§3.7, 01§9.2 |
| IMX-003 | Import modes: add/update (upsert by SKU or barcode) or replace all. | [MVP] | ALL | 03:IMP-003 |
| IMX-004 | Undo the last import. | [MVP] | ALL | 03:IMP-004 |
| IMX-005 | Downloadable templates for every importable entity (products, variants, customers, suppliers, opening stock, prices, barcodes, balances), with headers in AR, FR or EN. | [MVP] | ALL | 03:IMP-005, 07§8 |
| IMX-006 | Column-mapping wizard with AR/FR/EN header auto-detection, preview, per-row validation errors, dry run, commit in one transaction. | [MVP] | ALL | 03:IMP-006, 07§8 |
| IMX-007 | Large imports (target 100k rows). | [MVP] | ALL | 03:IMP-007 |
| IMX-008 | Import customers. | [MVP] | ALL | 03:IMP-008, 03:CRM-007 |
| IMX-009 | Import suppliers. | [MVP] | ALL | 03:IMP-009 |
| IMX-010 | Export items. | [MVP] | ALL | 03:IMP-014, 01§9.2 |
| IMX-011 | Export customers. | [MVP] | ALL | 03:IMP-015 |
| IMX-012 | Export receipts and sales lines. | [MVP] | ALL | 03:IMP-016 |
| IMX-013 | Every list and report exports to XLSX, CSV and PDF. | [MVP] | ALL | 03:IMP-017, 03:RPT-017, 07§20, 01§9.7 |
| IMX-014 | Encoding and locale safety: UTF-8 with BOM for Excel, `;` separator for French locale, CP1256/CP1252 legacy files, Arabic text, leading zeros in barcodes kept. | [MVP] | ALL | 03:IMP-019, 07§8 |
| IMX-015 | Full data export in open formats ("vos données vous appartiennent"). | [MVP] | ALL | 03:IMP-021 |
| IMX-016 | JSON export/import of settings and of the full dataset. | [V1] | ALL | 03:ADM-004, 04§6.1 |
| IMX-017 | "Import from my old software" wizard: HFSQL/WinDev files (ODBC driver route, or direct `.fic/.ndx/.mmo` reader), DBF, and column-mapped XLSX/CSV exports of any competitor. | [V1] | ALL | 03:IMP-013, 09§8, 01§7, 01§12 (3) |
| IMX-018 | Bulk image import (folder or ZIP matched by SKU/barcode). | [V1] | ALL | 03:IMP-020 |
| IMX-019 | MS Access (.mdb/.accdb) import, only if field research shows demand. | [V2+] | ALL | 09§8 |
| IMX-020 | Scheduled auto-export to a folder. | [V2+] | ALL | 03:IMP-022 |
| IMX-021 | Export documents and reports to Word (.docx) and HTML. | [V2+] | ALL | 01§3.8 (COMMSoft), 01§4.1 (Quick FACT), 01§9.7 |
| IMX-022 | Import presets for the standard article, customer and supplier exports of Sage 100, EBP, Ciel, PC Compta, Odoo and Dolibarr. | [V1] | ALL | 01§4.5 (Almawarid migration offer), 01§12 (3) |

## 35. APIs, messaging, third-party integrations (API)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| API-001 | Messaging: WhatsApp share links (offline-friendly `wa.me` + PDF), SMS through the GSM modem or DZBuild cloud, Telegram/WhatsApp owner alerts for sales and stock when online. | [V1] | ALL | 03:API-009, 02§4.8 |
| API-002 | Local REST API plus the DZBuild cloud API. | [V2+] | ALL | 03:API-001, 01§9.9 |
| API-003 | Webhooks. | [V2+] | ALL | 03:API-002 |
| API-004 | Scripting / automation engine. | [V2+] | ALL | 03:API-003 |
| API-005 | Accounting-software integrations. | [V2+] | ALL | 03:API-005 |

## 36. AI and emerging features (AI)

| ID | Feature | Rel | Verticals | Src |
|---|---|---|---|---|
| AI-001 | Order-mode presets (dine-in, takeaway, delivery, wholesale) that set price list, taxes and printers in one tap. | [V1] | ALL RST | 03:AI-010 |
| AI-002 | Rule-based anomaly and fraud flags offline (unusual voids, discounts, refunds, no-sales per cashier); AI-based detection later. | [V1] | ALL | 03:AI-007, 01§9.7 |
| AI-003 | AI assistant with natural-language questions ("what sold best last Ramadan?"), via the DZBuild cloud when online. | [V2+] | ALL | 03:AI-001, 02§2.1, 01§9.7 |
| AI-004 | Voice commands. | [V2+] | ALL | 03:AI-002 |
| AI-005 | Generated content (descriptions, photos, menus). | [V2+] | ALL | 03:AI-003 |
| AI-006 | AI pricing suggestions. | [V2+] | ALL | 03:AI-004 |
| AI-007 | Predictive inventory: demand forecasting and stock-out prediction. | [V2+] | ALL | 03:AI-005, 02§2.1, 01§9.7 |
| AI-008 | Self-checkout / kiosk. | [V2+] | SUP RST | 03:AI-008, 01§9.9 |
| AI-009 | Supplier-invoice capture from a photo ("facture from photo"): French offline with the built-in Windows OCR; Arabic through Tesseract (ara+fra) or DZBuild cloud AI. | [V2+] | ALL | 03:AI-009, 02§4.8, 09§10.3, 01§9.3, 01§1 (11) |
| AI-010 | Product-photo background removal (offline model or DZBuild cloud). | [V2+] | ALL | 01§4.2 (KERS), 01§9.2 |
| AI-011 | Plain-language summary of the financial situation (daily or weekly) in AR/FR/EN: rule-based text offline, AI-written through the cloud. | [V2+] | ALL | 01§4.2 (KERS), 01§9.7 |

---
## 37. Reports and états catalogue (RPT)

Every report opens on screen, prints, and exports to **PDF, XLSX and CSV** (IMX-013) unless the *Formats* column says otherwise, in Arabic, French or English (UX-043). Every report has period, register, user, warehouse and category filters where they make sense. History is unlimited (PLT-003).

### 37.1 Sales and dashboard

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-001 | Home dashboard: today's sales, net profit, money owed by customers, returns, expenses, trend, peak hour, best products and cashier, average basket, alerts. | screen | [MVP] | ALL | 03:RPT-001, 01§4.2 (KERS) |
| RPT-002 | Sales summary by period (day, week, month, year, custom). | all | [MVP] | ALL | 03:RPT-002 |
| RPT-003 | Sales by item, category, variant and brand. | all | [MVP] | ALL | 03:RPT-003, 02§3.10 |
| RPT-004 | Sales by customer. | all | [MVP] | ALL | 03:RPT-004 |
| RPT-005 | Sales by employee, cashier and vendeur. | all | [MVP] | ALL | 03:RPT-005, 03:EMP-007, 02§7, 01§9.7 |
| RPT-006 | Sales by payment method. | all | [MVP] | ALL | 03:RPT-006 |
| RPT-007 | Period comparison (vs previous period or year; up to 4 years side by side). | all | [MVP] | ALL | 03:RPT-019, 01§3.8 (COMMSoft) |
| RPT-008 | Top sellers and slow movers (راكد). | all | [MVP] | ALL | 02§3.8, 02§7, 01§9.2 |
| RPT-009 | Sales by channel (comptoir, WhatsApp, Facebook, DZBuild, Yassir) with commissions. | all | [V1] | ALL | DZ-CHK-03 |
| RPT-010 | Sales by hour and weekday (heatmap). | screen, PDF | [V1] | ALL | 03:RPT-007, 01§9.7 |
| RPT-011 | Average basket, items per basket, conversion. | all | [V1] | ALL | 03:RPT-020, 01§9.7 |
| RPT-084 | Sales target (daily or monthly) with a progress bar on the dashboard. | screen | [V1] | ALL | 01§4.2 (KERS), 01§9.7 |
| RPT-085 | Sales by wilaya and commune (customer geography). | all | [V1] | ALL WHS | 01§4.2 (PME PRO), 01§4.1 (SOFTEL) |

### 37.2 Profit

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-012 | Profit, margin and COGS by period, product and family. | all | [MVP] | ALL | 03:RPT-008, 02§7, 01§9.7 |
| RPT-013 | Real-time profit (instant, hourly, daily). | screen | [MVP] | ALL | 02§2.1 |
| RPT-014 | Simple P&L: sales − COGS − expenses = net profit after expenses. Raised from V1 because every local competitor sells it. | all | [MVP] | ALL | 03:RPT-027, 02§1, 02§4.4, 01§9.6 |

### 37.3 Discounts, returns, loss prevention, audit

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-015 | Discounts by reason and by cashier. | all | [MVP] | ALL | 03:RPT-009, 03:DSC-017 |
| RPT-016 | Returns and refund rate. | all | [MVP] | ALL | 03:RPT-011 |
| RPT-017 | Voids, cancellations and no-sale drawer openings. | all | [MVP] | ALL | 03:RPT-012 |
| RPT-018 | Loss-prevention report per cashier (voids, no-sales, refunds, discounts, price overrides). | all | [MVP] | ALL | 03:AUD-004 |
| RPT-019 | Audit log report and hash-chain integrity verification result. | screen, PDF, CSV | [MVP] | ALL | 03:AUD-001, 07§11, 01§9.7 |
| RPT-020 | Personal-data access log. | screen, CSV | [MVP] | ALL | 04§7 |
| RPT-021 | Price and cost change history. | all | [V1] | ALL | 03:PRD-064, 04§6.1 |
| RPT-022 | Backup history and status. | screen | [MVP] | ALL | 07§5.5 |

### 37.4 Inventory

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-023 | Stock on hand per location, variant, lot and serial. | all | [MVP] | ALL | 03:RPT-013 |
| RPT-024 | Stock valuation: cost value, retail value, potential profit. | all | [MVP] | ALL | 03:INV-018 |
| RPT-025 | Low-stock and out-of-stock list. | all | [MVP] | ALL | 02§2.1 |
| RPT-026 | Expiry list: expired and near-expiry lots (منتهية الصلاحية). | all | [MVP] | PHA COS SUP | 02§1, 02§2.1 |
| RPT-027 | Stock movements ledger (per item, per document, per reason). | all | [MVP] | ALL | 03:INV-017 |
| RPT-028 | Count variance report (qty and value). | all | [MVP] | ALL | 03:INV-009 |
| RPT-029 | Inventory ageing / dead stock. | all | [V1] | ALL | 03:INV-024, 01§9.2 |
| RPT-030 | ABC analysis. | all | [V1] | ALL | 03:INV-025 |
| RPT-031 | Sell-through. | all | [V1] | ALL | 03:INV-026 |
| RPT-032 | Stock value at a past date. | all | [V1] | ALL | 03:INV-041 |
| RPT-033 | GMROI and turnover. | all | [V2+] | ALL | 03:INV-027, 01§9.7 |
| RPT-083 | Stock balance between two dates per item (opening quantity, entries, exits, closing quantity and value). | all | [V1] | ALL | 01§9.2 |

### 37.5 Purchases, suppliers, customers, credit

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-034 | Purchases by supplier, product and period. | all | [MVP] | ALL | 03:RPT-014 |
| RPT-035 | Supplier balances and payables ageing; supplier statement. | all | [MVP] | ALL | 03:RPT-015, 02§4.1 |
| RPT-036 | Receivables: customer balances, ageing buckets, credit sales, versements. | all | [MVP] | ALL | 03:RPT-015, 02§4.1 |
| RPT-037 | Customer statement of account (relevé, كشف حساب). | print, PDF, WhatsApp | [MVP] | ALL | 03:DOC-018, 02§4.1 |
| RPT-038 | Instalment arrears and collection forecast. | all | [V1] | INS | 02§3.14 |
| RPT-039 | Gift card and store-credit liability. | all | [V1] | ALL | 03:GFT-005 |
| RPT-040 | Loyalty points liability. | all | [V1] | ALL | 03:LOY-001 |
| RPT-041 | Customer analytics (top customers, frequency, RFM). | all | [V1] | ALL | 03:CRM-012 |

### 37.6 Cash

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-042 | X report and Z report (per register and business day, perpetual totals), and the Z archive. | print 58/80 mm, PDF | [MVP] | ALL | 03:RPT-016, 03:CSH-007 |
| RPT-043 | Cash movements report (pay in/out, drops, expenses), filterable. | all | [MVP] | ALL | 03:CSH-012 |
| RPT-044 | Daily report by payment type and register. | all | [MVP] | ALL | 02§7, 02§4.4 |
| RPT-045 | Expenses by category. | all | [MVP] | ALL | 02§4.4 |
| RPT-046 | Monthly timbre statement (by day, register and document), reconciled with cash tenders. | all | [MVP] | ALL | 04§4.5 |

### 37.7 Tax and fiscal états

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-047 | Tax report by rate and period: HT per rate, TVA collected, exempt turnover. | all | [MVP] | ALL | 03:RPT-010, 03:TAX-006, 04§3.1 |
| RPT-048 | G50 helper (réel): CA HT at 9 % and 19 %, exempt CA, TVA collected and deductible, timbre "sur état", IRG salaires input. | PDF, XLSX | [MVP] | ALL | 04§3.4, 04§5.6, 01§9.7 |
| RPT-049 | G9 état des fournisseurs (NIN, NIF, name, address, RC, invoice date and reference, TTC, TVA deducted). | XLSX, CSV, PDF | [MVP] | ALL | 04§3.3 |
| RPT-050 | CA TTC daily, monthly, annual (IFU / AE), with margin for regulated products. | all | [MVP] | ALL | 04§5.1 |
| RPT-051 | Registre des ventes / livre journal (paged, cumulative, côté-paraphé style). | print, PDF | [MVP] | ALL | 04§5.1, 04§5.2, 01§9.7 |
| RPT-052 | Registre des achats. | print, PDF | [MVP] | ALL | 04§5.1, 01§9.7 |
| RPT-053 | État clients annuel for wholesale (name, NIF, RC, AI, address, annual HT, TVA); local products sell an "état 104" that is probably this statement (to verify). | XLSX, CSV, PDF | [MVP] | WHS | 04§5.3, 01§9.7 |
| RPT-054 | Soldes / promotion / liquidation declaration (designation, qty, old price, new price, %). | XLSX, PDF | [MVP] | CLO ALL | DZ-PRM-02, 09§5.2 |
| RPT-055 | État spécial art. 18 CTCA (cancelled or unpaid operations for TVA regularisation). | XLSX, PDF | [V1] | ALL | 04§2.6 |
| RPT-056 | G12 / G12 bis helper and IFU threshold tracker. | PDF, XLSX | [V1] | ALL | 04§5.1 |
| RPT-057 | Livre d'inventaire (year-end inventory, paged and numbered). | print, PDF, XLSX | [V1] | ALL | DZ-STK-01 |
| RPT-058 | Jeweller client register (CII 359). | print (uneditable), PDF | [V1] | JEW | DZ-STK-03 |
| RPT-059 | Zakat statement. | print, PDF | [V1] | ALL | DZ-FIN-05 |
| RPT-060 | Warranty certificates (reprint) and warranty-claims log. | print, PDF | [V1] | TEL INS | 09§5.1 |
| RPT-061 | Fiscal archive export (documents, journal, hash chain, manifest). | ZIP (CSV/JSON + PDF) | [MVP] | ALL | 04§6.1 |

### 37.8 Staff, restaurant and vertical reports

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-062 | Commissions per employee. | all | [V1] | ALL | 03:EMP-008 |
| RPT-063 | Hours worked and lateness (time clock). | all | [V1] | ALL | 03:EMP-005 |
| RPT-064 | Hourly workload. | all | [V2+] | ALL | 03:EMP-010 |
| RPT-065 | Sales by table, server and order type. | all | [V1] | RST | 02§3.4 |
| RPT-066 | Recipe / food cost. | all | [V1] | RST BAK | 02§3.4 |
| RPT-067 | Preparation-time report. | all | [V2+] | RST | 03:RPT-021 |
| RPT-068 | Repairs: open tickets, profit per repair, technician performance. | all | [V1] | TEL | 02§3.9 |
| RPT-069 | Uncollected service tickets (after N days). | all | [V1] | PRS | 09§4.1 |
| RPT-070 | Production vs sales and invendus. | all | [V1] | BAK | 02§3.2 |
| RPT-071 | Jewellery: sales, exchanges, melting, credit. | all | [V1] | JEW | 09§4.3 |
| RPT-072 | Chantier statement (per job site and contractor). | all | [V1] | QNC | 02§3.6 |
| RPT-073 | Delivery round (tournée) statement. | all | [V1] | WHS | 02§3.12 |
| RPT-074 | Flexy and digital-card sales and float. | all | [V1] | TEL KSK | 02§3.9 |
| RPT-075 | Container balances per customer. | all | [V1] | CNS | 09§4.5 |
| RPT-076 | Membership expiry, renewals and visits. | all | [V2+] | GYM | 09§4.4 |
| RPT-077 | Chifa bordereaux and rejections. | all | [V2+] | PHA | 02§3.5 |
| RPT-078 | COD remittance reconciliation per courier. | all | [V2+] | ALL | 04§12.2 |

### 37.9 Platform-level reporting

| ID | Report / état | Formats | Rel | Verticals | Src |
|---|---|---|---|---|---|
| RPT-079 | Breadth target: at least 50 built-in reports at MVP. | — | [MVP] | ALL | 03:RPT-024, 01§9.7 |
| RPT-080 | Custom report builder. | all | [V2+] | ALL | 03:RPT-018 |
| RPT-081 | Scheduled / emailed reports. | PDF, XLSX | [V2+] | ALL | 03:RPT-025 |
| RPT-082 | Peer benchmarks (anonymised, opt-in, via DZBuild). | screen | [V2+] | ALL | 03:RPT-026 |

Multi-store consolidation (LAN-016, from 03:RPT-022) and the owner's live mobile view (DZB-022, from 03:RPT-023) are covered in their modules.

---

## 38. Import/export matrix (entity × format)

**Legend:** **I** = import, **E** = export, **I/E** = both. The release tag follows. "—" = not offered. *Backup file* = the encrypted full-database backup (DAT-006), which contains every entity and restores everything at once. *Competitor formats* = the "import from my old software" wizard (IMX-017): HFSQL/WinDev `.fic/.ndx/.mmo` (ODBC or direct reader), DBF, MS Access (V2+), and any competitor Excel/CSV through column mapping. Scanned paper notebooks are entered through the opening-balance templates. Import templates and export headers exist in AR, FR and EN (IMX-005, UX-043). Word/HTML export is V2+ (IMX-021); import presets for Sage, EBP, Ciel, PC Compta, Odoo and Dolibarr exports are V1 (IMX-022).

| Entity | Excel .xlsx | CSV | PDF | JSON | Backup file | Competitor formats |
|---|---|---|---|---|---|---|
| Products (variants, barcodes, units, prices, attributes) | I/E [MVP] | I/E [MVP] | E [MVP] (catalogue, price list) | I/E [V1] | I/E [MVP] | I [V1] (HFSQL, DBF, mapped Excel/CSV) |
| Categories / families | I/E [MVP] | I/E [MVP] | E [MVP] | I/E [V1] | I/E [MVP] | I [V1] |
| Price lists / tariffs | I/E [MVP] | I/E [MVP] | E [MVP] | I/E [V1] | I/E [MVP] | I [V1] |
| Product images | I [V1] (ZIP/folder by SKU) | — | — | — | I/E [MVP] | — |
| Customers | I/E [MVP] | I/E [MVP] | E [MVP] | I/E [V1] | I/E [MVP] | I [V1] |
| Suppliers | I/E [MVP] | I/E [MVP] | E [MVP] | I/E [V1] | I/E [MVP] | I [V1] |
| Opening stock (per warehouse, lots, serials, cost) | I [MVP] | I [MVP] | — | I [V1] | I/E [MVP] | I [V1] |
| Customer opening balances (kridi) | I [MVP] | I [MVP] | — | I [V1] | I/E [MVP] | I [V1] |
| Supplier opening balances | I [MVP] | I [MVP] | — | I [V1] | I/E [MVP] | I [V1] |
| Stock counts | I/E [MVP] | I/E [MVP] (data-collector TXT `barcode;qty`) | E [MVP] (count sheets) | — | I/E [MVP] | — |
| Stock movements / transfers | E [MVP] | E [MVP] | E [MVP] (bon de transfert) | E [V1] | I/E [MVP] | — |
| Sales, receipts, sales lines | E [MVP] | E [MVP] | E [MVP] (receipts) | E [V1] | I/E [MVP] | — |
| Invoices, avoirs, devis, proformas, BL, BC | E [MVP] (lists) | E [MVP] | E [MVP] (each document) | E [V1] | I/E [MVP] | — |
| Purchase orders (lines) | I [V1] / E [MVP] | I [V1] / E [MVP] | E [MVP] | E [V1] | I/E [MVP] | — |
| Receptions and supplier invoices | E [MVP] | E [MVP] | E [MVP] | E [V1] | I/E [MVP] | I [V2+] (pharma wholesaler e-invoices) |
| Payments, versements, cheques and traites | E [MVP] | E [MVP] | E [MVP] (receipts) | E [V1] | I/E [MVP] | — |
| Cash sessions, X/Z reports, cash movements | E [MVP] | E [MVP] | E [MVP] | E [V1] | I/E [MVP] | — |
| Expenses | I [V1] / E [MVP] | I [V1] / E [MVP] | E [MVP] | E [V1] | I/E [MVP] | — |
| Users and employees (no passwords) | E [MVP] | E [MVP] | — | E [V1] | I/E [MVP] | — |
| Loyalty, gift cards, store credit | I [V1] / E [V1] | I [V1] / E [V1] | E [V1] | E [V1] | I/E [MVP] | — |
| Instalment contracts and schedules | E [V1] | E [V1] | E [V1] (contract, schedule) | E [V1] | I/E [MVP] | — |
| Service tickets and repairs | E [V1] | E [V1] | E [V1] (intake slips) | E [V1] | I/E [MVP] | — |
| Promotions and soldes declarations | E [MVP] | E [MVP] | E [MVP] | — | I/E [MVP] | — |
| All reports (§37) | E [MVP] | E [MVP] | E [MVP] | — | — | — |
| Fiscal états (G50 helper, G9, état clients, état spécial, registres, timbre, G12) | E [MVP–V1] | E [MVP–V1] | E [MVP–V1] | — | — | — |
| Accountant journals (SCF mapping) | E [V1] | E [V1] | E [V1] | — | — | — |
| Fiscal archive (documents + hash-chained journal + manifest) | — | E [MVP] | E [MVP] | E [MVP] | — | — |
| Audit log and PII access log | — | E [MVP] | E [MVP] | E [V1] | I/E [MVP] | — |
| Warranty certificates, jeweller register, livre d'inventaire, zakat statement | E [V1] | — | E [MVP–V1] | — | I/E [MVP] | — |
| Scale PLU file (Rongta, ACLAS, Dibal) | E [V1] | E [V1] | — | — | — | E [V2+] (direct push) |
| Labels | — | — | E [MVP] (print/PDF) | — | — | — |
| Bank statements | I [V2+] | I [V2+] (AFB/CFONB fixed-width if available) | — | — | — | — |
| Settings / configuration | — | — | E [V1] (printout) | I/E [V1] | I/E [MVP] | — |
| Full database | — | — | — | E [MVP] (open-format data export) | I/E [MVP] | — |
| Diagnostic bundle | — | — | — | E [MVP] (ZIP) | — | — |
| DZBuild catalogue and orders | — | — | — | I/E [V1] (API sync) | — | — |
| Google Sheets | E [V2+] (when linked) | — | — | — | — | — |

---

## 39. Non-functional requirements (NFR)

| ID | Requirement | Rel | Src |
|---|---|---|---|
| NFR-001 | Reference hardware floor: Celeron J1900 or 3rd/4th-gen i3/i5, 4 GB RAM, HDD or small SSD, 1024×768 touch screen, Windows 10 22H2. Measured on a reference PC every release. | [MVP] | 03:HW-022, 05§6.3, 07§16 |
| NFR-002 | Online installer ≤ 15 MB. | [MVP] | 07§16 |
| NFR-003 | Cold start to a usable sale screen ≤ 3 s on the reference PC; SQLite open < 50 ms. | [MVP] | 07§16 |
| NFR-004 | Idle RAM ≤ 250 MB across all processes (hard ceiling 300–500 MB); WebView2 memory set to LOW when minimised. | [MVP] | 07§16, 05§6.3 |
| NFR-005 | Scan → line added ≤ 50 ms. | [MVP] | 07§16 |
| NFR-006 | Sale commit including fsync ≤ 30 ms (SSD) / ≤ 80 ms (HDD). | [MVP] | 07§16 |
| NFR-007 | Receipt to paper ≤ 1.5 s for an Arabic raster receipt (header/logo bitmap cached). | [MVP] | 07§16 |
| NFR-008 | Product search < 30 ms on 50k products in the core; < 100 ms end-to-end on 100k. | [MVP] | 07§15, 03:UX-007 |
| NFR-009 | Five years of a busy shop fits under ~2 GB (estimate to verify). | [MVP] | 07§16 |
| NFR-010 | POS route JS ≤ 300 kB gzipped; back office code-split; every list virtualised; tables paged from Rust. | [MVP] | 07§9 |
| NFR-011 | Money is integer centimes (i64); quantities are integer milli-units; never floats. | [MVP] | 07§5.4 |
| NFR-012 | UUIDv7 primary keys; human document numbers per series and per till. | [MVP] | 07§5.4 |
| NFR-013 | Append-only ledgers (stock, cash, customer, supplier); balances derived and rebuildable. | [MVP] | 07§5.4 |
| NFR-014 | Timestamps in UTC; business date computed in Africa/Algiers. | [MVP] | 07§5.4, 09§10.2 |
| NFR-015 | Full-text search (FTS5) over Arabic, French and English names, barcodes and references, using the UX-032 normalisation. | [MVP] | 07§5.4 |
| NFR-016 | One SQLite transaction per write touching stock or money; single writer thread. | [MVP] | 07§5.2 |
| NFR-017 | No SQL in the webview (no `tauri-plugin-sql`); typed IPC through tauri-specta; least-privilege capabilities; strict CSP; no remote content, no `eval`; Tauri isolation pattern evaluated. | [MVP] | 07§4, 07§11 |
| NFR-018 | Every command returns `Result`; no `unwrap()`/`expect()` in command paths; parameterised SQL only. | [MVP] | 07§20 |
| NFR-019 | Forward-only migrations, automatic backup before migrating, tested against fixture DBs from every past version. | [MVP] | 07§5.1, 07§15 |
| NFR-020 | Every user-facing string goes through i18n with three equal catalogues (ar, fr, en); logical CSS properties only (lint rule); RTL and LTR visual snapshots. | [MVP] | 07§15, 07§20, owner decision 2026-09-28 |
| NFR-021 | Business logic lives in Tauri-free crates (`pos-core`, `pos-hw`, `pos-docs`, `pos-server`) so the shell is replaceable. | [MVP] | 07§4, 07§18 (R12) |
| NFR-022 | Release hardening: `lto = "fat"`, `codegen-units = 1`, `strip`, assets embedded; `cargo deny`/`cargo audit`, `npm audit`, committed lockfiles, Renovate, reproducible CI builds. | [MVP] | 07§11 |
| NFR-023 | Test strategy: proptest on money/tax/rounding/stock invariants, insta snapshots of receipts, ESC/POS byte streams and XLSX, virtual printer, golden PNGs for Arabic raster receipts and for receipts, invoices and labels in AR, FR, EN and bilingual layouts, cargo-fuzz on importers, crash tests killing the process mid-transaction, Vitest with `mockIPC`, WebdriverIO E2E on Windows, ar/fr/en visual snapshots, amount-in-letters tests per language, manual hardware-lab smoke test per release. | [MVP] | 07§15 |
| NFR-024 | Updater private key backed up offline in two places with a documented rotation plan. | [MVP] | 07§11, 07§18 (R10) |
| NFR-025 | Printing stays in the user session; a Windows service never prints A4 through a webview. | [MVP] | 07§17 |
| NFR-026 | Certificate pinning (if used) pins a public key with a backup pin, never a leaf certificate. | [V1] | 06§5.4 |
| NFR-027 | Translation completeness gate: CI and `pnpm i18n` fail on any missing, empty or untranslated key in ar, fr or en (UI, document and label templates, reports, messages, help); ICU plural rules per language (Arabic has six forms); one reviewed AR/FR/EN glossary of business and fiscal terms (facture / فاتورة / invoice, avoir, timbre, BL…) that keeps the official French and Arabic names where English has no legal equivalent; pseudo-localisation run; a release is blocked until all three languages are complete. | [MVP] | owner decision 2026-09-28, 07§15 |

---

## 40. Not applicable and prohibited features (EXC)

### 40.1 Not applicable in Algeria (from 03, 22 rows) and other exclusions

| Src | Feature | Why excluded |
|---|---|---|
| 03:CHK-037 | Send the cart to an online checkout | No such flow in local practice |
| 03:PAY-008 | Mobile wallets / NFC (Apple Pay, Google Pay) | Not available locally |
| 03:PAY-009 | BNPL (Afterpay, Shop Pay Installments) | Replaced by instalments (§23) and layaway |
| 03:PAY-020 | Offline card store-and-forward | Cards go through standalone bank TPEs |
| 03:PAY-021 | Automated cash machines (Cashdro, Glory) | No local market |
| 03:PUR-014 | Supplier catalogue integration (NuORDER) | Not used in Algeria |
| 03:PUR-020 | EDI with suppliers | Not used by Algerian SMEs |
| 03:INV-029 | Drop-shipping | Out of scope for a shop POS |
| 03:EMP-009 | Full payroll | Replaced by the "salaires & avances" lite module (EMP-005) |
| 03:CSH-018 | Tip pooling | Low cultural relevance |
| 03:RPT-028 | Franchise royalties | Out of scope |
| 03:RST-025 | Auto-gratuity for large parties | Low cultural relevance |
| 03:OMN-009 | Virtual shopping / social selling | Out of scope |
| 03:OFF-006 | Offline card limits and expiry windows | No integrated card acceptance |
| 03:OFF-010 | Wired-network fallback for card readers | Not relevant |
| 03:MST-006 | Franchise mode | Out of scope |
| 03:HW-017 | Cash recyclers, automatic coin dispensers (Onyx, 01§3.1) | No local market |
| 03:SEC-006 | Login captcha | Desktop app, local PIN login |
| 03:SEC-007 | PCI-DSS scope | Never store card data (design rule) |
| 03:API-004 | App marketplace | Not planned |
| 03:AI-006 | Computer-vision shelf counts | Out of scope |
| 03:AUD-006 | CCTV overlay | Marked n/a in 03, **re-included** as AUD-013 [V2+] after 09 found the Hikvision POS overlay |
| — | Peppol UBL / e-MECeF e-invoicing | Other countries' formats (09§8) |
| — | Fuel-station pump controllers | Out of scope (09§4.5) |
| — | Hotel, clinic, school management suites | Out of scope; only the subscription engine is reused (VRT-150) |
| 01§3.2–§3.6, §3.12 | Enterprise ERP modules sold by Megasoft, Devoro, BIG, SAMA VISION and Onyx: GMAO, GPAO/MRP/MES, TMS/WMS, fleet, full HR, general-ledger accounting, hospital, education, public procurement, project management, SAP integration, AI supply-chain agent | Out of scope for a shop POS; accounting data leaves through the accountant export (FIN-006) |
| 01§3.12 (Tayssir) | Cross-docking | Out of scope for a shop POS (drop-shipping is already excluded as 03:INV-029) |
| 01§6 | Licence keys, machine-bound activation, per-PC/per-user/per-phone fees, time-limited keys, USB-key licences, online licence checks | Contrary to PLT-002 (free forever, no activation) |
| 01§4.1 ("Magasin & Stock 2026"), 01§4.5 (FlustockX) | macOS and Linux desktop builds | Windows-only product (PLT-001); Tauri would allow them later if demand appears |

### 40.2 Prohibited by design (legal or security reasons)

| Rule | Reason | Src |
|---|---|---|
| No dual stock ("stock facturable / non facturable", "real vs virtual stock", hidden non-billable stock), no fictive, "virtual" or "non comptabilisées" (non-accounted) invoices, no invoice deletion | CTCA art. 118 and CIDTA art. 193 treat sales without invoice and fictive entries as fraud; CIDTA art. 303 (LF 2026 art. 25) raises penalties to 5–10 years and 5–10 M DA when committed by IT means, and accomplices incur the same penalties; CPF 51 sexies lets the tax office demand the code | DZ-CMP-01, 09§5.5, 01§1 (6), 01§4.1 (Smart Gestion, WINSTO, ZSOFT, PRO SOLUTION), 01§9.2, 01§9.4, 01§12 (4) |
| No guarantee-cheque workflow for instalment sales | Criminal exposure for customers (Echorouk) | 02§3.14 |
| No prize draw tied to a purchase or payment | Décret 06-215 art. 9 | DZ-CMP-03 |
| Never show TVA on IFU invoices | CTCA art. 64 | 04§2.2 |
| Never ask for the merchant's DZBuild password or merchant API key; no dashboard scraping | DZBuild security rules | 06§7 |
| No embedded-WebView login; no client secret in the binary; no tokens in DB, logs, dumps, exports or backups | RFC 8252, DZBuild rules | 06§5.4, 06§7 |
| No SQL from the webview | Security architecture | 07§4 |
| Never put the SQLite file on an SMB share or run it from a USB stick | Corruption risk | 05§8.2, 05§9 |
| Never store card data | PCI scope | 03:SEC-007 |
| No telemetry or crash upload without explicit consent | Loi 18-07, trust | 03:ADM-008, 07§14 |
| No health/religion data outside the pharmacy pack | Loi 18-07 art. 18 | 04§7 |

---
## 41. Counts and coverage check

### 41.1 Totals

| Item | Count | Before report 01 was merged |
|---|---|---|
| Feature rows (§1–§36) | **756** | 705 |
| Reports and états (§37) | **85** | 82 |
| Non-functional requirements (§39) | **27** | 26 |
| **Catalogued items in total** | **868** | 813 |
| Excluded / not applicable rows (§40.1) | 29 (22 from 03, of which one re-included as AUD-013, plus 7 scope exclusions) | 25 |
| Design prohibitions (§40.2) | 12 | 12 |
| Entities in the import/export matrix (§38) | 37 | 37 |

Feature rows by release: **361 [MVP]**, **245 [V1]**, **150 [V2+]**. Reports by release: 43 [MVP], 33 [V1], 9 [V2+]. NFRs: 26 [MVP], 1 [V1].

**What the 2026-09-28 update added (55 rows):**
- **49 rows from report 01** that the spec did not cover: PLT-026; DAT-023–025; LAN-019–020; USR-015; UX-040; ONB-012–013; SAL-038; PRM-026; TAX-017; DOC-027–033; PRD-044–046; LBL-013; STK-039; PUR-018–020; CRM-015–016; CRD-012; LOY-011; ORD-010; FIN-009–012; RST-027; VRT-005, VRT-039, VRT-102–103; IMX-021–022; AI-010–011; RPT-083–085. By release: 7 [MVP], 26 [V1], 16 [V2+].
- **6 rows for the trilingual decision** (§0.6), all [MVP]: UX-041–044, DOC-034, NFR-027. Fifteen existing rows were rewritten for three equal languages (UX-001, UX-003, UX-004, UX-005, UX-014, UX-032, UX-035, DOC-004, DOC-005, DOC-013, LBL-005, PRD-008, ONB-001, ONB-004, ONB-010, plus NFR-020 and NFR-023 and small edits to TAX-001, IMX-005, IMX-006, ONB-008).
- Existing rows enriched with 01 details: PRC-001, PRD-003, LOY-004, RPT-001, RPT-007, RPT-053, EMP-002, FIN-003, PLT-016, PUR-002, ORD-006, VRT-037, PAY-026, UX-018.

The MVP is large on purpose: the brief is "the most complete POS in Algeria". Most MVP rows are small CRUD, setting or report items on one shared data model. The engineering-heavy MVP blocks are the offline DB with LAN multi-till, the hardware layer (Arabic raster printing, scanners, scale barcodes, drawer, displays), the document engine (AR/FR/EN, RTL and LTR, bilingual layouts, legal mentions, amount in letters in three languages), the translation pipeline and its CI gate, the fiscal journal (hash chain, Z, archive), CMUP costing, pricing/promotion rules, the timbre engine and the import/export engine.

### 41.2 Rows per module

| Module | Rows | Module | Rows | Module | Rows |
|---|---|---|---|---|---|
| PLT | 26 | PRM | 26 | CSH | 17 |
| DAT | 25 | TAX | 17 | FIN | 12 |
| LAN | 20 | DOC | 34 | RST | 27 |
| USR | 15 | PRD | 41 | VRT | 55 |
| AUD | 13 | LBL | 13 | OMN | 10 |
| PDP | 10 | STK | 39 | DZB | 28 |
| UX | 44 | PUR | 20 | HW | 35 |
| ONB | 13 | CRM | 16 | IMX | 22 |
| CFG | 10 | CRD | 12 | API | 5 |
| SAL | 49 | TMP | 9 | AI | 11 |
| PAY | 26 | LOY | 11 | RPT | 85 |
| RET | 15 | ORD | 10 | NFR | 27 |
| PRC | 12 | EMP | 8 |  |  |

### 41.3 Rows tagged for a specific vertical (feature and report tables)

SUP 25 · FRS 14 · BAK 17 · RST 50 · PHA 25 · QNC 13 · AUT 10 · CLO 10 · TEL 21 · COS 10 · LIB 3 · WHS 32 · OPT 5 · INS 18 · PRS 3 · SAL 4 · JEW 10 · GYM 5 · KSK 10 · CNS 3 · IMP 6 · VAN 5 · MFG 6. All other rows are `ALL`.

### 41.4 Release changes vs the research tags

| Row | Research tag | Tag here | Why |
|---|---|---|---|
| PRD-051, PRD-052, PRD-053 (lots, expiry, FEFO) | 03: V1 | MVP | Expiry lists are table stakes in local products (02 §1, 01 §1 (5)); décret 13-378 requires lot and DLC/DDM labelling (04 §9) |
| AUD-004 (hash-chained fiscal journal) | 03: V1 | MVP | Needed for the 1 Jan 2027 publisher engagement (04 §6.1, 01 §11) |
| RPT-014 (simple P&L, net profit after expenses) | 03: V1 | MVP | Sold by nearly every Algerian competitor (02 §1, §4.4, 01 §4.2) |
| RST-018 (waiter handheld app) | 03: V2 | V1 | Marketed by Algerian restaurant vendors (02 §3.4, 01 §4.3) |
| DOC-012 (warranty certificate) | 03 PRD-054: V1 | MVP for TEL/INS | Legal document under décret 13-327 (09 §5.1) |
| AUD-013 (CCTV overlay) | 03: n/a | V2+ | Hikvision NVR POS overlay found (09 §7.4) |
| EMP-005 (salaires & avances lite) | 03: n/a | V2+ | Local competitors bundle payroll; IRG barème verified (09 §2.2, §5.11) |
| UX-001, UX-004, DOC-004, DOC-013, LBL-005, ONB-004 and the new language rows | 03/02: "AR/FR" or bilingual | MVP, trilingual | Owner decision 2026-09-28: Arabic, French and English equal at MVP (§0.6) |

Report 01 has no release tags; the tags of the 49 rows it added were set here, using the same rule as the rest of the spec.

### 41.5 Coverage check

A script extracted every feature ID from `research/03` (547) and `research/09` (50), then searched the *Src* cells of this file (and the first column of §40.1). Result on 2026-09-28, re-run after the 01 merge: **597 of 597 source IDs are traced; 0 missing; no duplicate row IDs.** Findings without IDs (01, 02, 04, 05, 06, 07) are traced by section references such as `01§9.4`, `02§3.3` or `04§4`.

**Report 01** has no feature IDs. Every bullet of its master catalogue (01 §9.1–§9.11) and of the vendor feature lists in 01 §3, §4.2–§4.5, §8, §11 and §12 was checked against this spec. **250 rows now cite 01**: the 53 new rows that come from it or restate it (the other two new rows, DOC-034 and NFR-027, rest on 04 and the owner decision) and 197 existing rows whose *Src* gained an `01§…` reference. Items that are out of scope are in §40.1 (enterprise ERP modules, cross-docking, licence mechanics, macOS/Linux builds, coin dispensers); the illegal ones (fictive, "virtual" or non-accounted invoices, real vs virtual stock, hidden non-billable stock) stay in §40.2.

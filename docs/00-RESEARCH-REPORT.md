# 00 — DZBuild POS: Master Research Report

> **Product:** a free, offline-first, installable Windows POS plus gestion commerciale / gestion de stock for Algerian shops, offered by DZBuild (dzbuild.dev) and optionally linked to a DZBuild account through OAuth.
> **Date:** 2026-09-28 (updated the same day after report 01 was recovered). **Role of this file:** synthesis of research files 01–09, for the product owner and the build team.
> **Companion:** [`01-MASTER-FEATURE-SPEC.md`](01-MASTER-FEATURE-SPEC.md) is the complete, deduplicated feature catalogue (756 features, 85 reports, 27 non-functional requirements; updated 2026-09-28 with report 01 and the trilingual decision).
> **Language decision (product owner, 2026-09-28):** the app is **natively trilingual**. Arabic (RTL), French and English are equal, first-class languages for the UI, documents, reports, exports, help and onboarding. It is not an Arabic-first or "Arabic + French" product. See §1 (13) and spec §0.6.

---

## Contents

1. Executive summary
2. Algerian market map
3. What users complain about, and our opportunities
4. Regulatory must-haves
5. Hardware support matrix
6. DZBuild integration summary
7. Recommended stack
8. How we build it: engineering practices
9. Risks
10. Contradictions resolved between research files
11. Open questions and unverified items
12. Next steps
13. Section files

---

## 1. Executive summary

1. **The market is very fragmented and sold through Ouedkniss and social media.** Three layers: about **15–20 established editors** selling ERP and gestion commerciale through integrators; **hundreds of micro-editors and hardware resellers** selling Windows desktop POS on Ouedkniss, Facebook, TikTok and YouTube; and a **new cloud/SaaS wave** (Almawarid, Codeshade, LogiTools, Symloop, Fisira, eFawtara, SouqDzayer, Restorium, Inyad/Mahaal) (01 §1). The standard pitch: 100 % Algerian, Arabic + French, works offline, **no subscription (بدون اشتراكات)**, free lifetime updates, trial on request, installation over AnyDesk. On Ouedkniss (505 listings, 456 software), the **median price of the 194 priced software listings is 18,000 DA**; 53 % cost under 20,000 DA and 78 % under 40,000 DA (01 §1, §5). Prices run from **3,500 DA** (Smart Barcode, SolutionInfoDZ) through 8,000–35,000 DA for a typical small-shop lifetime licence, 95,000–110,000 DA for editor packages (SmartCom), **45,000–150,000 DA** for pharmacy, and 85,000–170,000 DA for hardware + software packs. SaaS runs 11,900–120,000 DA per year (Fisira, LogiTools, Almawarid, Symloop); one pharmacy SaaS reads 80,000–200,000 DA per month (01 §5, 02 §6, 09 §3.2).
2. **"Free" alone is not a differentiator.** Four free offers already exist, all limited (01 §1 (4), 09 §3.1):
   - **Klick Stock Express** ("GRATUIT à vie", Windows 7–11, French only) leaves out kridi, multi-depot, price tiers, colisage, inventory, reports and treasury; those sit in the 9,900–10,000 DA Standard edition.
   - **CRESCENDO POS Free** is single-PC, capped at 2 tills, 2 users and 2 company files, French only, and needs an activation key.
   - **Almawarid's free plan** is cloud-bound: 3 invoices a month, no PDF.
   - **Dipposoft** has a free version with undocumented limits.

   No product is at once free, complete, offline, multi-PC and in Arabic RTL, French and English. DZBuild POS wins only if it is **free, complete, compliant and trilingual** at the same time.
3. **No product covers every vertical.** Pharmacy, restaurant, auto-parts, optician and instalment tools each serve one trade; general tools (BNsoft, KERS, AlgoStock) are shallow on restaurants, scales and instalments. Buyers ask for "commercial management + repairs in one" on three separate listings, and for jewellery, packaging and instalment support (01 §8). Real gaps: scale PLU push and live weight reading, carcass yield for butchers, school-list packs for librairies, jewellery compliance, gyms, small wholesalers with delivery rounds (02 §8, 09 §4). A modular app with **vertical packs** is unmatched.
4. **The "Algerian way" of running a shop is well defined.** Kridi ledger with versements and opening balances from the paper notebook; at least four price levels (détail, demi-gros, gros, revendeur); held tickets; expenses from the till and "net profit after expenses"; **Zakat** (official nisab 1447 AH: 2,295,000 DA); flexy top-ups; phone repairs; expiry lists; label printing; Arabic RTL + French interface (a few products already add English: AlgoStock, MekaStore, DS Software, FlamePOS, Factor Facile); 58/80 mm tickets and A4/A5 invoices; BL → facture; carton/pack/unit selling; several barcodes per product; scale barcodes; supplier debts; cash float and daily close with variance; several tills on a LAN; NIF/NIS/RC/AI, VAT 19/9 % and timbre on documents; Excel import (01 §1 (5), 02 §1, §4; 09 §5.6).
5. **Law shapes the product more than features do.** Mandatory invoice mentions (décret 05-468), NIF, a ticket for every consumer sale, gapless numbering, TVA 19/9 % with no TVA shown under IFU, the **droit de timbre** algorithm (flat bracket, not progressive, minimum 5 DA, cash only), and from **1 January 2027** a publisher engagement that the cash software is **inalterable, secured, preserved and archived** (LF 2026). Retention is 10 years for documents. New in 09: warranty law (décret 13-327), soldes law (décret 06-215), regulated staple prices, and the 2026 jeweller register (04 §0, 09 §5).
6. **Several competitors sell illegal features.** "Stock non facturable", "factures fictives/virtuelles/non comptabilisées" are marketed openly (Klick, Smart Gestion, WINSTO, ZSOFT, PRO SOLUTION; 01 §1 (6), §4.1). Since LF 2026, fraud by IT means carries 5–10 years and 5–10 M DA, **accomplices get the same penalties**, and the tax office can demand a publisher's source code. "Conforme, inaltérable" is therefore a selling point, and DZBuild must not ship those features (09 §5.5).
7. **The hardware is cheap, Chinese and ESC/POS.** Xprinter-class 80 mm printers (10,000–23,500 DA), HID scanners (5,800–23,500 DA), RJ11 drawers (7,500–15,000 DA), TSPL label printers (14,500–42,500 DA), EAN-13 label scales (61,000–109,000 DA), all-in-one i3/i5 3rd/4th-gen terminals (61,500–150,000 DA) (01 §10, 05). **Arabic must be printed as a raster image.** Scanners break on AZERTY and Arabic keyboard layouts. Card TPEs are standalone bank devices (05 §0, 09 §7).
8. **The environment is hostile to cloud-only software.** Windows 11 52.8 %, Windows 10 46.0 %, Windows 7 0.86 % (StatCounter, Aug 2026). Nationwide internet shutdowns during the baccalaureate (five × 10 h in June 2023). Frequent power cuts (anecdotal). So: local database as the source of truth, crash-safe writes, automatic verified backups, LAN multi-till with no internet (05 §7–§9).
9. **DZBuild is an e-commerce platform, not a POS vendor.** It claims 14,498 stores, is built around cash on delivery, and launched a developer platform (OAuth 2.0 + PKCE, REST API, webhooks) on **2026-09-26**. A desktop app can't use that OAuth directly today: redirects must be https and a client secret is mandatory. The POS needs a DZBuild-run **"POS Link" broker** now (Track A) and a first-party public client later (Track B). Linking stays optional; the POS never needs activation (06 §0).
10. **Recommended stack:** **Tauri 2.12 (Rust core) + React 19/TypeScript + SQLite (rusqlite, WAL, `synchronous=FULL`, SQLCipher)**, with an embedded LAN host/client mode (axum + mDNS), our own ESC/POS layer with Arabic raster printing, WebView2 silent printing for A4, calamine/rust_xlsxwriter for Excel, and the signed Tauri updater. Targets Windows 10 1809+ / 11 x64 (07 §0).
11. **The catalogue:** 756 features (361 MVP, 245 V1, 150 V2+), 85 reports and états, 27 NFRs, 37 import/export entities. All 547 benchmark features (03) and 50 gap additions (09) are traced, and every item of report 01's feature catalogue (01 §4.2, §9) maps to a spec row or an exclusion.
12. **Research limits.** All work is desk research: no shopkeeper interviews and no hands-on test of competitor software (01 §0, 09 P4). Many local-product facts come from listings and social media (tag [S]). Report 01's coverage of foreign products (Loyverse, Daftra, DEXEF, PC Compta, EBP) is thin because the search stopped before every lead was followed. Facebook pages, YouTube descriptions and Ouedkniss mirrors could not be read. Several regulatory texts are pending or unread (§11).
13. **Language decision: natively trilingual.** The product owner decided on 2026-09-28 that DZBuild POS ships **Arabic (RTL), French and English as equal first-class languages**, all at MVP. The market research alone pointed to "Arabic + French" (01 §1 (20), 02). English is now an equal; a few small competitors already offer it (AlgoStock, MekaStore, DS Software, FlamePOS, Factor Facile, SmartStock), and most established editors do (Onyx, IntelliX, Inabex, Cirtasoft). What "equal" means (spec §0.6):
    - UI in the three languages, switchable at runtime and saved **per user**;
    - receipts, invoices, quotes, BL, statements and labels printable in **any of the three languages, per document or per customer**, with an optional **bilingual AR + FR layout**;
    - **amount in letters** in Arabic, French and English;
    - reports, dashboards and exports (XLSX/CSV/PDF) in the three languages;
    - search that normalises Arabic and Latin scripts;
    - numerals: Western 0–9 by default, Arabic-Indic optional;
    - help, documentation, onboarding and videos in the three languages;
    - **every translation complete at release**, enforced by a CI check that fails on any missing key.

    Legal limit: consumer information on food and regulated goods must stay "essentially in Arabic" (décret 13-378), so the app warns when such a label or ticket is printed without Arabic (§4 (15)).
14. **What buyers expect by default** (01 §1): till speed (keyboard-only with reprogrammable F-keys, several open tickets); tills that keep selling when the server fails (a 70,000 DA premium at TifawtTech); support for old PCs (Windows 7/XP and 512 MB RAM claims); an owner mobile app, bundled even with 15,000–20,000 DA products; AI supplier-invoice OCR (5 products); WhatsApp receipts and support; and Flexy, repairs, instalments, état 104/G50/Zakat helpers and bon de route as local extras. Licences are tied to the machine and installed over AnyDesk.

---

## 2. Algerian market map

**Sources.** Report 01 is the main source: a French, Arabic and English web sweep, the vendor sites, and Ouedkniss's public GraphQL API on 2026-09-28 (28 searches, **505 unique listings, 456 of them software**, about 110 read in full, with view counts and comments). Reports 02 (Arabic and social-media vendors, verticals) and 09 (gap sweep, extra Ouedkniss verticals) add the rest. Every feature list is a vendor claim: nobody installed or tested a competitor (01 §0).

**How to read the tables.**
- **Confidence tags:** **[V]** verified on the vendor page or an official source; **[S]** seen in a listing, search snippet or social media only; **[U]** unverified.
- **Prices** are in DZD as read in September 2026. "n/p" = not published (many Ouedkniss sellers post a symbolic 1 DA or 10 DA). "LT" = lifetime licence.
- **Languages** = UI languages the vendor claims: AR (Arabic), FR (French), EN (English), TR, ZH.
- **Views** = cumulative Ouedkniss views: a proxy for interest, not sales.
- Ouedkniss listings are cited as reference links (`[dNNNNNNNN]`), defined at the end of this file.

### 2.0 Market structure (01 §2)

| Segment | Buyers | Platform | Price | Examples |
|---|---|---|---|---|
| **A. Established ERP / gestion commerciale editors** (10–35 years old) | SMEs, distributors, pharma, industry, chains | Windows client/server (SQL Server, Oracle), increasingly web | On quote; 95,000–200,000+ DA per module or pack | Onyx (S2I), Megasoft, IntelliX Silwane, Inabex, Devoro, BIG Informatique, Smart Partner, Megatheque, Cirtasoft, Elmoumtez, Winalco, BBS, Tayssir |
| **B. Micro-editors: Windows desktop POS + stock** | Superettes, grocers, cosmetics, clothing, phones, auto parts, hardware stores | Windows desktop, offline, single PC or LAN | 5,000–50,000 DA lifetime | PME PRO, Quick Fact, SMART-Com, ITSCOM, GFS, ZSOFT, SAFE POS, DS Software, AMPRO, MekaStore, AlgoStock, Zedkira, Dolisoft, Crescendo, Flampex, FlamePOS, KERS POS |
| **C. Hardware integrators bundling software** | Same as B | Touch terminal + printer + drawer + scanner + licence | 24,500–170,000 DA per pack | Digital Company / General IT, Asalix (Auradev), Tpv Technologies, Ecopos, Danisoft, Expert POS, CLYO, Computime (CaisseKom), Eurequat, OptiQ |
| **D. Restaurant / café POS** | Fast food, pizzeria, café, tea room | Windows touch; SaaS on tablets | 15,000–42,000 DA lifetime; SaaS from 25,000 DA | Restorium, FoodMax, CoffeLine, Msk Ozone, Kiober Food POS, Menu Max, CaisseKom Resto, Crescendo Restauration, Symloop, INNOVA POS, PRO_Resto |
| **E. Cloud / SaaS / mobile** | Tech-savvy SMEs, multi-branch | Web + Android (some with an offline Windows client) | 7,000 DA/month to 120,000 DA/year | Almawarid, Codeshade, LogiTools, Symloop, SouqDzayer, FlustockX, Inyad (Mahaal), Tayssir, SAMA VISION, CSI-Stock |
| **F. Invoicing-only SaaS** | Services, self-employed | Web | 11,900–35,000 DA/year | Fisira, eFawtara, Costy, Factury, Fatoura, InnovaInvoice |
| **G. Foreign ERPs through local partners** | Mid-size and large companies | Desktop / cloud | On quote | Sage 100/X3 (BSC), Odoo (ITHRYTEC, Digitaliac, Digisys, Finoutsource, Numidit), Dolibarr (Doliflex), Gestimum (NTIC-SOFTPRO) |
| **H. Vertical niche tools** | Flexy/phone shops, salons, pharmacies, instalment sellers, jewellers | Windows desktop | 3,500–120,000 DA | KERS FLEXY, Easy Life Flexy, FASTransfer, Dukan Pro, ECHO, sCom_pharm, PharmaX, "Gestion de Facilité" |

**What shapes the market (01 §1):**
- **Windows desktop, offline, lifetime licence is the default.** Listings repeat "fonctionne sans internet", "licence/activation à vie", "sans abonnement", "mises à jour gratuites".
- **Licences are tied to the machine** (serial per PC, "valid after formatting", USB-key licences), with installation over AnyDesk/TeamViewer and support over WhatsApp.
- **Hardware shops are the main channel.** Many "editors" are POS-hardware shops bundling a licence (segment C; also Lamwas tech, Groupe SPV, Chrono Informatique).
- **WinDev/HFSQL is the usual stack.** Cracked WinDev 25 sells openly at 30,000 DA ([d51577181]); buyers quote WinDev runtime errors (01 §7).
- **Till speed, old PCs and an owner mobile app sell.** Keyboard-only tills with F-keys, Windows 7/XP support and 512 MB RAM claims, and an Android owner app bundled even with 15,000–20,000 DA products.
- **AI has reached cheap products:** supplier-invoice OCR at GestiumPRO (Scaneye), LogiTools, AlgoStock Elite, Factor Facile and Fisira.
- **Languages:** most small products are French-only or Arabic + French. Only a handful claim Arabic, French **and** English (AlgoStock, MekaStore, DS Software, FlamePOS, Factor Facile, Codeshade, SmartStock, Bou Saâda till); the established editors add English and sometimes Turkish.

### 2.1 Free and near-free baselines

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | **Stock Express** (Klick Stock) | Klick Algérie (Fouka, Tipaza); also sold in ~13 African countries | Windows 7–11; Standard, Server and Cloud editions | **Free for life**; Stock Standard 10,000 perpetual (vendor), 9,900 through a reseller | FR | General retail, wholesale | Full sales cycle (devis → avoir), full purchase cycle, counter sales, POS hardware, print customisation, flexible taxes; paid editions add multi-depot and colisage | Free edition has no customer balances (kridi), multi-depot, price tiers, colisage, inventory, reports or treasury; French only; headline feature "stock facturable / non facturable" is legally risky | [V] | https://stock.klick.dz/ · https://stock.klick.dz/fr/prix · https://www.klick.app/ · Ouedkniss [d54257126] |
| 2 | **CRESCENDO POS Free** | Crescendo Software | Windows 7+ and Windows 10 tablets; no Android, iOS or Mac | **0**, activation key sent by email (can be activated offline). Paid: Commerce Pro 20,000; Fashion 25,000; Restauration 30,000; Loisir 35,000. Support: Silver 10,000/yr, Gold 20,000/2 yrs, Platinum 30,000/3 yrs. 30-day trial of paid editions | FR | Superette, general retail (free); fashion, restaurant, leisure (paid) | Product families, automatic CUMP, basic barcode, stock. Paid editions add departments, promotions, photos, composite items, expiry, LAN, sizes/colours, floor plans, pizza composition, kitchen orders, gift vouchers, loyalty, automatic restocking, multi-site, losses, free items | Free edition: **single PC only, max 2 tills, 2 users, 2 company files**; activation key required; French only; LAN only in paid editions | [V] | http://www.crescendo-software.com/ · https://www.crescendo-software.com/produits/ · https://www.crescendo-software.com/foire-aux-questions-faq/ |
| 3 | **Almawarid free plan** | Issal SARL (Bir El Djir, Oran) | Web + Android; Windows app with offline/LAN mode | **0**: till, stock, 3 invoices, 1 payslip and 1 contract per month; PDF export only on paid plans | FR/AR/EN/TR/ZH | General retail, 41 professions | Same engine as the paid plans (see Almawarid in §2.6) | Cloud-bound; a 3-invoice cap makes it a trial, not a tool | [V] | https://almawarid.app/tarifs · https://almawarid.app/blog/logiciel-gestion-commerciale-algerie-gratuit-ou-payant/ |
| 4 | **Dipposoft** (free version) | Seller, Djelfa | Windows, LAN; mobile app | Free version (limits unknown); paid 16,000 | "Multi-language" (which not stated) | General retail | Google Drive backup; fraud audit; mobile app | Free-version limits undocumented | [S] | Ouedkniss [d56826623] |
| 5 | **SolutionInfoDZ** | Bordj Bou Arreridj | Offline (Windows assumed) | **From 3,500**, lifetime activation, free trial | n/p | General retail | Cashier, stock, barcode, customer credit; price floor of the market | 43 views; thin details | [S] | Ouedkniss [d55739143] |
| 6 | **Smart Barcode System** | Seller, Annaba | Windows | 3,500 launch (15,000 normal) | AR | General retail | Data survives a reinstall | Launch price only; thin details | [S] | Ouedkniss [d56453599] |
| 7 | **"Magasin & Stock 2026"** | Seller, Douera | Claims Windows, Mac and Linux; Wi-Fi sharing | 5,000 launch (9,000 normal) | AR/FR | General retail, weighed goods | Sells by weight, length or volume | Multi-OS claim unverified | [S] | Ouedkniss [d56207773] |
| 8 | **"برنامج تسيير الفواتير باللغة العربية"** (Arabic invoicing program) | El Eulma, Sétif | n/p | 5,000 | AR | Invoicing | **23,194 views**, the 3rd most-viewed software listing: strong demand for Arabic invoicing | Thin details | [S] | Ouedkniss [d6347367] |

**What the free editions leave out.**
- **Klick Stock Express** drops kridi, multi-depot, price tiers, colisage, inventory, reports and treasury (09 §3.1).
- **Crescendo POS Free** is single-PC, capped at 2 tills, 2 users and 2 company files, French-only, and needs an activation key.
- **Almawarid's free plan** is cloud-only and capped at 3 invoices a month with no PDF.
- **Dipposoft's** free limits are undocumented.

**No product is at once free, complete, offline, multi-PC and natively trilingual (Arabic RTL, French and English as equals)** (01 §1 (4), §12 (1)). DZBuild POS can own "free forever, every feature, no activation key, three languages".

### 2.2 Established editors and mid-market suites (segment A)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 9 | **Onyx Pro ERP** (OnyxPOS, OnyxStock) | Ultimate Solutions Algérie / S2I (SII), Bir Mourad Raïs | Modular ERP on Oracle or SQL Server; POS synced with the back office | n/p; "one year of updates and free maintenance" | FR/AR/EN/TR | Chains, malls, super/hypermarkets, confectionery, clothing, shoes, jewellery, perfume | Scales, **CIB TPE integration**, coin dispensers; damaged/expired articles; loans; store transfers; mobile inventory; size/colour; weighed goods; loyalty, promotions, orders; custom receipt layout; multi-unit, multi-depot stock; finance, HR, production, treasury back office | Enterprise pricing and on-site deployment | [V] | https://www.solutionsinformatiques.dz/Logiciel-de-gestion-de-points-de-vente-POS-algerie?lang=fr · https://www.solutionsinformatiques.dz/Inventory-systems |
| 10 | **MEGASOFT Office** (MEGA COMMERCIAL) | Megasoft, Hydra (since 1990; 300+ clients, 12,000+ users) | Windows client/server or single PC; Megasoft Digital cloud | On quote | n/p | SMEs; auto (MEGA-AUTO, §2.11), pharma, real estate, labs | ERP, SCF/IFRS finance, HR and CNAS/IRG payroll, GPAO, GMAO; TMS, WMS, MES; SAP integration; AI supply-chain agent | Enterprise scope; not a shop till | [V] | https://www.megasoft-office.com/ |
| 11 | **Silwane ERP / iCom / Comptoir** | IntelliX Group (Bordj El Kiffan; East and West branches; Casablanca, Tunis), 25+ years | Windows; Silwane .Net cloud; Androway (Android) | On quote; 30-day trial | FR/AR/EN | Retail (Comptoir POS), pharmacy (Signature, §2.9), distribution (Androway with GPS), multi-site retail (VPN) | iCom has 70+ commercial modules; accounting, payroll, fixed assets | Price hidden; enterprise | [V] | https://www.intellixgroup.com/ |
| 12 | **Inabex suite** (Works, AfterWave, Zorg, DirectWay, ArreniA, LiveData, Xycomat, InaBox) | Inabex (since 1998) | Windows (SQL Server), web, Android | On quote | FR/EN/AR | SMEs, distribution, multi-store retail, pharmacy (PharmaX, §2.9) | Purchase/sales cycle, stock, cash; field sales with barcode; multi-store sync with **central purchasing**; real-time cloud stock; picking; mobile POS receipts; CloudSave backup | Many separate products; price hidden | [V] | https://inabex.com/logiciels-de-gestion-en-algerie/ |
| 13 | **Devoro ERP** | Devoro (Oran) | ERP + PDA | On quote | n/p | Distribution, industry | 13 modules: purchasing (RFQ, EDI), sales (price levels, commissions), stock (lots, serials, expiry, FIFO/LIFO/CUMP), distribution (routes, geolocation, **e-proof of delivery**), offline PDA order capture with portable printing, BI, SCF accounting, MRP, CNAS/IRG payroll, CRM, WMS, fleet, e-commerce | Enterprise scope | [V] | https://devoro-erp.com/ |
| 14 | **BIGSOFT** (Achats Stock, Stock Vente) | BIG Informatique (Annaba, Alger, Oran) | Windows | On quote | n/p | SMEs, hospitals, pharmacies, hotels (WinHotel), transport, education, public procurement | Multi-warehouse with automatic valuation; receipts, internal consumption, transfers; three inventory methods; purchase request → supplier invoice; automatic accounting entries | Not a till product | [V] | https://biginformatique.com/produits/achats-stock |
| 15 | **SmartCom / SmartPOS / Tawsil** | Smart Partner (Dar El Beida; since 1998; "+4,000 users") | Windows multi-user network; Tawsil mobile distribution | SmartCom Commercial **95,000 TTC**; Counter/POS **110,000 TTC**; Plus on quote | n/p | SMEs, retail, pharmacy (SmartAvicenne), distribution | Full sales cycle, multi-warehouse, price management, **bank reconciliation**, loyalty, automated archiving, import/export, POS peripherals, PDA inventory | Mid-market price | [V] | https://www.smart-dz.com/produits-services/smartcom · http://www.smart-dz.com/ |
| 16 | **DELTA POS** | MEGATHEQUE (Tizi-Ouzou) | Windows; "serveur non stop" replication | n/p | n/p | Retail | Offline till with automatic sync; scale; EAN-13/EAN-8 generation; loyalty cards; bulk, generic and pack products; photo catalogue; profile-based access; printers, displays, drawers, scales, readers, tablets, PDAs | Price hidden | [V] | https://megatheque.net/logiciel-point-de-vente-algerie/ |
| 17 | **COMMSoft** | MEGATHEQUE (Tizi-Ouzou) | Windows | n/p | n/p | Wholesale, distribution, pharma distribution, importers, production | **Richest feature list in Algeria:** full document set incl. supplementary invoices and goods vs financial credit notes; lots with FIFO/LIFO/CUMP; ageing, cheques and drafts, multi-currency debts; **13 tariffs**; BOM, routings, work orders, cutting plans; landed costs; warranty and serials; **returnable packaging (consignes)**; reps, routes, commissions; PPA/SHP pharma margins and quotas; hourly footfall and VAT curves; export to Excel/Word/HTML | Price hidden; breadth = complexity | [V] | https://megatheque.net/logiciel-gestion-stock-commerciale-algerie/ |
| 18 | **Gestium family** (GestiumGo, GestiumPRO, GestiumERP) | Cirtasoft (since 2005) | Web / desktop; delivery mobile app | GestiumGo **25,000 HT**; PRO and ERP (190+ modules) on quote | FR/EN/AR | 50+ sectors | Devis → facture; multi-warehouse with rupture alerts; touch POS with daily closing and multi-payment; CRM and loyalty with automated follow-ups; delivery routes with driver tracking; cheques and due dates; price grids, quantity pricing, seasonal promotions, contractor credit ceilings; **"Scaneye" AI OCR** of supplier invoices; publishes the clearest guide to the 2027 certification rule | PRO/ERP prices hidden | [V] | https://cirtasoft.com/gestiumpro/ · https://www.cirtait.com/logiciel-gestion-commerciale/ar/ |
| 19 | **Elmoumtez** | Elmoumtez Informatique (Annaba, since 1994) | PC, all-in-one, tablet, cloud/browser | n/p | n/p | Retail, restaurant | Accounting, payroll, invoicing, commercial management, POS, restaurant, treasury, production | Thin details | [V] | https://www.elmoumtez.com/ |
| 20 | **Winalco GSCOM** | Winalco (Sétif, since 2002) | Windows 8.1/10/11 | n/p; free demos | n/p | SMEs | Stock, purchasing, invoicing, quotes, BL; Winalco SCF accounting and Paie (CNAS/IRG); **Smart Check**: free cheque printing for Algerian banks | No POS focus | [V] | https://winalco.dz/ |
| 21 | **Planète CAISSE** | DSSI (Alger) | Windows | n/p | n/p | Retail, multi-site | Seller management and till security; profile duplication; function unlocking at the till; customer risk and credit alerts; order → BL → invoice → avoir; central and per-site stock; A4/A5 | Price hidden | [V]/[S] | https://www.d2si.eu/solutions/gestion_des_points_de_vente · Ouedkniss [d22047875] |
| 22 | **Adisoft gestion commerciale** | Adisoft | Windows | n/p | n/p | SMEs, wholesale | 5 price categories; **supplier invoice updates prices automatically**; HT/TTC; labels; quote → order → BL → return → invoice → avoir; advances and payment terms; PMP; reps; custom print templates; charts | Price hidden | [V] | https://www.adisoft-dz.com/logiciel-gestion-commerciale/ |
| 23 | **Tayssir ERP** | Tayssir (Baraki, Alger) | Web | n/p | n/p | Distribution, e-commerce | Double-entry inventory, multi-warehouse, reorder points, lots and serials, dropshipping, cross-docking, CRM, purchasing | Web-only | [V] | https://tayssir-erp.dz/tayssir-erp-logiciel-de-gestion-commercial-stock-crm-rh/tayssir-erp-logiciel-gestion-stocks-algerie/ |
| 24 | **BBSCom** | BBS (Oran) | Windows network | 35,000 | n/p | Wholesale, retail | Stocked and service items; assembly/disassembly; retail/semi-wholesale/wholesale tariffs; PUMP, LIFO or last-price valuation; price by customer and quantity; **automatic customer blocking on unpaid balances**; VAT-exempt sales; quote → invoice chain; 14,941 views | A buyer reports a WinDev runtime error ("erreur dans propriété ligne 381") | [S] | Ouedkniss [d10912645] |
| 25 | **SAMA VISION ERP** | SAMA DIGITAL COMPANY (Khraïssia) | Online; subscription or enterprise licence | n/p | n/p | Multi-store retail, repairs, production | Sales, purchasing, stock, invoicing, debts, POS, CRM, marketing, projects, HR, accounting, website/e-commerce, repairs, production; multi-company, multi-store, multi-POS | Online dependency | [S] | Ouedkniss [d51171823] |
| 26 | **INNOVA POS** (Store, Fashion, Clean, Beauty) / **INNOVA COM** | INNOVA SOFT EURL (Bir Mourad Raïs) | Windows touch | n/p | n/p | Retail, clothing, dry cleaning, beauty, restaurant | Touch till; **scale integration with article updates** (no duplicate programming); size/colour; **waterproof tickets** for dry cleaning; commission per service provider; loyalty | Prices hidden | [V] | https://www.innova-dz.com/ · https://www.innova-dz.com/Solutions-point-de-vente.html |
| 27 | **SOFTEL GestCom** | SOFTEL Solutions (Mohammadia) | Windows multi-PC, multi-company, multi-user | n/p | FR | SMEs | Customers by wilaya; serials; weighted-average cost; cheques; bank reconciliation; full action history; 7,413 views | Price hidden | [S] | Ouedkniss [d52503089] |

### 2.3 SME desktop POS and gestion commerciale: detailed offers (segment B)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 28 | **PME PRO** | TifawtTech (Dar El Beida) | Windows single PC + LAN | 70,000 listed ("from 20,000" in a reply). Base licence: 1 user, 2 company files, 2 till/bank accounts, 4 payment modes, each extendable as a paid option | FR | Superette, general retail, wholesale | **Most-viewed software ad (39,497 views)**; 6 sale modes; product filters by wilaya, commune, seller, customer, supplier; commission scales; backup, restore and DB repair; scale and customer display; F-keys + touch; export to the accountant's format; GMAO add-on | Basics sold as paid options; French only | [S] | Ouedkniss [d12505797] |
| 29 | **Logiciel Gestion Supermarché** | TifawtTech (Dar El Beida) | Windows LAN with offline tills | 70,000 | FR | Supermarket | Unlimited tills with **transfers between tills**; loyalty, credit and consumption cards; promotions; unlimited company files; stock-minimum and expiry alerts; bank balances; inventory from a mobile app; charges, losses and depreciation; **tills keep selling when the server or network fails** (manual sync button) | Expensive; French only | [S] | Ouedkniss [d30370559] |
| 30 | **POS Light** | TifawtTech (Dar El Beida) | Windows single PC | n/p | FR | Superette, cosmetics, clothing | **Flexy option** | Thin details | [S] | Ouedkniss [d30369909] |
| 31 | **PRO-PME/PMI, PME PRO + Android distribution app** | PRO SOLUTION (Bab Ezzouar) | Windows single PC / LAN; Android distribution app | 30,000 | FR | Superette, toys, clothing, distribution | Safe (coffre); support 7 days a week, 8h–21h; 22,137 and 23,838 views (#4 and #2 on Ouedkniss) | Sells **"accounted / non-accounted invoices"** (fraud since LF 2026) | [S] | Ouedkniss [d11162776] · [d23967633] · [d11162910] |
| 32 | **Quick FACT** | PC LINE (Alger centre) | Windows single PC + server | 25,000 | FR | SMEs | Multiple company files and depots; expiry; export to Excel/Word/PDF; email; 15,797 views | French only | [S] | Ouedkniss [d16488297] |
| 33 | **ITSCOM / ITSpoint / ITSstock** | Its2010 (Akbou) | Windows XP–8.1, 512 MB RAM | 28,000 / 35,000 | FR | General retail | "Key for life"; runs on very old PCs; 16,263 views | Legacy Windows only | [S] | Ouedkniss [d17834397] · [d6894011] |
| 34 | **SMART-Com** | Seller (Chlef / West) | Windows single PC / LAN | 25,000 | FR (product names also in AR) | General retail | Activation survives formatting; **backup on close** | May differ from Smart Partner's SmartCom | [S] | Ouedkniss [d22499274] |
| 35 | **StockCom** | Seller (Biskra) | Single PC / LAN | 20,000 | FR/AR documents | Wholesale, retail | **Groups several BLs into one invoice** | Thin details | [S] | Ouedkniss [d23906352] |
| 36 | **GFS** | Chrono Informatique (Hydra); Chlef seller | n/p | 25,000 | FR/AR | General retail | Popular (YouTube tutorials) | Thin details | [S] | Ouedkniss [d33391286] · [d30034727] |
| 37 | **Smart Gestion V2.0** | M. Idris (Aokas) | Windows single PC | 35,000; 21-day trial | FR | General retail | Full purchase and sales documents | Sells **real and "virtual" invoices and stock** (fraud) | [S] | Ouedkniss [d24550112] |
| 38 | **WINSTO** | LOG103 (Sétif) | Client/server, multi-till | n/p | FR | Retail, wholesale | Multi-till | **Separates "billable" and "non-billable" stock** (fraud) | [S] | Ouedkniss [d21173889] |
| 39 | **WINCOM** | Seller (Sétif; listing in Chlef) | n/p | 85,000 (up to 120,000 with hardware) | FR | Wholesale, semi-wholesale, retail | 16,936 views | Expensive | [S] | Ouedkniss [d15298462] |
| 40 | **ZSOFT** | Seller ("58 wilayas") | n/p | n/p | AR/FR | General retail | Dead-stock report | Sells **real, fictive and proforma invoices** (fraud) | [S] | Ouedkniss [d40249015] |
| 41 | **SAFE POS / PMO Pro** | VAST SOFT (Bouzaréah) | Single PC / LAN; mobile distribution with van stock transfers | n/p | FR/AR | Superette, wholesale, van sales | **Several barcodes per product**; three sale modes; real-time stock across server and tills; loyalty points as cheques or discounts; net profit after charges and losses; **float per till**; most-profitable-products histogram; one-click backup | Price hidden | [S] | Ouedkniss [d24881513] · [d39454663] |
| 42 | **PROMOOO** | trSOFT (Alger) | Single PC / LAN | 22,000 | FR | Retail, production | SAFE POS feature set + production (raw material → finished goods) + import/export/printing of documents, barcodes and price tickets | French only | [S] | Ouedkniss [d26730980] |
| 43 | **easykes** | Seller (Oran) | n/p | 25,000 | FR | Superette, bureau de tabac / multiservices | Turnover by hour (peak hours); 13,005 views | Thin details | [S] | Ouedkniss [d12993513] |
| 44 | **Zedkira Inventory** | Zedkira (Tizi Ouzou) | Windows, multi-user; mobile app | 32,000 / 49,000 / 65,000; +4,000 per user, +8,000 per PC, +6,000 per phone; 15-day trial | FR | General retail, repairs, services | D.E. 05-468 invoice compliance; credit at the till with limits and alerts; multiple price tiers; customer statements; bundle sales; repair workshop; accounting export; activity log; local activation with an optional online check | Per-user, per-PC and per-phone fees; French only | [V] | https://inventory.zedkira.com/ |
| 45 | **AlgoStock** | AlgoSynapse | Windows 10/11 x64 + iOS/Android companion over **local Wi-Fi, no cloud**; LAN | 19,500 LT (1 PC); Pro Connect +5,000/yr; Elite AI +12,000/yr | **AR (RTL)/FR/EN**, switch without restart | General retail, repairs | Cash / CIB-Edahabia / BaridiMob / credit tenders; thermal + A4 + BL + devis; multi-warehouse; reorder suggestions; expiry; EAN; bulk price update; NIF/NIS/RC; repair module (tickets, parts, labour); 5 roles; local PostgreSQL, 100 % offline; USB/network backups; Elite: OCR, natural-language questions, forecasts, anomaly alerts | Yearly fees for mobile and AI; licence per PC | [V] | https://algosynapse.com/algostock/ |
| 46 | **Dolisoft PRO** | Dolisoft | Windows, offline, multi-user | **12,000 per PC**, LT; delivered on a USB key, cash on delivery; promotions ("buy 2, 3rd free"; earlier "1st licence bought = 2nd free") | FR/AR | Auto parts, general retail, production | Smart search (name, reference, barcode); shelf locations; tariffs détail/gros/professionnel; BL → facture; Excel mass import; ticket, A5 or A4; stock alerts, batches; production; customer debts; **VAT, stamp duty, "état 104", Zakat** | Per-PC licence | [V] | https://dolisoft-dz.com/ · https://dolisoft-dz.com/products/dolisoft-pro-logiciel-de-gestion-de-stock-special-algerie-2025 |
| 47 | **ProPOS / AMPRO** | Dif Softs (Bordj El Kiffan; founded 2022, 21 staff) | Windows, unlimited tills; free Android app (AMPRO) | AMPRO 12,000–15,000 (LT key, free updates for life); 7-day trial | FR (+EN) | General retail | Currency gain/loss; 45+ reports (ProPOS), 20+ (AMPRO); free owner Android app | French-first | [V]/[S] | https://www.difsofts.com/ · https://www.difsofts.com/logiciel-gestion-de-stock-gestion-commerciale-algerie/ · Ouedkniss [d56142529] |
| 48 | **ProCommerce** | Dif Softs (Bordj El Kiffan) | Windows and macOS (claimed) | 12,000 | n/p | Mid-size businesses | Treasury with due-date alerts; the listing positions it "contrairement à ProPOS" | Thin details | [S] | Ouedkniss [d44596393] |
| 49 | **Factor Facile** | Devion | Windows desktop (v1.6.70) + Android; multi-device | One-time purchase, n/p; download password "213" | AR/FR/EN | General retail, services | All commercial documents, POS, stock, kridi, expenses/revenues/staff; AI assistant; invoice from a photo; multi-device with permissions; encrypted local data, offline | Price not published | [V] | https://devion.cloud/factorfacile |
| 50 | **KERS POS** | KERS (Souk Ahras) | Windows, offline; Python/Matplotlib (inferred) | 10,000 LT, no subscription | AR/FR | Superette, restaurant, many verticals | **Richest small-vendor list (01 §4.2):** reads the scale weight; carton vs unit prices; credit with ceiling alert; mixed payment; kitchen-printer routing; RFID loyalty cards; tables; recipes and variants; live dashboard (daily target bar, peak hour, best cashier, average basket, AI financial summary, ROI, rotation); lots/expiry; EAN-13 generation; photo background removal; A4/80 mm invoices, BL, **bon de route**, VAT + timbre; CSV/Excel export of every table; NIF/NIS/RC/AI; supplier POs and overdue supplier debts; expenses, bank accounts | Site unreachable at fetch time (02) | [S] | https://kers-pos.shop/ · Ouedkniss [d54074597] |
| 51 | **MekaStore** (Starter / Pro / Network / Cloud / E-Com+ / Park) | Seller (Bab Ezzouar) | Windows; Network edition; cloud and e-commerce editions | 20,000 / 50,000 / from 100,000; 7-day trial | AR/FR/EN | General retail, e-commerce | Phone monitoring app; network edition; e-commerce edition | Upper editions expensive | [S] | Ouedkniss [d56562698] |
| 52 | **DS SOFTWARE / DS RAPIDE / DS MARKET** | Seller (Ghriss, Mascara) | Windows single or multi-PC | 15,000 each | AR/FR/EN | Superette, wholesale, butcher (by-weight edition) | **Keyboard-only till** with reprogrammable keys (F1 hold, F2/F3 quantity, F4 search, F5 delete line, F6 clear, F9 pay, F10 history, F12 cancel, Ctrl+D discount, Ctrl+R return, Ctrl+C customer, Ctrl+B label); remaps keys to match your old software; several open tickets, merge tickets, free price, advances; carton/pack/unit wholesale prices; dense light/dark panels; Excel/CSV import/export; installation, product entry and training included | Listing information only | [S] | Ouedkniss [d57221382] · [d57221406] |
| 53 | **FA_Gstock** | Seller (Bougara) | Windows single PC | 10,000 | **FR only** | General retail | Serial tracing; 100 parallel tabs; 13 reports + 3 dashboards; licence re-issued within 3 years if the PC changes | French only | [S] | Ouedkniss [d27434885] |
| 54 | **FlamePOS** | FlamePOS (Alger) | Windows | 5,000 per year; 10,000 for 3 years; 15,000 for 5 years; 20,000 LT | AR/FR/EN | General retail | Theme customisation; time-limited or lifetime keys | Thin details | [S] | Ouedkniss [d55784365] |
| 55 | **Flampex** | Flampex (Hussein Dey) | Windows 32- and 64-bit | 8,500 LT; 7-day trial | AR | General retail, phone repair | Repairs module; runs on 32-bit Windows | Thin details | [S] | Ouedkniss [d57312381] |
| 56 | **WR manager** | Seller (Aïn Oussara, Djelfa) | Windows | 19,999 | AR | General retail | 3 independent till screens; can sell into negative stock; 18,073 views | Thin details | [S] | Ouedkniss [d34059266] |
| 57 | **Tajir** | Seller (Oran) | Windows | 12,000 | AR | General retail | 4 price levels; "smart purchase advice"; 10,634 views | Thin details | [S] | Ouedkniss [d22296334] |
| 58 | **Superette-Grossiste** | Seller (Oran) | Single PC / LAN | 9,900 | AR/FR | Superette, wholesale | **SMS to debtors; SMS when a wanted product is back in stock**; CCTV support; 12,923 views | Thin details | [S] | Ouedkniss [d18778898] |
| 59 | **ATS POS 3.20** | Seller (Annaba) | Multi-till with offline sync | 8,900 | FR | Superette | Reads scale barcodes; sensitive-action controls; tills sync after going offline | A buyer: "I don't think an 8,900 DA program is serious" | [S] | Ouedkniss [d57939003] |
| 60 | **Bou Saâda till** | Seller (M'Sila) | Windows, weak PCs | 8,000; 14-day trial | AR/FR/EN | General retail | Built for weak PCs; installed over AnyDesk | Thin details | [S] | Ouedkniss [d55290118] |
| 61 | **SmartStock** | Seller (Alger) | Windows | 40,000 | FR/AR/EN | General retail | Local data only | Expensive for its segment | [S] | Ouedkniss [d55281649] |
| 62 | **Multisphere COM Silver** | Seller (Sétif) | Multi-user | 25,000 | FR/AR | General retail | Debt-collection module | Thin details | [S] | Ouedkniss [d56573309] |
| 63 | **GDS PRO** | Digideel (Bab Ezzouar); also promoted by Yellow Store DZ on Facebook | LAN | 25,000 | FR/AR | General retail | Claims 3,000+ clients | Thin details | [S] | Ouedkniss [d57517611] |
| 64 | **Orbis** | Seller (Chéraga) | Web + Android | 15,000 | FR/AR | Wholesale, van sales | Wholesale pre-sale offline; delivery tours | Thin details | [S] | Ouedkniss [d56087176] |
| 65 | **ShopLine / CoffeLine / RestoLine** | Seller (West; Chlef) | Single PC / LAN | 18,000 | FR | Retail, café, restaurant (many vertical editions) | Several barcodes per product; many vertical editions | French only | [S] | Ouedkniss [d25951270] · [d25918343] |
| 66 | **Shoperra** | Seller (Oran) | Windows + ready-made website | 59,000 | AR | Retail, e-commerce | Stock + ready-made website | Expensive | [S] | Ouedkniss [d51024226] |
| 67 | **El Eulma gestion commerciale** | Seller (El Eulma, Sétif) | n/p | "Affordable" | FR | General retail | Staff management; **état 104, G50, Zakat** | Thin details | [S] | Ouedkniss [d29886941] |
| 68 | **Sétif gestion commerciale** | Sétif | n/p | 25,000 | n/p | SMEs | Cheque/traite due dates, dunning letters, multi-bank treasury, bank reconciliation "format AFB", monthly TVA declaration, chart-of-accounts export, lots/serials, composites, PMP | Claims unverified | [S] | Ouedkniss [d32976718] |
| 69 | **GECO SOFT** | Sétif | Windows XP → 10 | 40,000 | n/p | SMEs | Multi-company; exports Excel, DBF, TXT, RTF; dead stock, expired stock, product images; 11,373 views | Legacy platform | [S] | Ouedkniss [d16024589] |
| 70 | **BNsoft** | BNsoft (TikTok) | Desktop (Windows assumed) | Lifetime, free updates (n/p) | n/p | 13+ verticals | 4 price levels; labels; held tickets; debts; phone repairs; profit per period; **Zakat in one click**; expired-products list | Social-media information only | [S] | https://www.tiktok.com/@bnsoft_/video/7500743518688742662 |
| 71 | **"المتجر" (Al-Metjar)** | Forums / Ouedkniss | Windows (assumed) | n/p | AR ("100 % Arabic") | General retail, wholesale | Returns and damaged goods; retail vs wholesale auto price; debts; opening balances; favourite/blacklist; expenses; net profit after expenses; Zakat; automatic archiving | Details unverified | [S] | Ouedkniss [d13827110] |

### 2.4 SME desktop POS: thinly documented offers

Names, prices and one-line claims only. They matter for the price map and for the "import from my old software" wizard.

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 72 | **Stock Standard** | Net Informatique Services (Blida) | n/p | 18,000 | FR | General retail | Full purchases, sales, stock | Thin details | [S] | Ouedkniss [d21919164] |
| 73 | **KIO POS / kiopos** | Sellers (Alger) | n/p | 52,900 or 15,000 | AR/FR | General retail | "Delivery to 69 wilayas" | Two very different prices | [S] | Ouedkniss [d55740700] · [d56045476] |
| 74 | **OMNI CAISSE PRO** | Seller (Alger) | n/p | 12,000 LT | FR | General retail | Expiry alerts | Thin details | [S] | Ouedkniss [d55706998] |
| 75 | **Dahabia POS** | Seller (Laghouat) | n/p | 15,000 (title) / 35,000 (body) | n/p | Superette, weighed goods | Weighed goods; credit | Inconsistent price | [S] | Ouedkniss [d56301202] |
| 76 | **Gestion POS** | Seller (Tlemcen) | n/p | 26,000 | n/p | General retail | Credit; permissions | Thin details | [S] | Ouedkniss [d57957894] |
| 77 | **GESTOCK** | Seller (Khemis El Khechna) | n/p | 12,000 | AR/FR | Warehouses | Warehouse focus | Thin details | [S] | Ouedkniss [d57732597] |
| 78 | **SuperettePro** | Seller (Koléa) | Windows 7, old PCs | n/p | FR | Superette | Runs on Windows 7 and old PCs | Legacy platform | [S] | Ouedkniss [d56299357] |
| 79 | **Arkan** | Seller (Annaba) | n/p | n/p | AR | General retail | – | No details | [S] | Ouedkniss [d55107943] |
| 80 | **iStock Pro 2026** | Seller | n/p | 12,000 | n/p | General retail | – | No details | [S] | Ouedkniss [d55690765] |
| 81 | **BzPos** | Seller | n/p | n/p | n/p | General retail | – | No details | [S] | Ouedkniss [d51405412] |
| 82 | **GestaCaisse** | Seller (Bouzaréah) | n/p | n/p | n/p | General retail | 6,854 views | No details | [S] | Ouedkniss [d18252240] |
| 83 | **AMSOLUTION** | Seller | n/p | n/p | n/p | General retail | – | No details | [S] | Ouedkniss [d28927989] |
| 84 | **"État 204" program** | Seller (El Attaf) | n/p | 5,000 | AR | Fiscal statements | Produces the "état 204" (meaning to confirm, §11) | No details | [S] | Ouedkniss [d57434988] |
| 85 | **Web PHP/SQL Server management suite** | Seller (Khraïssia) | Web on SQL Server | 200,000 per pack | n/p | SMEs | Purchases, sales, stock, till | Top of the price range | [S] | Ouedkniss [d27522066] |
| 86 | **"Logiciel de vente… plus de 10 000 clients"** | Bab El Oued, Algiers | n/p | 30,000 | n/p | General retail | Claims 15 years and 10,000+ clients; 21,631 views | Claim unverified | [S] | Ouedkniss [d20975347] |
| 87 | **Logiwell** (+ Logiwell Resto) | Tipaza | n/p | 15,900 | n/p | Retail, wholesale, restaurant | Free trial install and training; COD delivery to all wilayas | Thin details | [S] | Ouedkniss [d50074436] · [d26432450] |
| 88 | Ghardaïa gestion commerciale | Ghardaïa | n/p | 35,000 once | n/p | Restaurants, cafés, superettes, cosmetics, clothing, phones | Install + training | Thin details | [S] | Ouedkniss [d54491714] |
| 89 | **Supersoft** | Bab Ezzouar / Constantine / Oran | n/p | n/p | n/p | Retail/wholesale, fast food, street vendors (mobile app), instalment shops, fuel station, dégraissage, gym, dental, production | Wide range; 4,734 views | Thin details | [S] | Ouedkniss [d26141727] |
| 90 | **TradMan** | Algiers centre | n/p | n/p | n/p | "Tout domaine du commerce et prestation de service" | 12,518 views | Thin details | [S] | Ouedkniss [d11537219] |
| 91 | **UniSoft** | Dar El Beida | n/p | n/p | n/p | General retail | Range of commercial software | Thin details | [S] | Ouedkniss [d15825713] |
| 92 | **YBServices** | Dar El Beida | n/p | n/p | n/p | Superette, cosmetics, tabac, restaurant, clothing, appliances, spare parts, tea room | All-in-one POS | Thin details | [S] | Ouedkniss d24408171 |
| 93 | **CPOSdz** | Facebook | n/p | n/p | n/p | Superette | Software + hardware; e-commerce | Thin details | [S] | https://www.facebook.com/cposdz/ |
| 94 | **Dimastore** | Ouedkniss | n/p | n/p | "3 languages" | General retail, SMEs | POS + SME management | Thin details | [S] | fibladi mirror of Ouedkniss 15722662 |
| 95 | **Stock X** | Newsoft DZ | Windows | n/p | n/p | General retail | Stock & invoicing v10, all documents | Site now 404 | [S] | https://ar.newsoft-dz.com/ |
| 96 | **Basmatec SILVER** | Basmatec | n/p | n/p | n/p | Hardware stores | Multi-register invoices, multi-barcode | Thin details | [S] | https://www.basmatec.com/?page_id=145 |
| 97 | **Lamacta** | Lamacta | n/p | n/p | n/p | General retail | POS with daily closure and payment breakdown | Its blog repeats legal claims the decree doesn't contain | [V]/[S] | https://lamacta.com/blog/logiciel-caisse-algerie-commerce/ |
| 98 | **YASSNA-POS** | YouTube | n/p | n/p | n/p | General retail | "Strongest retail management program in Algeria" (2022–2023 versions) | Titles only | [S] | YouTube 0Bdx2hPSNYs |
| 99 | **POWER PRODUCTION** | YouTube | n/p | n/p | n/p | Production workshops, shops, warehouses | – | Titles only | [S] | YouTube lotrFNyo6Qo |
| 100 | **DIGISOFT GOLD** | YouTube | n/p | n/p | n/p | General retail | Shop management, "gold edition" (Aug 2025) | Titles only | [S] | YouTube sRxZ9EgcsrM |
| 101 | **Infinity (إنفينيتي) AI** | YouTube | n/p | n/p | AR | Supermarket | "Fastest" supermarket POS with AI (Feb 2026) | Titles only | [S] | YouTube wvBpdljTh9Q |
| 102 | **LinkSoft** | Facebook | n/p | n/p | n/p | General retail | Gestion commerciale | Titles only | [S] | https://www.facebook.com/LinkSoftLogiciel/ |
| 103 | **Logiwal** | LSCDEV | n/p | n/p | n/p | "Every trade" | "Smartest" shop program | Titles only | [S] | Facebook video |
| 104 | Store-management "pack 2023" bundle | YouTube | n/p | n/p | n/p | General retail | Bundle of programs | Titles only | [S] | YouTube U7rvBiu_TSo |

### 2.5 Hardware integrators bundling software (segment C)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 105 | **ITRI range and PME PRO / SMART packs** (ITRI-POS vêtement, pièces détachées, quincaillerie, pressing, bijouterie; ITRI RESTO PRO) | Digital Company / General IT (Mohammadia, Kouba; hardware distributor) | Windows, 1 server + clients, monoposte or multi-caisses | Packs, on request | FR + AR | Clothing, auto parts, hardware stores, pressing, jewellery, restaurant, production, import | Vertical editions sold with hardware; GMAO, production, import; Excel/PDF export; action history; also sells a Chifa card reader | Prices hidden | [V]/[S] | https://www.general-it.dz/ · Ouedkniss [d50572699] · [d38319701] · [d46334731] · [d50571559] |
| 106 | **Asalix / Auradev packs** | Asalix (Alger) | Touch terminal + peripherals + licence | From 24,500 (promo 20,000) | AR/FR | 8–9 packs, one per vertical | 90 days of remote support | Thin software details | [S] | Ouedkniss [d30782671] · [d35537273] |
| 107 | **FASTSHOP** | Tpv Technologies (Kouba) | Touch terminal + licence | n/p | n/p | Superette, retail; pressing (§2.12) | 16,153 views | Price hidden | [S] | Ouedkniss [d11018311] |
| 108 | **Danisoft packs** | Danisoft | i3 touch all-in-one + printers + drawer + scanner + licence | 118,000–133,000 | n/p | General retail | Complete turnkey pack | Expensive | [S] | Ouedkniss [d44928828] |
| 109 | **Expert POS** | Expert POS (Blida) | Hardware + licence | n/p | n/p | General retail | "Logiciel de gestion de stock" listing with 12,278 views | Price hidden | [S] | Ouedkniss [d12373295] |
| 110 | **Ecopos** | Ecopos | Hardware + licence | n/p | n/p | General retail | – | No details | [S] | Ouedkniss [d40287712] |
| 111 | **CaisseKom** (mini-superette, pâtisserie, bijouterie, boucherie, restaurant, salle de sport) | Computime (Oran) | Windows; sold with hardware | Mini-superette 25,000; pâtisserie 30,000; others on request | FR | Superette, bakery, jewellery, butcher, restaurant, gym | One engine, many vertical editions; gym listing 10,623 views; mini-superette 10,738 views | Thin details | [S] | https://computime.store/ · Ouedkniss [d14435454] · [d45414283] · [d45454081] · [d45414436] · [d14437207] |
| 112 | **HiroSystem** | Birtouta | Starter / Pro / Premium hardware packs | Packs, n/p | AR | General retail | Sale by barcode and name; daily and monthly reports | Thin details | [S] | Ouedkniss [d52265413] |
| 113 | **Idipos / CAISSE PRO** | Seller (Rouiba) | Sold with a PC | 85,000 with PC | FR | General retail | Turnkey | Thin details | [S] | Ouedkniss [d26735464] |
| 114 | **Eurequat POS** | Eurequat Algérie | Hardware + POS software | n/p | n/p | Retail | POS offer from an established hardware integrator | Thin details | [V] | https://www.eurequat-algerie.com/point-de-vente-pos/ |
| 115 | **OptiQ** | OptiQ | Hardware shop bundling licences | n/p | n/p | Retail | Named by 01 as a channel | No details | [S] | 01 §2, §6 |

Other hardware shops that act as channels: Lamwas tech, Groupe SPV, Chrono Informatique (GFS). Typical bundled hardware prices are in §5.

### 2.6 Cloud / SaaS and ERP-based (segment E)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 116 | **Almawarid** | Issal SARL (Bir El Djir, Oran) | Web + Android + Windows desktop (offline/LAN) | Free plan (§2.1); **Standard 20,000/yr** (100 documents/month); Enterprise (500 documents/month); 14-day refund guarantee. Live page on 2026-09-28: "Tarif sur demande" with caps (e.g. 50 documents/month, 1,000 products); a listing says "dès 15,000 DA/an"; Ouedkniss "complete pack" 33,000; lifetime desktop licence available | FR/AR/EN/TR/ZH | 41 professions; SEO pages per trade and wilaya | Invoices to DGI rules with NIF, VAT, timbre and QR; credit notes that correct VAT; PDF/WhatsApp/email sending; 80 mm receipts, till sessions, billetage; ticket Z "conforme DGI"; lots, expiry, CMUP; decimal quantities; QR labels; Excel/CSV import; price tiers per customer type; CNAS/IRG payroll; hosted in Algeria; **free migration from Sage, EBP, PC Compta, Ciel, Odoo or Excel within 48 h**; warns publicly against cracked software | **80 mm only** (58 mm prints cut off); needs a Windows driver; **labels 40×30 only**; **no scale support**; no TPE integration; drawer only in the desktop app; document caps | [V] | https://almawarid.app/ · https://almawarid.app/tarifs · https://almawarid.app/logiciel-gestion-commerciale-algerie/ · Ouedkniss [d56860799] · [d56993778] · 05 §1.2, §2.2, §4.1 |
| 117 | **Codeshade POS** | Codeshade | Web, "works offline" | Starter 7,000 (1 POS, 500 products); Growth 9,000 (5,000 products); multi-store on quote. **Billing period unclear:** 01 reads "per month", 02 "one-time" (§10) | AR (Darija)/FR/EN | General retail, multi-site | Debts; daily movements; real-time profit; multi-location overview | Product caps; cloud-first | [V] | https://codeshade.app/ |
| 118 | **LogiTools DZ** | LogiTools | Web | 15,000/yr; +5,000 for product data entry; 1 month free | FR + Darija assistant | General retail | AI invoice photo → stock; waste and theft log; lots; customer and supplier credit; **crossed-out promo prices**; roles; "tamper-proof fiscal register"; French/Darija AI assistant | Subscription; cloud | [V] | https://www.logitools-dz.com/ |
| 119 | **Symloop** | Symloop (Sétif) | Offline with sync | 35,000 / 65,000 / 120,000 per year incl. install and training. Hardware: touch till 85,000, printer 25,000, drawer 15,000, scanner 12,000, tablet 35,000, kitchen display 45,000 (bundle ≈ 137,000) | AR/FR | Restaurant + commerce | Kitchen display; time-based pricing; SMS loyalty and promos; API; multi-payment incl. credit | Yearly fees | [V] | https://symloop.com/fr/blog/logiciel-caisse-pos-restaurant-commerce-setif-2026/ · https://symloop.com/ar/blog/logiciel-caisse-pos-restaurant-commerce-setif-2026/ |
| 120 | **SouqDzayer** | SouqDzayer | Web | n/p | FR (site) | Retail, B2B resellers | Stock with variants, multi-warehouse, **reseller portal**, customer catalogue, orders, quotes, returns, invoices, POS | Web-only; price hidden | [V] | https://souqdzayer.com/fr/logiciel-gestion-commerciale-algerie |
| 121 | **CSI-Stock** | Codage Solution Industrie (Birkhadem) | Web app on a local or cloud server | 150,000 | FR | Multi-store retail | Multi-store; dark mode | Expensive | [S] | Ouedkniss [d56061973] |
| 122 | **FlustockX** | FlustockX (pan-African) | Android, iOS, Windows, macOS, web | n/p | n/p | General | Multi-platform | No prices; not Algeria-specific | [V]/[U] | https://flustockx.com/blog/logiciel-de-gestion-de-stock-algerie/ |
| 123 | **Managely** | Managely | Cloud + offline POS | 7-day trial; no per-user fees | AR (+ n/p) | Retail, services | Keyboard-only POS; WhatsApp automation; e-shop; delivery; booking; SCF accounting | Cloud dependency | [V] | https://managely.cloud/ar/best-pos-software-in-algeria |
| 124 | **DZ Compta** | DZ Compta | Cloud | n/p | AR (+ n/p) | General | Inventory, valuation, stock-out alerts, multi-warehouse | Cloud only | [S] | https://dzcompta.cloud/ar/stock/ |
| 125 | **Tassyir** | Tassyir | Web | 1,900 / 4,900 / 9,900 per month | n/p | E-commerce | E-commerce order flow, delivery integration, staff permissions | Not an in-store POS | [V] | https://tassyir.io/ |
| 126 | **EP Digital** (Odoo) | EP Digital | Web (Odoo) | n/p | n/p | Hardware stores | Unit conversions pièce/boîte/carton/mètre/kg | ERP complexity | [V] | https://epdigitaldz.com/logiciel-gestion-stock/quincaillerie/ |

### 2.7 Invoicing-only SaaS (segment F)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 127 | **Fisira** | Fisira | Web | 11,900 / 22,000 / 35,000 per year TTC | n/p | Services, self-employed | NIF/NIS/RC/AI; VAT; **automatic timbre**; **automatic G50**; AI scanning; accounting; e-signature and QR ("2026 compliance" claim) | No stock or till; compliance claim unverified | [V] | https://www.fisira.app/ |
| 128 | **eFawtara** | eFawtara | Web | n/p | n/p | Services, B2B | RC/NIF/NIS/AI; VAT 19/9/exempt; digital stamp; **sequential numbering that can't be changed**; QR and signed XML invoices; SATIM card payments; audit trail | No POS | [V] | https://efawtara.com/fr/facturation |
| 129 | **Costy** | Costy | Web | n/p | n/p | Services | Publishes an invoicing guide (legal mentions, timbre tiers) | Invoicing only | [V] | https://costy.app/facturation-en-algerie-en-2025-guide-complet/ |
| 130 | **Factury** | Factury | Web | n/p | n/p | Services | Cited for the mandatory invoice mentions | Invoicing only | [S] | 01 §4.5, §11 |
| 131 | **Fatoura** | Fatoura | Web | n/p | n/p | Services | – | Invoicing only | [S] | 01 §2, §4.5 |
| 132 | **InnovaInvoice** (innova-invoice) | Seller (Bir El Djir, Oran) | Web | n/p | n/p | Services | Online invoicing | Invoicing only | [S] | Ouedkniss [d55015843] |
| 133 | **DigiFact** | Seller (Tlemcen) | n/p | 4,900 | n/p | Services | Cheap invoicing tool | Invoicing only | [S] | Ouedkniss [d56709041] |

### 2.8 Restaurant and café specialists (segment D)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 134 | **Restorium** | Cirtasoft | Windows touch + tablets; KitApp | Starter **25,000 HT**; Pro and Enterprise on request | FR/EN/AR | Restaurant, café | Orders shown on a TV; KitApp kitchen app; automatic recipe deduction; waiter mobile ordering; BI; **160+ access rights**; multi-establishment; **QR code per table**; queue display; bill splitting; daily closing | Upper tiers priced on request | [V] | https://www.cirtasoft.com/logiciel-restaurant/en/ |
| 135 | **FoodMax** | Seller (Oran) | Windows | 18,000 | n/p | Restaurant, pizzeria | Tables; cash, kitchen and table tickets; add-ons; expenses; **customer credit with limit**; refunds; 6,826 views | Thin details | [S] | Ouedkniss [d24009309] |
| 136 | **Msk Ozone** | Seller (Tlemcen) | Windows touch | 35,000 | n/p | Restaurant, café | Stock, staff, rotation by hour; **cloud backup**; key survives formatting | Thin details | [S] | Ouedkniss [d30642759] |
| 137 | **El Eulma resto system** | Seller (El Eulma) | Windows + phones + TV | Basic 15,000 LT (max 2 printers); Normal from 25,000; menu entry service 3,500 | n/p | Restaurant, café | Normal adds unlimited printers, **QR menu**, **waiter phone app**, **orders on TV**, 6 months of support | Printer cap in Basic | [S] | Ouedkniss [d57659580] |
| 138 | Sétif resto / shop system | Seller (Sétif) | Windows touch + phone view | 35,000 | n/p | Restaurant, retail | Cooking modes and kitchen notes; prep tickets; **order taking from a touch remote**; floor plan; **home delivery**; size/colour; loyalty; view from a phone over the internet | Thin details | [S] | Ouedkniss [d39031109] |
| 139 | Oran resto / pizzeria system | Seller (Oran) | Windows + phones/tablets | From 38,000 | n/p | Restaurant, pizzeria | Waiter orders on a phone; chef receives on a phone or tablet | Thin details | [S] | Ouedkniss [d39658646] |
| 140 | POS Restaurant (Draria) | Seller (Draria) | Windows + website + app | 39,000 | n/p | Restaurant | **Online ordering (site + app)**, card payment, **table booking**, **caller ID**, email confirmations | Thin details | [S] | Ouedkniss [d56978620] |
| 141 | **Menu Max** | Seller | Windows + tablets | 42,000 | n/p | Restaurant, café | Digital menu; QR or tablet ordering; kitchen display | Thin details | [S] | Ouedkniss [d48086740] |
| 142 | Oran restaurant SaaS | Seller (Oran) | 10–15″ tablet + Bluetooth printers | Subscription | n/p | Restaurant, fast food | **Self-order kiosk**; order status board | Subscription; tablet-bound | [S] | Ouedkniss [d57473655] |
| 143 | **Kiober Food POS** | Kiober (Oran) | Windows; **Microsoft Store** (30-day trial) | 12,000 LT | n/p | Restaurant, café | Tables, counter and delivery; reseller programme | Thin details | [S] | Ouedkniss [d55642837] |
| 144 | Restaurant system pack | Seller (El Mouradia) | Screen + ticket printer + software | 170,000 | n/p | Restaurant | Turnkey pack | Top of the price range | [S] | Ouedkniss [d45540038] |
| 145 | **Clyo Systems Algérie** | Kouba | Touch caisse sold with hardware | 120,000–160,000 with hardware | n/p | Restaurant, fast food, cafeteria, bakery/pastry, butcher, fish shop, superette, beauty institute | Remote monitoring; also sells dot-matrix printers; 13,774 views | Expensive | [V]/[S] | https://www.clyosystems.dz/ · Ouedkniss [d6203275] · [d55066585] |
| 146 | **RESTO-PRO** | PRO SOLUTION (Bab Ezzouar) | Windows | n/p | FR | Restaurant | 12,304 views | Price hidden | [S] | Ouedkniss [d18723112] |
| 147 | **PRO_Resto** | Facebook page | n/p | n/p | n/p | Restaurant | – | Page not readable | [S] | https://www.facebook.com/logiciel.proresto/ |
| 148 | **Protid Pro Restaurant** (+ DSD / Cash Van) | Protid | n/p | n/p | n/p | Restaurant, van sales | Reservations, orders, kitchen tickets, menus, stock, mobile terminal; van sales | Thin details | [S] | https://www.protid.com/logiciels-algerie/gestion-restaurants-en-algerie/ |
| 149 | **Smart Resto** | arabusinesse.com | n/p | n/p | n/p | Restaurant, café, tourist parks | Waiter app | Site down | [S] | https://arabusinesse.com/ |
| 150 | **Bistro** | bistro.icu | n/p | n/p | n/p | Restaurant | Restaurant management | Site down | [S] | https://www.bistro.icu/ |
| 151 | **Ultimate FERP** | S2I | n/p | n/p | n/p | Restaurant | Dining room ↔ kitchen link | Thin details | [S] | 02 §3.4 |
| 152 | **RestPlus** | SII | n/p | n/p | n/p | Canteens, restaurants, hotels | – | Thin details | [S] | https://www.solutionsinformatiques.dz/ |
| 153 | **iThreeWeb** | iThreeWeb | n/p | n/p | n/p | Restaurant | Restaurant management | Thin details | [S] | https://ithreeweb.com/ |
| 154 | Tébessa resto / café / pâtisserie | Tébessa | PC bundle | 85,000 | n/p | Restaurant, pastry | Restaurant + pastry | 6 views | [S] | Ouedkniss [d57871113] |

Restaurant editions of general products: Crescendo Restauration (§2.1), CaisseKom Resto (§2.5), ITRI RESTO PRO (§2.5), RestoLine/CoffeLine (§2.3), Logiwell Resto (§2.4), INNOVA POS (§2.2), Symloop (§2.6).

### 2.9 Pharmacy specialists

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 155 | **PharmaPro** | tkawen | Windows 10/11, SQLite | **45,000 LT**; 15-day trial | AR/FR | Pharmacy | Chifa files + rejection tracking; UPROMEDIC supplier import; FEFO; search by DCI; loyalty; Telegram alerts; e-invoice JSON "ready for DGI" (vendor claim) | 14-day offline licence grace (licence check) | [V] | https://pharmapro.tkawen.com/ |
| 156 | **sCom_pharm pro** | Seller | Windows | 120,000 | n/p | Pharmacy | **CHIFA/CAMSSP export**; prescription register; officinal preparations; search by generic name (DCI) | Expensive | [S] | Ouedkniss [d31231999] |
| 157 | **PharmaX Connect** (+ PharmaCom, PharmaLab) | Inabex | Windows | On quote | FR/EN/AR | Pharmacy, pharma distribution, labs | CHIFA; barcode; loyalty; PharmaCom: distributor with e-invoice | Price hidden | [V] | https://inabex.com/logiciels-de-gestion-en-algerie/ |
| 158 | **Shabakett Officine** | Shabakett | n/p | 80,000 / 100,000 / 150,000 | n/p | Pharmacy | Real-time CNAS/CASNOS bordereaux; direct invoice import; > 40 privileges | Expensive | [V] | https://www.shabakett.com/products/shabakett-officine |
| 159 | **Pharmacium** | Cirtasoft | n/p | 80,000–200,000 **per month** + TVA (as read) | FR/EN/AR | Pharmacy | Prescriptions, interactions, renewal alerts; "Scaneye" AI OCR; FEFO | Very expensive | [V] | https://www.cirtait.com/pharmacium/ |
| 160 | **XpertPharm** (+ XpertAssist) | Xpertsoft | SQL Server, tabs, remote access | n/p | n/p | Pharmacy | Assisted Chifa entry by barcode or price; order assistant | Price hidden | [V] | https://xpertsoft-dz.com/xpertpharm.php |
| 161 | **Silwane Signature** | IntelliX | n/p | n/p | FR/AR/EN | Pharmacy | "100 % barcode"; UG; seller audits; customer display | Price hidden | [V] | https://www.intellixgroup.com/ar/logiciel-pharmacie-silwane-signature |
| 162 | **CTI Pharm** | CTIB | n/p | n/p | n/p | Pharmacy | User manual published | Thin details | [S] | CTIB manual PDF |
| 163 | **Vetosoft** | n/p | n/p | 45,000 | n/p | Veterinary clinics | Clinic software | Not a shop POS | [S] | 09 §4.5 |

A Thales CHIFA card reader sells for 19,500 DA on Ouedkniss (01 §10).

### 2.10 Phones, repair, flexy, instalments, optics

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 164 | **KERS FLEXY** | KERS (Souk Ahras) | Windows | 25,000 | AR/FR | Flexy, phone shops | Mobilis, Djezzy, Ooredoo top-ups (API or USSD, operator auto-detection) plus repairs | Thin details | [S] | Ouedkniss [d56463136] |
| 165 | **Easy Life Flexy v5.4** | Seller | Windows | 13,000 | n/p | Flexy, kiosks | Mobilis bill payment; Ooredoo Maxy | Thin details | [S] | Ouedkniss [d19431931] |
| 166 | **FASTransfer** | Seller | Windows | 20,000 | n/p | Flexy | Flexy transfers | Thin details | [S] | Ouedkniss [d39656135] |
| 167 | Flexy through a USB modem | Seller (Hammam Bouhadjar) | Windows + USB GSM modem | n/p | n/p | Flexy | Sends top-ups through a modem | Thin details | [S] | Ouedkniss [d9607035] |
| 168 | **Dukan Pro** | Seller (Alger) | Windows; 3.24 MB installer | 9,000 | AR | Phone repair | **Technician PIN**; audit log; tiny installer | Thin details | [S] | Ouedkniss [d57584481] |
| 169 | **Vyvex** | Vyvex | n/p | n/p | AR (+ n/p) | Instalment sales, phones | Instalment contracts and schedules; cash/CCP/direct-debit collection; client scoring; IMEI/serial; multi-location; 4 roles | Price hidden | [V] | https://www.vyvex.app/ar |
| 170 | **GesRep** | Natrix-soft | n/p | n/p | n/p | Phone and PC repair | Intake slip, device state, parts, net profit per repair | Site 404 | [S] | https://www.natrix-soft.com/2019/11/gesrep.html |
| 171 | **DzPhoneHub** | Algiers | n/p | n/p | n/p | Phone shops | IMEI verification, repair tracking, POS, customers, payments | Thin details | [S] | https://dzphonehub.online/ |
| 172 | "Flash" repair software | dzgsm forum (Aug 2026) | n/p | n/p | n/p | Repair | Repair management | Forum post only | [S] | https://dzgsm.com/vb/threads/96766/ |
| 173 | Phone-shop management system | Boumerdès | n/p | 35,000 | n/p | Phones, electronics | Dedicated phone/electronics shop system | Thin details | [S] | Ouedkniss [d56350090] |
| 174 | **"Logiciel de Gestion de Facilité"** | Seller (Dar Chioukh, Djelfa) | Offline | 11,000 | AR/FR | Instalment sales | Customers, debts, automatic instalment schedules, arrears / late payments | Thin details | [S] | Ouedkniss [d52615477] |
| 175 | **NESOFT Optic Vision Manager** (+ Provider Vision Manager) | NESOFT | n/p | n/p | n/p | Optics, distributors | Patient file; frames, lenses, articles; devis → facture → commande; SMS/email; distributor edition | Price hidden | [V] | https://www.nesoft.dz/nosproduits.php |

Other products with a repair module: AlgoStock, Flampex, Zedkira Pro (§2.3). Buyers ask for "commercial management + repairs in one" on three listings (01 §8).

### 2.11 Auto parts and hardware stores

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 176 | **MEGA-AUTO** | Megasoft | n/p | n/p | n/p | Auto parts | Megasoft suite (§2.2), also sells payroll | Thin details | [S] | https://www.megasoft-office.com/logiciel-mega-auto/ |
| 177 | **Sphinx Manager** | n/p | n/p | n/p | n/p | Auto parts | Auto-parts management | Thin details | [S] | 02 §3.7 |

Dolisoft PRO (§2.3) is the auto-parts favourite; Basmatec SILVER (§2.4) and EP Digital (§2.6) target hardware stores.

### 2.12 Pressing, butcher, jewellery, gym, salons (found by 09 and 01; corrects 02)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 178 | Tpv Technologies "LOGICIEL PRESSING" | Kouba | Windows | 30,000 | n/p | Pressing | Purchases, sales, suppliers, customers, charges and losses, statistics; **11,902 views** | Thin details | [S] | Ouedkniss [d18280273] |
| 179 | REVEXN System (pressing) | Oran | n/p | 25,000 | n/p | Pressing | Cash sales, categories, customers; 5,496 views | Thin details | [S] | Ouedkniss [d25229062] |
| 180 | Pressing & blanchisserie software | Bab Ezzouar | n/p | 18,000 | n/p | Pressing | Orders and customer follow-up | Thin details | [S] | Ouedkniss [d56041582] |
| 181 | Pressing / dégraissage / laverie software | Constantine | n/p | n/p | AR (+ n/p) | Pressing | Receipt slips (الوصل), garment categories, service price list | Thin details | [S] | Ouedkniss [d30243863] |
| 182 | "Pack boucherie" | Birtouta | Touch PC + scanner + **electronic scale** + drawer + printer | n/p | n/p | Butcher | Lifetime software in the pack | Thin details | [S] | Ouedkniss [d53418317] |
| 183 | "Logiciel boucherie + PC" | Tlemcen | PC bundle | n/p | n/p | Butcher | Butcher bundle | Thin details | [S] | Ouedkniss [d57850917] |
| 184 | "Logiciel pour boucherie" | Bordj El Kiffan | n/p | n/p | n/p | Butcher | Butcher software | Thin details | [S] | Ouedkniss [d28923331] |
| 185 | Jewellery software | Blida | n/p | n/p | n/p | Jewellery | Gold/silver/stone stock; **broken-gold exchange against cash**; **melting**; credit; repairs; special orders | Thin details | [S] | Ouedkniss [d47974462] |
| 186 | Jewellery software | Batna | n/p | n/p | AR/FR/EN | Jewellery | **Automatic reprice when the gold price changes**; instalments | Thin details | [S] | Ouedkniss [d44602069] |
| 187 | LOG103 (gym) | Sétif | n/p | n/p | n/p | Gym | 8,248 views | Thin details | [S] | Ouedkniss d21165618 |
| 188 | Kinetic Gym | Tipaza | n/p | 15,000 | n/p | Gym | Gym management | Thin details | [S] | Ouedkniss [d57648717] |
| 189 | Gym Manager Pro | Oum El Bouaghi | n/p | 20,000 | n/p | Gym | Members, subscriptions, **RFID access control** | Thin details | [S] | Ouedkniss [d55843231] |
| 190 | Smart Gym | Bab Ezzouar | n/p | n/p | n/p | Gym | Gym management | Thin details | [S] | Ouedkniss [d40978507] |
| 191 | Titaou Gym Ultimate | Khenchela | n/p | n/p | n/p | Gym | Gym management | Thin details | [S] | Ouedkniss [d56592551] |
| 192 | Gym software (face recognition, RFID cards and tags) | Oran | n/p | n/p | n/p | Gym | Access control | Thin details | [S] | Ouedkniss [d56746856] |
| 193 | Gym software with ZKTeco (fingerprint/face/RFID) | Tlemcen | n/p | n/p | n/p | Gym | Access control with pointeuse | Thin details | [S] | Ouedkniss [d21116539] |
| 194 | Gym software with tripod turnstile | Birkhadem | n/p | n/p | n/p | Gym | Turnstile integration | Thin details | [S] | Ouedkniss [d45334057] |
| 195 | ProAccess | n/p | n/p | n/p | n/p | Gym with swimming pool | Access control | Thin details | [S] | 09 §4.4 |
| 196 | **ECHO** | Seller (Alger) | Windows | 8,000 | AR | Salons | Worker commission %; **WhatsApp ticket** | Thin details | [S] | Ouedkniss [d57214320] |

INNOVA Clean and INNOVA Beauty (§2.2) also serve dry cleaning and beauty salons.

### 2.13 Foreign ERPs through local partners (segment G)

| # | Product | Vendor / place | Platform | Price (DZD) | Languages | Verticals | Strengths | Weaknesses / gaps | Conf. | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| 197 | **Sage 100 / X3 / 100 Online / Paie** | BSC Algérie (El Achour), "1st Sage integrator in Algeria", Premium partner | Desktop / cloud | On quote (Sage 100 from about €192/month in Morocco) | FR (AR n/p) | Mid-size and large companies (Lafarge, Coca-Cola, Danone, Banque d'Algérie) | Brand and depth; many Ouedkniss ads offer Sage 100 training | Expensive, integrator-led; not a shop till | [V] | https://www.bsc-algerie.com/ · https://www.sage.com/fr-ma/produits/sage-100/gestion-commerciale/ · Ouedkniss [d28343846] |
| 198 | **Odoo** | ITHRYTEC (50+ companies; localised for DZD, VAT, national chart of accounts, payroll); Digitaliac (SCF, VAT, timbre, G50, payroll, **Yalidine integration**, POS; reseller programme); Digisys; Finoutsource; Numidit; directory partners (§2.15) | Web | On quote; projects up to 650,000 | FR/AR/EN | SMEs to large companies | Very broad; local localisation packages; an Ouedkniss Odoo listing has 18,508 views | ERP complexity; integrator dependency | [V] | https://ithrytec.com/odoo-algerie · https://www.digitaliac.com/integration-odoo · https://odoo-algerie.com/ · https://digisys.dz/odoo-algerie/ · https://www.odoo.com/partners/grade/ready-3/country/algeria-61 · Ouedkniss [d20319224] |
| 199 | **Dolibarr** | Doliflex (page fetch failed); independent installers | Web | n/p | FR (+ others) | SMEs | Open source | Presence unverified | [U]/[S] | Ouedkniss [d30720309] |
| 200 | **Gestimum** | NTIC-SOFTPRO (Azazga) | Windows, SQL Server | On quote | FR | SMEs | Multi-depot; serial and batch tracking; expiry; six valuation methods; BOM; EDI/PDF/XML | French product through an integrator | [V] | https://ntic-softpro.com/s_g.html |
| 201 | **EBP, Ciel, PC Compta** | No official Algerian reseller found | Desktop | n/p | FR | SMEs | Seen only as migration sources (Almawarid) and in job ads | Presence unverified | [U] | 01 §4.6 |

### 2.14 Android and mobile apps used in Algeria

| # | App | What it does | Conf. | Source |
|---|---|---|---|---|
| 202 | **Mahaal / Inyad** | Mobile POS: inventory, recipes, tables, delivery, multi-site, loyalty, gift cards, payroll, e-invoicing, QR ordering. Mahaal claimed "the most used POS in Algeria, 6,000+ merchants" and merged into Inyad (350,000 businesses in 40+ countries). Current Algerian pricing unverified (URLs point to Morocco) | [V]/[S] | https://www.mahaal.app/ar · https://inyad.com/ar/pos-system-software · Play `com.inyad.store` |
| 203 | **Konnash (Inyad)** | Digital credit notebook (كناش الديون) with payment reminders; free | [S] | Play `com.invyad.konnash` |
| 204 | **DZ POS** (NGC Technologies) | Order taking for sales teams, multiple open bills | [S] | Play `com.ngctechnologies.dzposAPP` |
| 205 | **Masroufi (مصروفي)** | Algerian BNPL grocery app (Djelfa, 2024): buy on credit until payday, deducted from Edahabia | [V] | echaab.dz, 2024-09-19 |
| 206 | **Flexy apps** (Flexy Pro, Flexy ici, Mobile ALG) | Mobile top-up wallets, Idoom and digital cards | [S] | https://flexy-pro.com/?lang=ar |
| 207 | Cashier – ادارة حسابات المحلات (Egyptian) | Shop accounts, barcode reader | [S] | Play `com.ahmedelshazly2020d.sales_managers` |
| 208 | Alcashier | Restaurant/store POS, offline | [S] | Play `com.alcashier` |
| 209 | Daftra POS | Cloud POS | [S] | Play `com.izam.daftraPos` |
| 210 | OmniPOS Restaurant | Restaurant/café accounts, product costing | [S] | Play `omnipos.restaurant.pos` |
| 211 | Debt managers (مدير الديون, Teajar) | Debt tracking, WhatsApp reminders, PDF statements | [S] | Play `com.centuryegg.pdm` · https://teajar.io/ |

Owner companion apps bundled with desktop products (AMPRO, MekaStore, AlgoStock, Zedkira Pro, Dipposoft, Superette-Grossiste) are listed with their products.

### 2.15 Other players that shape expectations

- **Foreign Arabic-language vendors** that rank first in Arabic searches but aren't Algeria-specific (ZATCA, EGP/SAR): **DEXEF, Daftra, Alcashier, Al Badr, Pioneers Easy Store, Kashier, Fatoorah, Vatoce, Siindbad** (02 §2.2, 01 §4.6). They set the expectation of one engine with vertical landing pages. **No Algerian localisation evidence** for any of them; Loyverse and Zoho Inventory likewise (01 §4.6, 09 §3.4).
- **Odoo integrators in Algeria** (official directory): Insidjam COM, SIBIC, EURL BUSINESNA, FINOUTSOURCE, Cetelex, AH Consulting, HORIA TECHNOLOGIES; plus ITHRYTEC, Digitaliac, Digisys, Numidit (not in the directory) (09 §3.4, 01 §4.6).
- **Standalone cash registers**: Casio SE-G1 (26,000 DA, 1,015 views), Sharp XE-A107 (25,000 DA, 1,345 views). No stock, credit or Arabic; they set the price a tiny shop pays for "a till" today (09 §3.3).
- **Global benchmark** (03): Odoo, Square, Lightspeed R/X/K, Loyverse, Shopify POS, Toast, Zoho, Clover, ERPNext, Dolibarr TakePOS, uniCenta, Chromis, Floreant, SambaPOS, Openbravo, Ordoro, KORONA, Hike, OSPOS, Epos Now (21 products). None combines free + offline + installable + deep gestion commerciale (03 §2).
- **Ouedkniss demand signals** (01 Appendix A, top 100 software listings by views): generic titles dominate ("Logiciel superette", "برنامج تسيير المحلات التجارية"), which shows buyers search by trade, not by brand. The top five are PME PRO (39,497), PRO SOLUTION superette/toys/clothing (23,838), an Arabic invoicing program at 5,000 DA (23,194), PRO SOLUTION PME/PMI (22,137) and an Alger-centre "point de vente" at 35,000 DA (21,331). Restaurant listings (14) and Arabic-titled listings (about 15) each take more than a tenth of the top 100.
- **Piracy** (01 §7): cracked WinDev 25 at 30,000 DA in 4 identical listings ([d51577181]); "logiciel + outils d'activation" ([d29117684]); "free" programs of unknown origin on Arabic forums (ahlamontada, 7olm). Almawarid warns that cracked desktop software gets no fiscal updates, brings viruses and legal exposure.

### 2.16 Count

**211 market entries** are listed above:

- **201 Algerian products and offers** (§2.1–§2.13): desktop, cloud, vertical, hardware packs and foreign ERPs sold through local partners. By section: free and near-free 8 (§2.1); established editors 19 (§2.2); detailed SME desktop offers 44 (§2.3); thinly documented SME offers 33 (§2.4); hardware integrators 11 (§2.5); cloud / SaaS 11 (§2.6); invoicing SaaS 7 (§2.7); restaurant and café 21 (§2.8); pharmacy 9 (§2.9); phones, repair, flexy, instalments, optics 12 (§2.10); auto parts 2 (§2.11); pressing, butcher, jewellery, gym, salons 19 (§2.12); foreign ERPs through local partners 5 (§2.13).
- **10 mobile apps** used in Algeria (§2.14). Several are foreign (Egyptian or regional).

**What changed with report 01 restored:** the 14 name-only placeholders of the previous version are now full rows, **100 products were added** from 01, the two INNOVA rows were merged, and 17 existing rows were enriched with 01 details (Stock Express, Crescendo, AlgoStock, KERS POS, Dolisoft, Almawarid, Codeshade, Symloop, CaisseKom, CLYO, GDS PRO, HiroSystem, Gestium, Onyx, Logiwell, Gestion de Facilité, Mahaal). The previous version listed 112 entries.

Around them sit 9 foreign Arabic vendors and 2 global products with no Algerian localisation evidence, 11 Odoo integrators, 2 standalone cash registers and 21 global benchmark products (§2.15). **52 of the 201 products are verified** ([V] or partly [V]) on a vendor page; the rest rely on listings and social media.

### 2.17 Pricing landscape (what "free" competes against)

**Ouedkniss price statistics** (01 §1, §5): software listings with a real price of 3,000 DA or more, **n = 194**.

| Measure | Value |
|---|---|
| **Median** | **18,000 DA** |
| Minimum | 3,500 DA |
| Maximum | 500,000 DA |
| Typical small-shop licence | 8,000–35,000 DA, paid once |

| Price band | Listings | Share |
|---|---|---|
| under 10,000 DA | 31 | 16 % |
| 10,000–20,000 DA | 72 | 37 % |
| 20,000–40,000 DA | 49 | 25 % |
| 40,000–80,000 DA | 21 | 11 % |
| 80,000–150,000 DA | 16 | 8 % |
| 150,000 DA and above | 5 | 3 % |

So **53 % of priced listings cost under 20,000 DA** and 78 % under 40,000 DA.

| Model | Range | Examples |
|---|---|---|
| **Free** | 0 DA | Klick Stock Express (limited), Crescendo POS Free (1 PC, 2 tills/users/files, French only), Almawarid free plan (3 invoices/month), Dipposoft free version |
| **Ultra-low, lifetime** | 3,500–10,000 DA | Smart Barcode 3,500 (launch); SolutionInfoDZ from 3,500; "Magasin 2026" 5,000; ECHO 8,000; Bou Saâda 8,000; Flampex 8,500; ATS POS 8,900; Dukan Pro 9,000; Superette-Grossiste 9,900; Klick Standard 9,900–10,000; FA_Gstock 10,000; KERS POS 10,000 |
| **Standard, lifetime** | 11,000–35,000 DA | Gestion de Facilité 11,000; Dolisoft 12,000 per PC; AMPRO / ProCommerce / OMNI / Kiober / Tajir 12,000; DS 15,000; Orbis 15,000; Logiwell 15,900; Dipposoft 16,000; Stock Standard 18,000; FoodMax 18,000; ShopLine 18,000; AlgoStock 19,500; WR manager 19,999; FlamePOS 20,000 LT; MekaStore 20,000; StockCom 20,000; Crescendo Pro 20,000–35,000; Quick Fact, GFS, SMART-Com, Multisphere, GDS PRO, KERS FLEXY, easykes, CaisseKom 25,000; ITSCOM 28,000; PRO-PME 30,000; CaisseKom pâtisserie 30,000; Zedkira 32,000; BBS, Smart Gestion, ITSpoint, Msk Ozone, Ghardaïa 35,000 |
| **Mid / pro, lifetime** | 38,000–70,000 DA | Oran resto 38,000; Draria resto 39,000; SmartStock 40,000; GECO SOFT 40,000; Menu Max 42,000; PharmaPro 45,000; Zedkira Pro/Business 49,000–65,000; MekaStore Pro 50,000; KIO POS 52,900; Shoperra 59,000; PME PRO 70,000; TifawtTech Supermarché 70,000 |
| **Editor packages** | 80,000–200,000+ DA | Shabakett 80,000–150,000; SmartCom 95,000 / 110,000 TTC; sCom_pharm 120,000; CSI-Stock 150,000; web PHP/SQL Server suite 200,000 per pack; Odoo projects up to 650,000 |
| **Hardware + software packs** | 20,000–170,000 DA | Asalix from 24,500 (promo 20,000); Idipos 85,000; Tébessa resto 85,000; WINCOM up to 120,000; Danisoft 118,000–133,000; CLYO 120,000–160,000; Symloop bundle ≈ 137,000; restaurant system 170,000 |
| **Yearly SaaS and add-ons** | 5,000–120,000 DA per year | FlamePOS 5,000/yr; AlgoStock add-ons 5,000–12,000; Fisira 11,900–35,000; LogiTools 15,000; Almawarid 20,000 (Standard; now "on request"); GestiumGo / Restorium 25,000 HT; Symloop 35,000–120,000 |
| **Monthly SaaS** | 1,900–200,000 DA per month | Tassyir 1,900–9,900 (e-commerce); Codeshade 7,000–9,000 (if monthly, §10); Pharmacium 80,000–200,000 (as read) |
| **Support and per-seat add-ons** | 3,500–30,000 DA | Crescendo support 10,000–30,000; Zedkira +4,000 per user, +8,000 per PC, +6,000 per phone; El Eulma menu entry 3,500 |

**Trial lengths** (01 §6): 7 days (AMPRO, MekaStore, Flampex, ProPOS), 14 days (Almawarid, Bou Saâda), 15 days (Zedkira, PharmaPro), 21 days (Smart Gestion), 30 days (Kiober via the Microsoft Store, IntelliX, Crescendo). **Refunds: none** ([d8059698]).

---
## 3. What users complain about, and our opportunities

| # | Pain or complaint | Evidence | DZBuild POS answer | Spec |
|---|---|---|---|---|
| 1 | Subscription fatigue and yearly add-on fees | Local pitches stress "بدون اشتراكات"; AlgoStock charges 5,000–12,000 DA/yr for mobile and AI; Symloop 35,000–120,000 DA/yr (02 §1, §6) | Free forever, no licence server, no paid tiers | PLT-002, PLT-003 |
| 2 | "Free" editions paywall the essentials | Klick Stock Express free lacks kridi, multi-depot, price tiers, inventory, reports, treasury (09 §3.1); Crescendo POS Free: 1 PC, 2 tills, 2 users, 2 company files, French only, activation key; Almawarid free: 3 invoices a month, no PDF (01 §1 (4)); Loyverse/Square paywall inventory, staff and history (03 §4 G1) | Everything free, including credit, multi-depot, price levels, counts, reports, treasury | CRD-*, STK-003, PRC-001, FIN-003 |
| 3 | Caps on products or documents | Codeshade: 500 or 5,000 products; Almawarid: e.g. 50 documents/month and 1,000 products (02 §2.1, 09 §2.2) | No caps | PLT-003 |
| 4 | Arabic tickets print as disconnected letters, reversed, or Chinese characters | Code pages differ by brand; printers don't shape Arabic; UTF-8 turns into CJK on Chinese firmware (05 §1.5) | Arabic receipts rendered as a raster image; per-printer profiles | HW-005, HW-006 |
| 5 | Cheap printers and 58 mm rolls not supported | Almawarid: 80 mm only ("58 mm prints cut off"), requires a Windows driver (05 §1.2) | 58, 80 and 76 mm; raw ESC/POS **and** driver fallback | HW-001, HW-002 |
| 6 | Only one label size | Almawarid supports only 40×30 labels (05 §2.2) | 11 presets + custom, TSPL/ZPL | LBL-001–003 |
| 7 | No scale support | Almawarid doesn't support scales; no local product advertises PLU push, live serial reading or carcass yield (05 §4.1, 09 §4.2) | EAN-13 20–29 parsing in MVP, serial scales V1, PLU push V2 | SAL-014, HW-030–033, VRT-011 |
| 8 | Scanner types `&é"'(` instead of digits | French AZERTY layout, Arabic layout left on by Alt+Shift (05 §3.2) | Layout-proof decoding by physical key codes; AZERTY-aware numeric entry | HW-021, UX-031 |
| 9 | Card payments not linked to the till | TPEs are standalone; Almawarid lists them as unsupported (05 §11) | Manual CIB/Edahabia tender with auth number and end-of-day reconciliation; ECR later if banks allow | PAY-002, PAY-003, CSH-008 |
| 10 | Cloud POS stops working offline | Loyverse hides stock offline; Square has no gift cards, loyalty or closing offline; Toast offline "has significant limitations" (03 §4 G2); nationwide internet shutdowns (05 §8.1) | 100 % local; nothing blocks on the network | DAT-001–003 |
| 11 | Data loss after power cuts; restore needs a technician | SambaPOS restore needs SQL Management Studio (03 G5); vendors sell UPS "for the small POS" (05 §8.2) | Crash-safe SQLite (WAL + FULL), verified automatic backups, one-click restore, move-PC wizard | DAT-004–013 |
| 12 | "La version démo ne s'installe pas" (the seller answered "try another PC") | Ouedkniss comment (01 §8, [d8059698]) | Offline full installer, portable mode, demo data | PLT-005, PLT-006, ONB-002 |
| 13 | Support is paid and remote (AnyDesk/TeamViewer install, WhatsApp support, paid support tiers of 10,000–30,000 DA, "we enter your products for you") | 01 §4.1, §6 | Offline help in AR/FR/EN, Darija videos with AR/FR/EN subtitles, WhatsApp support button, "Get help" remote session, Excel import templates, reseller programme | ONB-004, ONB-010–013, IMX-005 |
| 14 | Excel breaks barcodes in CSV | Square warns Excel mangles SKUs (03 G8) | XLSX-first import, leading zeros kept, dry run and undo | IMX-002–006, IMX-014 |
| 15 | Official Chifa software is poor | "Fails to track expiration dates, batch numbers… no inventory, no barcode"; 80 % of pharmacists dissatisfied (02 §3.5) | Lots/FEFO for parapharmacie in MVP; Chifa pack later | PRD-051–053, VRT-031–032 |
| 16 | Fictive-invoice and dual-stock features expose users to prosecution | Sold by several competitors; LF 2026 penalties for IT-enabled fraud (09 §5.5) | "Conforme, inaltérable" only; lawful stock states instead | AUD-003, STK-027, §40.2 |
| 17 | Guarantee cheques in instalment sales | Customers risk 2–5 years in prison for bounced guarantee cheques (02 §3.14) | Neutral guarantee documents, guarantor file, schedule | TMP-006, TMP-009 |
| 18 | Confusion about timbre and IFU rules | Blogs call the 2025 timbre "progressive" or still quote the old cap; blogs say IFU threshold 15 M DA (04 §4.2, §5.1) | Timbre engine per DGI circular 14/2025; IFU tracker at 8 M DA; rules shipped as updatable data packs | TAX-006, TAX-014, PLT-024 |
| 19 | Amount in words wrong for DZD | num2words spells "dinard" and has no Arabic DZD (04 §8.2) | In-house AR/FR/EN converter with unit tests | DOC-004 |
| 20 | Courier integrations break on the 58-wilaya list | 69 wilayas since April 2026; carriers lag (04 §12.2) | 69-wilaya dataset + 58 mapping | CFG-010 |
| 21 | No single product covers every trade | Specialists per vertical (02 §8) | One engine, vertical packs toggled per shop | §30 of the spec |
| 22 | Credit is basic in retail-first POS | Only Odoo, Lightspeed X and Hike have real on-account; Odoo's limit only warns (03 G7) | First-class kridi: ceilings, versements, statements, reminders, dunning | CRD-001–011 |
| 23 | Dated WinDev interfaces, cracked tools | 01 §7, §8; 07 §2.4 | Modern trilingual UI, touch and keyboard, dark theme | UX-* |
| 24 | Low literacy and older owners | Adult literacy 75.1 % (2008), 65+ = 6.8 % (09 §9) | "Mode simple", sound cues, big tiles, centimes display | UX-030, UX-034, UX-006 |
| 25 | DZBuild merchants can't export orders or variant stock | DZBuild exports only through Google Sheets, no variant stock (06 §1.4) | POS keeps the physical stock and syncs it to DZBuild | DZB-012–014 |
| 26 | Standalone cash registers track nothing | Casio/Sharp ECRs at 25,000–26,000 DA (09 §3.3) | Department-key mode on an 18,500 DA refurbished PC | SAL-019 |
| 27 | Hidden prices: "prix svp"; "What's the point if no pack has a price!!!" | Ouedkniss comments (01 §8) | Free, and says so everywhere | PLT-002 |
| 28 | Trials of 7–30 days, demos sent by email, password-protected downloads | 01 §6 | Full product, public download, no trial clock | PLT-002, PLT-004, PLT-005 |
| 29 | No refunds ("refund if not satisfied?" → no) | 01 §6, §8 ([d8059698]) | Nothing to refund; full data export if the shop leaves | PLT-002, IMX-015 |
| 30 | Scaling questions: "multi-stock?", "can I add a 2nd PC?", "how many articles?" | 01 §8 ([d11162776]) | Multi-depot, unlimited LAN tills and articles, all free | STK-003, LAN-001, LAN-011, PLT-003 |
| 31 | Several activities in one product: "commercial management + repairs" (3 listings), jewellery, packaging shop, storage-only depot, refrigerated truck with 20–50 products, instalment company | 01 §8 | One engine, module toggles and vertical packs; van stock | PLT-016, §30 of the spec, ORD-005, TMP-*, STK-038 |
| 32 | Distrust of cheap tools ("I don't think an 8,900 DA program is serious") | 01 §1 (13), §8 | Signed installer, public changelog, DZBuild brand, published 2027 conformity statement per version | PLT-009, PLT-026, AUD-011 |
| 33 | Runtime bugs typical of WinDev apps ("erreur dans propriété ligne 381") | 01 §8 (BBS) | Typed Rust core, no `unwrap` in commands, crash reports with consent, fuzzed importers | NFR-018, NFR-023, PLT-017 |
| 34 | "Service après vente?"; support only over WhatsApp/AnyDesk | 01 §8 | See #13 | ONB-004, ONB-011, ONB-012 |
| 35 | Licences tied to the machine: re-activation after formatting or a CPU change, USB-key licences, per-PC/per-user/per-phone fees, time-limited keys | 01 §6 (FA_Gstock, Dolisoft, Zedkira, FlamePOS) | No licence, no activation, unlimited PCs and users | PLT-002, PLT-003 |
| 36 | Basics sold as paid options (PME PRO base: 1 user, 2 company files, 2 tills, 4 payment modes; Crescendo Free: 2 tills/users/files) | 01 §4.2 | Everything included | PLT-003 |
| 37 | Offline multi-till with server failover is a premium (70,000 DA at TifawtTech) | 01 §1 (9), §12 (5) | Standard: every till sells from its own replica | LAN-003, LAN-010 |
| 38 | Owner mobile app sold as an add-on or yearly fee (AlgoStock Pro Connect 5,000 DA/yr) | 01 §1 (10) | Free owner view on the shop LAN; DZBuild dashboard when linked | LAN-019, DZB-022 |
| 39 | Cracked software: viruses, no fiscal updates, legal exposure (cracked WinDev 25 at 30,000 DA; "outils d'activation") | 01 §7 | A free, signed, auto-updating product removes the reason to pirate | PLT-002, PLT-009, PLT-010 |
| 40 | Pains vendors claim to solve: paper credit notebooks, calculation errors, employee theft, stock discrepancies, expired goods, lost orders, not knowing who owes what, data loss, internet cuts, expensive hardware | 01 §8 | Kridi ledger, audit log and PINs, counts, expiry/FEFO, orders, statements, verified backups, offline-first, low hardware floor | CRD-*, AUD-001, STK-009, PRD-053, ORD-004, DAT-*, NFR-001 |
| 41 | Language gaps: many products are French-only (Crescendo Free, Klick, FA_Gstock, Quick FACT, PME PRO) or Arabic-only (Tajir, WR manager, Flampex); few offer AR/FR/EN, and documents are usually FR or AR only | 01 §4.1, §9.4 | Natively trilingual: per-user UI language, documents in any of the three languages or bilingual AR + FR, amount in letters in three languages | UX-001, UX-004, UX-041–044, DOC-004, NFR-027 |

### 3.1 Opportunities named by report 01 (§12) and how the spec answers them

| # | Opportunity | DZBuild POS answer | Spec |
|---|---|---|---|
| 1 | No product is free, complete, offline, Arabic RTL and multi-PC at once | "Free forever, every feature, no activation key", natively trilingual | PLT-002, PLT-003, UX-001 |
| 2 | Close the trust gap | Instant signed installer, no activation, auto-update, public changelog, 2027 conformity statement per version | PLT-009, PLT-010, PLT-026, AUD-011 |
| 3 | Make switching easy (competitors win switchers by entering products and remapping keys) | Excel/CSV wizards for products, customers with opening debts and suppliers; importers for WinDev/HFSQL, DBF, Access, Sage/EBP/Ciel/PC Compta/Odoo exports; keymap presets | IMX-005, IMX-006, IMX-017, IMX-019, IMX-022, UX-018 |
| 4 | Combine the Algerian specifics nobody has in one product | Credit ceilings + SMS/WhatsApp reminders, price levels, carton/pack/unit, scale barcodes, flexy, repairs, instalments, état 104/G50/Zakat helpers, timbre, NIF/NIS/RC/AI, bon de route, consignes, serials/warranty, lots/expiry, multi-depot | CRD-*, PRC-001, PRD-031, SAL-014, VRT-002, ORD-005, TMP-*, RPT-048, RPT-053, FIN-007, TAX-006, DOC-030, VRT-170, PRD-050–054, STK-003 |
| 5 | Offline multi-till with server failover as standard | Replica + outbox on every till; manual host promotion | LAN-002, LAN-003, LAN-010 |
| 6 | Use DZBuild OAuth for a mobile owner app and cloud extras | Remote dashboard, encrypted cloud backup, multi-shop consolidation, catalogue and stock sync | DZB-*, DAT-016, LAN-016 |
| 7 | WhatsApp everywhere | Receipts, invoice PDFs, debt reminders, supplier purchase orders | SAL-052, DOC-021, CRD-009, PUR-002, API-001 |
| 8 | Very low hardware requirements | Windows 10 1809+/11, optional 32-bit build, ≤ 15 MB online installer, 4 GB RAM floor (Windows 7 dropped, §10) | PLT-001, PLT-004, NFR-001 |
| 9 | Restaurant mode in the same app (hybrid "superette + pizzeria") | Switchable retail, restaurant and wholesale screens | UX-020, §29 of the spec |
| 10 | Build for the 2027 rules from day one | Append-only hash-chained journal, archives, Z reports, gapless numbering, corrections by credit note only, exportable audit file, per-version engagement | AUD-003–011, CFG-005, RET-012, DOC-028 |
| 11 | Go beyond "Arabic + French" | Owner decision: AR/FR/EN equal at MVP, completeness enforced in CI | §0.6 of the spec, NFR-027 |

## 4. Regulatory must-haves

Sources: 04 (primary texts: JORADP, DGI 2026 codes and circulars, ANPDP) and 09 §5. "Rel" is the release in the spec.

| # | Requirement | Legal basis | What the product must do | Rel | Spec |
|---|---|---|---|---|---|
| 1 | Every invoice carries seller and buyer identity, legal form, capital, RC, NIS, payment mode and due date, date and sequential number, designation, qty, PU HT, total HT, each tax, **TTC in figures and in letters**; wet stamp and signature unless telematic | Décret 05-468 art. 3–4 | Invoice template with blocking validation | MVP | DOC-002, DOC-003, DOC-004 |
| 2 | **NIF** of seller and buyer; omission = "défaut de facturation", fine 80 % of the amount | Loi 04-02 art. 33–34 | NIF mandatory for B2B customers | MVP | SAL-043, CRM-001 |
| 3 | Every consumer sale gets a **ticket de caisse** or bon; invoice on request | Loi 04-02 art. 10; LFC 2018 art. 4 | Default output = ticket; one-click convert to facture | MVP | DOC-005, SAL-054 |
| 4 | Uninterrupted chronological numbering; cancelled invoice kept with "facture annulée" diagonally; no erasures | Décret 05-468 art. 10 | Gapless series per document type and till; void ≠ delete | MVP | CFG-005, DOC-007, AUD-003 |
| 5 | TVA 19 % / 9 % / exempt shown distinctly; **IFU taxpayers may not show TVA** | CTCA art. 21, 23, 64 | Tax codes as data with validity dates; regime switch | MVP | TAX-001–003, CFG-002 |
| 6 | **Droit de timbre** on cash receipts: ≤ 300 DA exempt; ceil(amount/100) tranches × 1 / 1.5 / 2 DA by the bracket of the whole amount (not progressive); minimum 5 DA; no ceiling; electronic payments exempt; cheque/transfer exempt with details on the receipt | Code du timbre art. 100, 258, 258 quinquies; LF 2025 art. 46–47; DGI circulaire 14/2025 | Timbre engine on the cash part; monthly statement | MVP | TAX-006, TAX-007, PAY-023, RPT-046 |
| 7 | From **1 January 2027**, cash/management software for non-invoiced sales needs a **publisher engagement** or accredited certificate of inalterability, security, conservation and archiving; publishers must hand over code and documentation on request | LF 2026 art. 58, 74, 81, 109 (CTCA 51 bis, CPF 20 ter, CPF 51 sexies); Cirtasoft cites a 25,000 DA fine plus revenue reassessment as an example (01 §11) | Hash-chained journal, perpetual Z totals, archive export, engagement template, technical documentation | MVP | AUD-003–012 |
| 8 | Retention: books and vouchers 10 years; IT data and processing documentation until the end of the 6th year | CPF art. 64 (LF 2026 art. 82) | No hard delete; archive per fiscal year | MVP | DAT-022, AUD-010 |
| 9 | Monthly **G50** by the 20th (e-filing mandatory for réel), **G9 état fournisseurs**, annual **état clients** for wholesale, **état spécial art. 18** for cancellations | CTCA 18, 29, 76; CIDTA 183 ter; LF 2026 art. 111 | G50 helper, G9 export, état clients, état spécial | MVP / V1 | RPT-048, RPT-049, RPT-053, RPT-055 |
| 10 | **IFU**: threshold **8,000,000 DA** (AE 5,000,000 DA); rates 5 / 12 / 0.5 %; minimum 30,000 / 10,000 DA; G12 by 30 June, G12 bis by 20 January; registres des achats et des ventes | CIDTA 282 bis–sexies, 365; CPF art. 1; DGI IFU page | IFU tracker, registers, G12 helper | MVP / V1 | TAX-014, RPT-050–052, RPT-056 |
| 11 | Wholesalers must authenticate each client's RC (CNRC) and NIF (DGI) before selling | CIDTA 183 ter | "Revendeur" type with verification date and portal links | MVP | CRM-006 |
| 12 | Merchants must offer an **electronic payment** instrument (fine 50,000 DA); TPE push in 11 sectors in 2026 | LF 2018 art. 111 as amended by LF 2024 art. 113 | CIB/Edahabia/QR/transfer tenders with references | MVP | PAY-002, PAY-007–009 |
| 13 | Cash ceilings: ≥ 1 M DA for vehicles, precious metals, antiques; ≥ 5 M DA for real estate; TVA not deductible on invoices > 1 M DA TTC paid cash | Décret 15-153; CTCA 30 | Warnings and a non-deductible flag | V1 / MVP | PAY-024, TAX-013 |
| 14 | **Personal data**: prior declaration to ANPDP, consent, DPO, processing register, automated access log, breach notice within 5 days, transfers abroad need authorisation | Loi 18-07 as amended by loi 25-11 (24 July 2025) | Local-first storage, consent flags, PII access log, subject-rights tools, DPA for cloud sync | MVP / V1 | PDP-001–010 |
| 15 | Consumer information "essentially in Arabic"; lot and DLC/DDM on food; prices displayed TTC | Décret 13-378 art. 7, 29–33; loi 04-02 art. 4–6 | Tickets and labels in AR, FR or EN, with a bilingual AR + FR layout; a warning when consumer-facing food or regulated-goods information is printed without Arabic; lots and expiry; displayed price = TTC; label reprint after price change | MVP | DOC-013, DOC-034, UX-042, LBL-005, LBL-007, PRD-051–053 |
| 16 | **Warranty** certificate with fixed mentions; minimum 6 months new / 3 months used; +30 days when immobilised; replace or refund within 30 days if repair fails | Décret 13-327 (JO 49/2013) | Certificate from the sale; warranty claims log | MVP (TEL/INS) / V1 | DOC-012, ORD-007, SAL-032 |
| 17 | **Soldes** twice a year, 6 weeks (Jan–Feb, Jul–Aug; dates per wali), goods held ≥ 3 months, prior declaration with previous prices; promotions also declared; no prize draw tied to a purchase | Décret 06-215 (JO 41/2006) | Campaign types with "prix pratiqué auparavant", window checks, declaration export | MVP / V1 | PRM-021–025, RPT-054 |
| 18 | **Regulated consumer prices**: oil 5 L 650 DA, sugar 90/95 DA/kg, milk sachet 25 DA/L, semolina tables | Décrets 11-108 / 21-383, 01-50 / 22-186, 07-402 / 20-242 | Ceiling guard and data pack | MVP | SAL-030, PRC-012, VRT-001 |
| 19 | **Jewellers**: tax-administration authorisation; special client register (nature, number, weight, titre, price, buyer identity/address), 5-year retention | CII art. 359 as amended by LF 2026 art. 62; DGI circulaire 34/2026 | Register print, authorisation fields | V1 | VRT-143, VRT-144 |
| 20 | BL may replace the invoice only with an authorisation decision; monthly facture récapitulative; **bon de transfert** for moves between own sites; **bon de transaction commerciale** for farm/fishing/artisan purchases | Décret 05-468 art. 12–17; décret 16-66 | Document types with required fields | MVP / V1 | CFG-003, DOC-009–011 |
| 21 | **69 wilayas** / 1,541 communes (names per décret 26-206) | Loi 26-06 (JO 25/2026) | Dataset + 58 mapping, updatable | MVP | CFG-010 |
| 22 | Fraud by IT means: 5–10 years and 5–10 M DA; accomplices share the penalties; sales without invoice and fictive entries are fraudulent | CIDTA 303 (LF 2026 art. 25); CTCA 118; CIDTA 193 | No fictive or dual-stock features | MVP (design rule) | AUD-003, §40.2 |
| 23 | Year-end inventory ("livre d'inventaire") | Code de commerce art. 9–10 via CIDTA 193, CTCA 118 | Year-end count, paged print | V1 | STK-030, RPT-057 |
| 24 | E-commerce: invoice for every online sale; no validation of out-of-stock orders; COD allowed; 4-day return, 15-day refund | Loi 18-05 | Stock sync and order handling | V2+ | OMN-009 |
| 25 | Not a law but a strong social norm: **Zakat** on trade goods (nisab 1447 AH = 2,295,000 DA per the Ministry of Religious Affairs) | marw.dz | Zakat report with editable nisab | V1 | FIN-007 |

**Not mandatory today (as of 2026-09-28):** no e-invoicing clearance, per-invoice QR, or DGI software homologation exists in LF 2025, LF 2026 or the 2026 circulars. The only software obligation found is the 2027 publisher engagement (04 §6.2).

## 5. Hardware support matrix

Prices in DZD from Algerian e-shops and Ouedkniss (05, 09 §7, 01 §10). Common brands on Ouedkniss: Smart POS / Safe POS, Xprinter, Rongta, Aclas, DIGI, Honeywell (01 §10).

| Device class | Common models in Algeria | Price (DA) | Interface / protocol | MVP support | Later |
|---|---|---|---|---|---|
| Receipt printer 80 mm | Xprinter XP-80T/Q806/D200N/320B/S260M/Q80K (Wi-Fi/USB/LAN), Rongta RP335, Smart POS SP-802/RP80180, POSLUX Z800/Z822; Epson TM-T20/T82 (premium) | 10,000–23,500 (XP-D200N 13,500–14,900; XP-Q80K 19,500); Epson 35,000–55,000 | ESC/POS over USB (spooler RAW or usbprint), LAN TCP 9100, COM, BT | Raw ESC/POS + driver fallback; Arabic raster (576 dots, `GS v 0` bands); text mode for Latin; logo, QR, barcode, cut; profiles; status polling; printer doctor | ASB events, NV logo, BT printers |
| Receipt printer 58 mm | Mobile BT (XP-P801A, SP 380, RPP-02A…), Sunmi built-in | 15,000–26,000 | ESC/POS BT/USB | 384-dot raster layout | — |
| Kitchen printer | Xprinter XP-A260M (buzzer, LAN) | 18,500 | ESC/POS LAN | Category routing, cancellation tickets | — |
| Dot-matrix (multi-copy) | Epson LX-300/LX-350, LQ-680, LQ-2090 | 31,500–180,000 | Windows driver, text mode | — | V1: continuous forms, ORIGINAL/DUPLICATA/TRIPLICATA |
| Label printer | Xprinter XP-365B/233B/235I/410B/420B/350B/370B/DT427B, Smart POS S58, Godex DT4x/G300, Zebra, TSC | 14,500–42,500 (XP-DT427B 17,500) | TSPL (+EPL/ZPL/DPL emulation), EZPL, ZPL | TSPL + Arabic bitmap; 11 presets + custom | V1 ZPL/EZPL |
| Barcode scanner | Henex HC-3206/3206R BT/5208/6052, Riva 3206, Smart SP1/SP-8602/5066, Honeywell MS5145, EliteECR, POSLUX | 5,800–23,500 (entry models 6,000–7,500) | USB HID keyboard wedge (1D/2D), wireless, BT | Layout-proof global capture, suffix handling, test barcode | V1 USB-COM and HID-POS modes |
| Data collector / PDA | POSLUX DT-92; Android POSLUX 35-6, Lenvii Q6, Rongta I2 | 32,000–137,500 | CSV/TXT batch; Android | Batch count import | V2 Android companion |
| Label-printing scale | Rongta RLS1000/1100, ACLAS LS2X/LS6, DIGI (30 kg), Dibal D900 | 61,000–109,000 (DIGI 30 kg 61,000; Rongta 89,900) | Prints EAN-13 prefixes 20–29; RS232 + Ethernet | Configurable weight/price barcode parser | V1 PLU export files; V2 PLU push (Rongta TCP, ACLAS SDK) |
| Checkout / counting scale | Toledo 8217, CAS PD-II/ER, Adam, Preciamolen, PRO PESAGE, CAS PR-C | 85,000 (price-computing) | Serial RS-232 / USB-serial | — | V1 serial protocols with auto-baud; V2 counting scales |
| Cash drawer | Riva RV 330R/410R, Smart POS, Safe POS, POSLUX CD-405A, TE-330 | 7,500–15,000; USB adapter 3,000 | RJ11/RJ12 kick via printer (24 V), USB-COM adapter | `ESC p` pin 2/5, USB-COM kick, audit of no-sale opens | V1 drawer-sensor enforcement |
| Customer display | VFD 2×20 (COMPOS PL200, built into POSLUX/ENIGMA); second monitor on dual-screen terminals | bundled | Serial ESC/POS/CD5220 dialects; HDMI second screen | VFD (Latin); second screen with full Arabic, QR | V1 ads playlist |
| All-in-one touch terminal | Smart POS A3, POSLUX G53, ENIGMA, HFPOS 698, NewPOS, SMARTPOS A3 (J1900) | 61,500–150,000 (15″ J1900/Celeron pack ≈ 85,000–95,000) | Windows 10, 15–15.6″, 1024×768 / 1366×768 | UI at 1024×768, on-screen AZERTY + Arabic keyboard | — |
| Budget PC till | Refurbished Lenovo M93 (4th gen) | 18,500 (+ Win10/11 key 1,500–2,000) | Windows 10 | Reference low-end target | — |
| Payment terminal (TPE) | GIE-certified: PAX A920/A8900, SZZT KS8226, Topwise, MoreFun, Verifone X990, Ingenico Move 2500, Nexgo G2 (CIBPAY app) | bank-supplied | Standalone | Manual tender + batch reconciliation | V2+ ECR/app-to-app if banks and GIE allow |
| Badges and biometrics | 125 kHz RFID readers (HID), barcode staff cards; ZKTeco K40/K60 pointeuses | n/p | HID wedge; ZK protocol TCP/UDP 4370 | — | V1 badge login; V2 ZKTeco import, fingerprint, gym access |
| GSM modem | Huawei E3531 / K3772 | 3,800 | AT commands (SMS, USSD) | — | V1 offline SMS/USSD gateway (flexy, reminders) |
| CCTV recorder | Hikvision NVR "POS" function | n/p | Text over TCP | — | V2 receipt overlay |
| UPS | 650 VA line-interactive | 7,900 | HID-UPS / battery API | Guidance | V1 battery-aware backup |
| Banknote counter | SH-2830, Ribao BC-55, counters with customer display | 15,000–260,000 | not integrated | Per-denomination entry at close | — |
| Chifa card reader | Thales; also sold by General IT | 19,500 | n/p | — | V2 pharmacy pack |

## 6. DZBuild integration summary

**What DZBuild is (06 §1):** an Algerian e-commerce SaaS founded in 2024, built around cash on delivery, 102+ couriers, landing pages, WhatsApp order messages, Arabic/French. It claims 14,498 stores. Paid plans: Pro 1,000 DA/month (300 active products), Illimité 2,500 DA/month, Entreprise from 5,500 DA/month. It has **no POS today**; its Pickup-from-Store add-on targets merchants who also run a physical shop.

**What exists (06 §3):**
- Developer platform at dzbuild.dev, launched **2026-09-26**: OAuth 2.0 authorization code + PKCE (S256), one non-expiring `dzpk_live_` token per store, REST API `https://api.dzbuild.app/v1` with 19 scopes, HMAC-signed webhooks (order events only), signed `dz_launch` JWTs, OpenAPI 3.1, `llms.txt`.
- Two undocumented authorization servers: an OIDC provider on dzbuild.com (confidential clients, no refresh tokens) and the MCP connector server at mcp.dzbuild.com (public clients, dynamic registration, refresh tokens).
- **dzship**: a free MIT shipping API for 99 couriers, usable by unlinked shops.

**Why a desktop app can't link directly today:** redirect URIs must be **https** (no loopback), a **client secret** is mandatory, and the security rules keep secrets and install tokens **server side** (06 §0).

**Recommended design:**
- **Principles:** offline-first, linking optional, never an activation server, least-privilege scopes, nothing permanent and store-wide stored on the shop PC.
- **Track A (now):** a DZBuild-run "POS Link" broker holds the client secret and install tokens (encrypted), issues short-lived device tokens with rotating refresh tokens over RFC 8252 loopback + PKCE, relays API calls, and receives webhooks for the desktop to pull.
- **Track B (target):** extend DZBuild's own authorization server with a first-party public client (loopback on any port, PKCE S256 only, refresh rotation, revocation endpoint, device authorization grant so the **owner approves a till from the DZBuild Android app**, optional DPoP).
- **Desktop rules:** system browser only; refresh token in Windows Credential Manager; access token in memory; native HTTP (no CORS support); deterministic idempotency keys; clock-skew correction; "Reconnexion requise" never blocks sales.

**Sync model (06 §5.5):**

| Data | Direction | Today |
|---|---|---|
| Store profile (name, logo, colours, public URL) | DZ → POS | `GET /v1/store`, for the receipt header and "Commandez en ligne" QR |
| Categories, products, variants, prices, images | POS → DZ (per-product "Publier en ligne") | Create/patch; images only from public URLs (10/min); plan caps (Pro 300 active); extra products as drafts |
| Physical stock | POS is the source of truth → DZ | `POST /stock` per product with delta/set; buffer rule; echo suppression |
| Online orders | DZ → POS inbox | Poll `GET /v1/orders` every 1–2 min + webhooks through the broker |
| Order status, send to delivery, WhatsApp notices | POS → DZ | `PATCH /orders`, `send-to-delivery`, `POST /orders/{id}/whatsapp` |
| Customer lookup (fraud score, banned) | DZ → POS | `GET /v1/customers?phone=` |
| In-store sales, Z reports | POS → DZ | **Not possible cleanly today** (needs new endpoints) |
| Cloud backup | POS → DZ | **Doesn't exist yet** (zero-knowledge backup endpoints proposed) |

**API gaps DZBuild should close (06 §4, §6):** no `updated_since` on products; orders filter by creation date only; no product/stock/customer webhooks; image upload only from URLs; `POST /v1/orders` can't model an in-store sale; stock writes land in the undo history; no barcode on variants; no `external_id`; stock-deduction mode, timezone and plan limits not exposed; no device registry; no RFC 8414 metadata; no backup storage; the "no 'dzbuild' in app names" rule blocks a first-party listing.

**Free-service model (06 §5.7):** the POS is free with `min_plan=free`; plan limits still apply to what the store accepts; unlinked users see a "Créez votre boutique en ligne" call to action (growth loop); opt-in identity-less usage events.

**Branding (06 §2):** amber `#F59E0B`, brand yellow `#FFC300`, ink `#1C1917` / `#0B1120`, off-white `#FBFAF7`; fonts Cairo (Arabic) and Inter (Latin), bundled offline.

## 7. Recommended stack

| Layer | Pick | Why |
|---|---|---|
| Desktop shell | **Tauri 2.12+** (Rust + WebView2), Windows 10 1809+ / 11 x64, optional i686 | ~3–10 MB installer, low RAM vs Electron, full Win32 access from Rust (spooler, serial, USB), signed updater, Chromium-grade Arabic shaping, web skills reused |
| UI | **React 19 + TypeScript + Vite 8 + Tailwind 4 + shadcn/ui (RTL mode)**, TanStack Table v9 + Virtual, TanStack Query, Zustand, react-hook-form + zod, i18next (ar/fr/en catalogues, runtime switch, `dir()`), native `Intl` | First-class RTL components, dense virtualised tables, typed forms, three equal UI languages |
| Rust ↔ UI contract | **tauri-specta** generated bindings | The UI never sends SQL; one typed contract |
| Database | **SQLite 3.53** via **rusqlite 0.40** (bundled SQLCipher), WAL + `synchronous=FULL`, rusqlite_migration, FTS5 | Zero admin, crash-safe, fast on HDD/Celeron, encrypted at rest |
| Multi-till LAN | Same exe in *host* role (axum HTTP + WebSocket, mDNS discovery) or *client* role (local replica + outbox); UUIDv7 IDs, append-only ledgers | Works with no internet; tills keep selling when the host is down; no third-party sync (cr-sqlite stalled, ElectricSQL/PowerSync/Turso cloud-bound or beta) |
| Cloud link | Same outbox → dzbuild.dev API; OAuth + PKCE loopback; keyring (Credential Manager) | Standards-based, optional |
| Receipts | Own ESC/POS layer on the `escpos` crate; RAW via winspool, usbprint, TCP 9100, COM; **Arabic as raster** (WebView2 canvas, or HarfRust + tiny-skia headless) | Printer firmware can't shape Arabic |
| A4 / PDF | HTML/CSS templates in a hidden WebView2 → silent `Print` / `PrintToPdf`; Typst for headless batch PDFs | Perfect Arabic, editable templates, no dialog |
| Excel / CSV | **calamine** (read), **rust_xlsxwriter** (write), `csv` + `encoding_rs` | The npm `xlsx` package is stuck on a vulnerable version; ExcelJS is inactive |
| Labels / barcodes | TSPL/ZPL generators; bwip-js for A4/labels; printer-native codes on receipts | |
| Updates / installer | Tauri updater (mandatory signatures, channels, LAN mirror, never during a shift); NSIS online setup + 127 MB offline installer | Works without internet; resellers distribute by USB |
| Signing | OV certificate on a cloud HSM (Azure Artifact Signing isn't available to Algerian entities) + Microsoft Store channel (Algeria is a Store market) | Smart App Control blocks unsigned apps |
| Logs / crashes | `tracing` rolling logs, out-of-process minidumps uploaded only with consent, diagnostic ZIP | Offline and privacy-respecting |
| Tests | cargo-nextest + proptest + insta, virtual printer + golden PNGs, cargo-fuzz on importers, Vitest + `mockIPC`, WebdriverIO `@wdio/tauri-service` | "Bulletproof" money, stock and printing |

**Why not the alternatives (07 §2.3):**
- **Electron:** 80–200 MB installers, 150–400 MB RAM, readable `app.asar`.
- **.NET WPF:** the best native option, but dated UI, easy IL decompilation, and .NET 10 lists only Enterprise/IoT Windows 10.
- **WinUI 3 / MAUI:** poor fit for a dense, keyboard-heavy POS.
- **Flutter:** thin desktop hardware ecosystem.
- **Qt 6:** Qt 6.12 is the last version supporting Windows 10 (46 % of Algerian PCs), plus licensing obligations.

**Windows 7 is out:** Tauri 2.12 (2026-09-26) and Rust ≥ 1.78 dropped it; Windows 7 is 0.86 % of Algerian desktops. This supersedes 05 §7.2 (see §10).

**Architecture (07 §4):**
- Crates: `app-shell` (thin Tauri layer), `pos-core` (domain, DB, sync; no Tauri dependency), `pos-hw` (ESC/POS, raster, spooler, serial, labels, drawer), `pos-docs` (Excel/CSV/PDF), `pos-server` (axum LAN host).
- Three roles from one binary: standalone, host, client.
- The webview is stateless for money: every sale is committed by one Rust command in one transaction.

## 8. How we build it: engineering practices

- **Toolchain:** MSVC C++ Build Tools, Rust (`rustup default stable-msvc`, plus rust-analyzer, clippy, rustfmt), Node 24 + pnpm.
- **Guardrails in the repository:** `pnpm check` runs ESLint, `tsc`, Vitest, the tooling tests, the ar/fr/en parity check, Prettier, `cargo fmt --check`, Clippy with `-D warnings` and `cargo test --workspace`; CI runs the same gates on every push. Keys, `.env` files and generated bindings stay out of hand edits, and applied migrations are never edited (a new numbered file per change).
- **Rules every change follows:** offline-first, integer money, one transaction per write, append-only migrations, no `unwrap` in commands, i18n everywhere, tests first for money, tax, stock and credit.
- **Verification ladder:** unit and property tests per change → contract tests on every command and on the generated TypeScript bindings → E2E screenshots on Windows in ar/fr/en per release → the manual smoke checklists on real hardware (`docs/testing/smoke/`).

---
## 9. Risks

| # | Risk | Likelihood | Impact | Mitigation | Source |
|---|---|---|---|---|---|
| R1 | **MVP scope is very large** (361 MVP features + 43 MVP reports) and delays the first release | High | High | Ship a core MVP (retail + kridi + cash + legal documents + hardware) first and put vertical packs behind toggles; freeze scope after field interviews | Spec §41 |
| R2 | A **free competitor already exists** (Klick Stock Express) | Certain | Medium | Beat it on what it paywalls (kridi, multi-depot, price tiers, inventory, reports, treasury), on Arabic RTL, scales, multi-till offline and 2027 compliance | 09 §3.1 |
| R3 | Some shops **want the illegal "non-billable stock" and fictive invoices** that competitors sell (Klick, Smart Gestion, WINSTO, ZSOFT, PRO SOLUTION) | Medium | Medium | Market "conforme, inaltérable" as protection; offer lawful stock states and IFU TTC mode | 09 §5.5, 01 §1 (6) |
| R4 | **Publisher liability from 2027**: DZBuild must sign an engagement and hand over code and documentation on request; the arrêté fixing the model isn't published | High | High | Legal review before 1 Jan 2027; hash-chained journal and documentation pack now; design the exporter as a plug-in | 04 §6.1, 09 P6 |
| R5 | **Licence model undecided** (proprietary freeware or open source) changes signing options, trust and code-disclosure exposure | Medium | Medium | Decide early; OSS signing routes (SignPath, Certum) exist only for OSI licences | 09 P5, §10.4 |
| R6 | **Support at scale** for a free product (installation, data entry, hardware issues) | High | High | Offline help, Darija videos, remote-support button, import templates, hardware-reseller programme, community forum, WhatsApp support hours | 09 P7, §11 |
| R7 | **Research is desk-only**; many facts are [S] (listings, social media); foreign-product presence is thin in 01 | Certain | Medium | Install and test Stock Express, Crescendo Free, Almawarid Desktop and Dipposoft free; interview 10–15 shops | 01 §0, 09 P4 |
| R8 | WebView2 RAM on 4 GB PCs | Medium | High | Memory budget tests, virtualised lists, `MemoryUsageTargetLevel` LOW, single window | 07 §18 R1 |
| R9 | A shop PC lacks WebView2 or has it broken | Low–Med | High | Embedded bootstrapper + offline installer, startup check with repair link | 07 R2 |
| R10 | Windows 7/8.1 users excluded, although several competitors still advertise Windows 7/XP and 512 MB RAM (ITSCOM, SuperettePro) | Low (~1.2 %) | Low–Med | Clear requirements; resellers upgrade to Win10 IoT/LTSC or Win11 | 07 R3, 01 §1 (8) |
| R11 | SmartScreen warnings for a new publisher; Azure Artifact Signing unavailable in Algeria | High (first months) | Medium | OV certificate on a cloud HSM, Microsoft Store channel, install video | 07 R4 |
| R12 | Rust skills gap | Medium | Medium | Small, typed Rust surface; current library docs; strong tests; code review on every change | 07 R5 |
| R13 | Arabic thermal printing quality varies by printer | Medium | Medium | Raster pipeline, per-printer calibration, hardware lab of the 12 cheapest local models | 07 R6, 05 §1.5 |
| R14 | Data loss on power cut or disk failure | Medium | Very high | WAL + FULL, integrity checks, verified multi-target backups | 07 R7 |
| R15 | LAN host down at peak time | Medium | High | Tills sell from their replica; manual "promote to host" | 07 R8 |
| R16 | Community Tauri plugins abandoned | Medium | Low | Own `pos-hw` crate; plugins only as references | 07 R9 |
| R17 | Updater private key lost | Low | Very high | Two offline encrypted backups, rotation plan | 07 R10 |
| R18 | WebView2 print API regressions | Low | Medium | PDF fallback; raw spooler for thermal printers; pinned minimum WebView2 | 07 R11 |
| R19 | Tauri breaking changes (v3) | Low | Medium | Business logic in Tauri-free crates | 07 R12 |
| R20 | **DZBuild API gaps** (desktop OAuth, deltas, in-store sales, backups) and a **broker** that holds every linked store's token | High | Medium | Track A broker with KMS, isolation and audit logs; push the Track B and API proposals to the DZBuild team | 06 §4, §5.2 |
| R21 | Plan caps limit online publishing (Pro = 300 active products) | High | Low | Per-product "Publier en ligne" toggle, drafts, clear cap display | 06 §1.3 |
| R22 | Cloud sync hosted outside Algeria needs ANPDP authorisation; DZBuild acts as sous-traitant | Medium | Medium | Algerian hosting or explicit authorisation; DPA in the terms; 5-day breach procedure | 04 §7 |
| R23 | Card-terminal integration needs GIE Monétique certification; no public ECR standard | High | Low | Manual tender in MVP; ECR as a bank partnership | 09 §5.8, 05 §11 |
| R24 | Yearly regulatory drift (tax rates, timbre, ceilings, nisab, wilayas, holidays) | Certain | Medium | Signed, updatable data packs with an owner and a yearly review | 09 DZ-CMP-02, 04 §3.1 |
| R25 | Migrating data from WinDev/HFSQL competitors is immature (reader v0.1) | Medium | Medium | ODBC route first; Excel mapping as the universal fallback | 09 §8 |
| R26 | Arabic on dot-matrix printers is unverified | Medium | Low | Graphics mode or recommend a laser printer for Arabic documents | 09 §7.1 |
| R27 | **Trilingual parity at MVP** triples translation and QA work for every string, template, report, label, message, help page and video; a half-translated language breaks trust | High | Medium | One reviewed AR/FR/EN glossary; translators per language; CI gate on missing or empty keys; pseudo-localisation; golden receipts, invoices and reports in all three languages; amount-in-letters unit tests per language | Owner decision 2026-09-28; spec NFR-027 |
| R28 | **English documents vs Algerian law**: consumer information must be "essentially in Arabic" (décret 13-378) and loi 91-05 may require Arabic on invoices | Medium | Medium | Warn when a consumer-facing food/regulated label or ticket has no Arabic; propose the bilingual AR + FR layout; confirm loi 91-05 before release (§11) | 04 §9, spec DOC-034 |
| R29 | **Hardware shops are the main sales channel** and bundle competitors' licences | High | Medium | Reseller/installer kit (offline installer, demo data, training), hardware lab of popular models, co-marketing | 01 §1 (15), §6; spec ONB-013 |

## 10. Contradictions resolved between research files

| Topic | What the files said | Resolution |
|---|---|---|
| Windows 7 | 05 §7.2: Tauri 2 can still target Win7 | **07 wins:** Tauri 2.12 (2026-09-26) dropped Win7 and Rust needs Win10+. Minimum is Windows 10 1809+. |
| Almawarid price | 01: Standard 20,000 DA/yr (100 documents/month), Enterprise 500 documents/month, free plan 3 invoices/month, Ouedkniss pack 33,000 DA; 02: 10,000 / 18,000 DA/yr | The live pricing page (2026-09-28) says "Tarif sur demande" for every plan, with document caps; one listing says "dès 15,000 DA/an". Record as "price on request, historically 10–20k DA/yr". |
| Droit de timbre | 02: "progressive 1 / 1.5 / 2 DA" | **04 wins:** DGI circular 14/2025 says the duty is **not** progressive; the bracket of the whole amount applies. |
| Pressing and butcher software | 02: none found in Algeria | **Wrong:** 09 found 5 pressing products and 4 butcher offers on Ouedkniss. |
| Klick "free" | 01: free, limits unknown; a reseller sells Stock Standard at 9,900 DA | 09 verified the free/paid feature split and the vendor's 10,000 DA Standard price (§2.1 above); 9,900 DA is a reseller price. |
| Codeshade billing period | 01: Starter 7,000 / Growth 9,000 DA **per month**; 02: the same amounts as one-time prices | Unresolved; §2.6 shows both readings. Check codeshade.app before quoting it. |
| Dolisoft price | 02: lifetime, "1st licence bought = 2nd free"; 01: 12,000 DA per PC, "buy 2, 3rd free" | Promotions change; record 12,000 DA per PC, lifetime. |
| GDS Pro vendor | 02/09: Yellow Store DZ (Facebook video); 01: Digideel, Bab Ezzouar, 25,000 DA | Treated as one product promoted by both (unverified). |
| Timbre tiers | 01 (quoting Costy): "about 1 % / 1.5 % / 2 %" by amount, minimum 5 DA | Consistent with 04 (1 / 1.5 / 2 DA per 100 DA tranche = 1 / 1.5 / 2 %), but **04 wins on the details**: ≤ 300 DA exempt, bracket of the whole amount, not progressive, cash only. |
| Old Windows | 01: sellers advertise Windows XP/7 and 512 MB RAM; 01 §12 suggests targeting Windows 7 | **07 wins** (Windows 10 1809+); kept as risk R10. |
| Product languages | 01 §1 (20) and 02: build in "Arabic (RTL) and French" | **Owner decision 2026-09-28:** natively trilingual AR/FR/EN, all equal and complete at MVP (spec §0.6). |
| Wilaya 65 spelling | 04: "Aïn Oussera" | Décret 26-206 spells it **"Aïn Ouessara"**. Bir El Ater comes from Tébessa (loi 26-06). |
| Payroll | 03: n/a | Local competitors bundle CNAS/IRG payroll → a "salaires & avances" lite module (V2+). |
| ar-DZ number format | 07: `1.234.567,50 د.ج.‏`; 04: `1 234,56 دج` | Pick one house style per language (AR, FR, EN) and override ICU; don't mix within a language (spec UX-005). |
| IFU threshold | Blogs: 15 M DA | DGI page and circular 29/2026: **8,000,000 DA**. |
| Africa/Algiers time zone | 07: unverified | Verified locally: UTC+01:00 all year, no DST. |

## 11. Open questions and unverified items

**Regulatory**
- Arrêté fixing the model of the 2027 publisher engagement / certificate and the accredited bodies (CPF 20 ter); arrêté on the format of dematerialised accounting files. Expected before 1 Jan 2027; not found.
- Implementing decree for ticket-de-caisse mentions (loi 04-02 art. 12); joint arrêté on telematic invoices (05-468 art. 11).
- Whether invoice numbering must restart each fiscal year; who legally bears the timbre; timbre on split tenders; rounding rules.
- Warranty-duration arrêtés per product and the certificate model (13-327); whether 06-215 was amended; yearly wali soldes dates.
- Bread price text and post-2022 revisions of staple ceilings; CNAS rates and IRG salary abatements.
- Whether Arabic is legally required on invoices themselves (loi 91-05, JO scan unread); Code de commerce art. 10 text. With English documents allowed (owner decision), also confirm whether an English-only or French-only invoice or ticket is acceptable for domestic sales.
- What local products call "état 104" (probably the annual état clients, RPT-053) and "état 204" (unknown); verify before naming the reports.
- Weekend decree reference, 3-day Eid, Ramadan hours; name and platform of the auto-entrepreneur registry.

**Payments and partners**
- Interbank QR standard and any ECR protocol; whether banks allow third-party apps on GIE-certified Android TPEs.
- BaridPay QR has no public API (manual confirmation only).
- Yassir partner API; Maystro and Noest API docs; which formats Algerian e-banking exports (AFB/CFONB).
- Microsoft Store developer registration from Algeria; which CAs issue OV certificates to Algerian organisations.

**Market**
- The exact limits of Klick Stock Express and of Dipposoft's free version; Codeshade's billing period (§10); Doliflex (page fetch failed).
- Presence in Algeria of Loyverse, Daftra, DEXEF, Vatoce, Albadr, Siindbad, Zoho Inventory, PC Compta and EBP (01 §4.6: no evidence found; not every lead was followed).
- Feasibility of importing WinDev/HFSQL `.fic` databases from competitors, and whether a 32-bit build is still needed (01 §12 (3), (8)).
- Fisira's "2026 compliance" claim (QR code and e-signature on invoices); no text requires them today (§4).
- Facebook pages, YouTube descriptions and Ouedkniss mirror sites (fibladi) could not be read (01 §0).
- Loyverse, Daftra and DEXEF usage in Algeria; Hikvision market share; Arabic on dot-matrix printers; the Algerian national layout for GS1 prefixes 20–29 (scales are set by installers); the barcode-obligation date for food products.
- Real shopkeeper priorities (no interviews yet).

## 12. Next steps

1. **Translation pipeline** (owner decision, trilingual): AR/FR/EN string catalogues, one reviewed glossary of business and fiscal terms, translators per language, the CI missing-key gate (NFR-027), pseudo-localisation, and golden documents in all three languages. Report 01 was restored on 2026-09-28 and the spec coverage check re-run.
2. **Field validation:** interview 10–15 shops (superette, butcher, phones, clothing, wholesale, café) and install Stock Express, Crescendo Free, Almawarid Desktop and Dipposoft free. Ask which UI and document language each person and each customer type wants. Freeze the MVP cut only after that.
3. **Competitor scoring matrix** on the ~60 features buyers ask about: Klick, Crescendo Free, Almawarid, AlgoStock, KERS POS, TifawtTech PME PRO and Supermarché, DS RAPIDE, SAFE POS, MekaStore, Dolisoft, Zedkira, CaisseKom, GestiumPRO.
4. **Decisions for the product owner:** licence model; MVP cut (core vs packs); Track A broker vs waiting for Track B; hosting location for cloud backups.
5. **Legal review** before 1 Jan 2027: publisher engagement, code-disclosure process, "no fictive features" policy, ANPDP position of the cloud sync.
6. **DZBuild platform asks** (06 §6): public-client OAuth, event feed, `updated_since`, batch inventory, `pos/sales`, backups, devices, variant barcodes, naming-rule exemption.
7. **Engineering bootstrap:** install the Rust toolchain, `git init`, CI gates and contributor rules; scaffold Tauri + React; build a hardware lab with the cheapest local printers, scanners, a label scale and a drawer.
8. **Data-pack pipeline:** who updates tax rates, timbre, ceilings, nisab, wilayas and holidays, how often, and how they are signed.
9. **Support and channel plan:** in-app help and Darija videos, remote-support button, hardware-reseller programme, community forum, WhatsApp support hours.

## 13. Section files

| File | Content | Status |
|---|---|---|
| [`01-MASTER-FEATURE-SPEC.md`](01-MASTER-FEATURE-SPEC.md) | Complete deduplicated feature catalogue, reports list, import/export matrix, NFRs, exclusions | Written 2026-09-28; updated the same day with report 01 and the trilingual decision |
| [`research/01-algeria-pos-vendors.md`](research/01-algeria-pos-vendors.md) | FR/AR/EN web vendor sweep, Ouedkniss API sweep (505 listings), market map, pricing statistics, master feature catalogue, buyer comments, opportunities | Complete (recovered 2026-09-28) |
| [`research/02-algeria-verticals-arabic.md`](research/02-algeria-verticals-arabic.md) | Arabic and social-media vendors, 17 verticals, local habits, payments, hardware | Complete |
| [`research/03-global-pos-features.md`](research/03-global-pos-features.md) | 547-feature global benchmark of 21 POS products | Complete |
| [`research/04-algeria-legal-fiscal.md`](research/04-algeria-legal-fiscal.md) | Invoicing, TVA, timbre, IFU/réel, 2027 cash-software rules, personal data, payments, COD, wilayas | Complete |
| [`research/05-hardware-environment.md`](research/05-hardware-environment.md) | Printers, labels, scanners, scales, drawers, displays, terminals, Windows, power, LAN, TPE | Complete (Win7 section superseded by 07) |
| [`research/06-dzbuild-integration.md`](research/06-dzbuild-integration.md) | DZBuild platform, OAuth servers, API inventory, gaps, Track A/B design, proposed endpoints | Complete |
| [`research/07-tech-stack.md`](research/07-tech-stack.md) | Framework comparison, architecture, DB, LAN sync, printing, import/export, updates, signing, tests | Complete |
| [`research/09-gaps-and-additions.md`](research/09-gaps-and-additions.md) | Completeness critique: 30 gaps, new verticals, new laws, contradictions, 50 backlog additions | Complete |

<!-- Ouedkniss listing links (reference definitions; URLs from research/01 and 02–09). -->
[d6203275]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-et-caisse-tactile-kouba-alger-algerie-d6203275
[d6347367]: https://www.ouedkniss.com/setif-el-eulma-algerie-applications-logiciels-%C2%A0%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%81%D9%88%D8%A7%D8%AA%D9%8A%D8%B1-%D8%A8%D8%A7%D9%84%D9%84%D8%BA%D8%A9-%D8%A7%D9%84%D8%B9%D8%B1%D8%A8%D9%8A%D8%A9-d6347367
[d6894011]: https://www.ouedkniss.com/bejaia-akbou-algerie-applications-logiciels-logiciel-point-de-vente-et-gestion-com-d6894011
[d8059698]: https://www.ouedkniss.com/alger-hydra-algerie-applications-logiciels-gestion-de-facturation-sur-mesure-d8059698
[d9607035]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-flexy-hammam-bouhadjar-ain-temouchent-algerie-d9607035
[d10912645]: https://www.ouedkniss.com/applications-logiciels-bbs-gestion-commerciale-oran-algerie-d10912645
[d11018311]: https://www.ouedkniss.com/alger-kouba-algerie-applications-logiciels-logiciel-fastshop-d11018311
[d11162776]: https://www.ouedkniss.com/applications-logiciels-logiciel-pme-pmi-stock-facturation-bab-ezzouar-alger-algerie-d11162776
[d11162910]: https://www.ouedkniss.com/applications-logiciels-logiciel-superette-jouet-vetement-bab-ezzouar-alger-algerie-d11162910
[d11537219]: https://www.ouedkniss.com/applications-logiciels-tradman-logiciel-gestion-commerciale-alger-centre-algerie-d11537219
[d12373295]: https://www.ouedkniss.com/autre-logiciel-de-gestion-stock-blida-algerie-d12373295
[d12505797]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestions-commercial-pme-pro-dar-el-beida-alger-algerie-d12505797
[d12993513]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-easykes-logiciel-pour-superette-d12993513
[d13827110]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-tizi-nbechar-setif-algerie-d13827110
[d14435454]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-pour-mini-superette-oran-algerie-d14435454
[d14437207]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-salle-sport-oran-algerie-d14437207
[d15298462]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-du-stock-et-caisse-prix-jusqua-120000-chlef-aflou-ain-babouche-batna-bejaia-algerie-d15298462
[d15825713]: https://www.ouedkniss.com/securite-alarme-logiciel-de-gestion-commerciale-dar-el-beida-alger-algerie-d15825713
[d16024589]: https://www.ouedkniss.com/setif-algerie-applications-logiciels-logiciel-de-gestion-commerciale-d16024589
[d16488297]: https://www.ouedkniss.com/applications-logiciels-quick-fact-gestion-commerciale-stock-facturation-serveur-monoposte-alger-centre-algerie-d16488297
[d17834397]: https://www.ouedkniss.com/bejaia-akbou-algerie-applications-logiciels-logiciel-gestion-commerciale-itscom-d17834397
[d18252240]: https://www.ouedkniss.com/alger-bouzareah-algerie-applications-logiciels-gestacaisse-d18252240
[d18280273]: https://www.ouedkniss.com/alger-kouba-algerie-applications-logiciels-logiciel-pressing-d18280273
[d18723112]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-resto-pro-bab-ezzouar-alger-algerie-d18723112
[d18778898]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-logiciel-superette-grossiste-d18778898
[d19431931]: https://www.ouedkniss.com/biskra-algerie-applications-logiciels-logiciel-de-flexy-v-5-4-d19431931
[d20319224]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-dentreprise-odoo-cheraga-alger-algerie-d20319224
[d20975347]: https://www.ouedkniss.com/alger-bab-el-oued-algerie-applications-logiciels-logiciel-de-vente-d20975347
[d21116539]: https://www.ouedkniss.com/tlemcen-sidi-bel-abbes-oran-naama-ain-temouchent-algerie-applications-logiciels-gestion-salle-de-sport-avec-poiteuse-d21116539
[d21173889]: https://www.ouedkniss.com/setif-algerie-applications-logiciels-winsto-gestion-de-stock-d21173889
[d21919164]: https://www.ouedkniss.com/blida-tizi-ouzou-alger-boumerdes-ain-romana-benian-algerie-applications-logiciels-stock-standard-d21919164
[d22047875]: https://www.ouedkniss.com/applications-logiciels-planete-points-de-vente-alger-centre-algerie-d22047875
[d22296334]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-tajir-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-d22296334
[d22499274]: https://www.ouedkniss.com/chlef-bechar-saida-sidi-bel-abbes-mostaganem-mascara-oran-el-bayadh-naama-algerie-applications-logiciels-logiciel-gestion-commercial-smart-com-d22499274
[d23906352]: https://www.ouedkniss.com/biskra-algerie-applications-logiciels-logiciel-stockcom-gestion-commercial-d23906352
[d23967633]: https://www.ouedkniss.com/applications-logiciels-logiciel-pme-pro-gestion-commerciale-et-stock-application-de-distribution-bab-ezzouar-alger-algerie-d23967633
[d24009309]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-foodmax-logiciel-restaurant-pizzera-d24009309
[d24550112]: https://www.ouedkniss.com/logiciels-abonnements-smart-gestion-v-20-bejaia-algerie-d24550112
[d24881513]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-safe-pos-bouzareah-alger-algerie-d24881513
[d25229062]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-logiciel-pressing-d25229062
[d25918343]: https://www.ouedkniss.com/chlef-bejaia-bechar-blida-bouira-tlemcen-tizi-ouzou-alger-setif-saida-sidi-bel-abbes-mostaganem-oran-boumerdes-tipaza-ain-temouchent-relizane-centre-algerie-applications-logiciels-logiciel-restaurant-restoline-d25918343
[d25951270]: https://www.ouedkniss.com/blida-tlemcen-alger-setif-sidi-bel-abbes-mostaganem-oran-ain-temouchent-centre-algerie-applications-logiciels-logiciel-restaurant-coffeline-d25951270
[d26141727]: https://www.ouedkniss.com/alger-constantine-oran-bab-ezzouar-bir-el-djir-algerie-applications-logiciels-logiciel-pour-commerçant-et-entreprise-d26141727
[d26432450]: https://www.ouedkniss.com/blida-alger-constantine-oran-boumerdes-tipaza-mila-bab-ezzouar-chelghoum-laid-algerie-applications-logiciels-logiwell-resto-d26432450
[d26730980]: https://www.ouedkniss.com/alger-ain-benian-naadja-taya-bab-el-oued-ezzouar-baraki-birkhadem-birtouta-bourouba-dar-beida-gue-de-constantine-les-eucalyptus-smar-rahmania-rouiba-staoueli-tessala-merdja-algerie-autre-logiciel-point-vent-sup%C3%A9rette-promooo-d26730980
[d26735464]: https://www.ouedkniss.com/alger-rouiba-algerie-applications-logiciels-logiciel-de-sup%C3%A9rette-idipos-d26735464
[d27434885]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-commerciale-et-stock-complet-fa-gstock-bougara-blida-algerie-d27434885
[d27522066]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-achat-vent-stock-caisse-khraissia-alger-algerie-d27522066
[d28343846]: https://www.ouedkniss.com/alger-centre-bordj-el-kiffan-algerie-ecoles-formations-formation-erp-sage-d28343846
[d28923331]: https://www.ouedkniss.com/alger-bordj-el-kiffan-algerie-applications-logiciels-logiciel-pour-boucherie-en-promo-d28923331
[d28927989]: https://www.ouedkniss.com/applications-logiciels-promotion-logiciel-gestion-commercial-blida-algerie-d28927989
[d29117684]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-stock-et-commerciale-outils-dactivation-bouira-algerie-d29117684
[d29886941]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-commerciale-el-eulma-setif-algerie-d29886941
[d30034727]: https://www.ouedkniss.com/logiciels-abonnements-gfs-gestion-commerciale-caisse%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D9%88-%D8%A7%D9%84%D8%A8%D9%8A%D8%B9-chlef-algerie-d30034727
[d30243863]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-pressing-degraissage-laverie-constantine-algerie-d30243863
[d30369909]: https://www.ouedkniss.com/applications-logiciels-logiciel-pos-light-superette-cosmetique-vente-vetement-avec-option-flexy-dar-el-beida-alger-algerie-d30369909
[d30370559]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-supermarche-dar-el-beida-alger-algerie-d30370559
[d30642759]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-fast-food-pizzeria-restaurant-pecherie-cafeteria-cremerie-patisserie-tlemcen-algerie-d30642759
[d30720309]: https://www.ouedkniss.com/applications-logiciels-dolibarr-erp-crm-baba-hassen-alger-algerie-d30720309
[d30782671]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-pack-superette-alimentation-generale-epicerie-alger-centre-bab-ezzouar-cheraga-dely-brahim-hydra-algerie-d30782671
[d31231999]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-ouled-fayet-alger-algerie-d31231999
[d32976718]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-commercial-setif-algerie-d32976718
[d33391286]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-stock-gfs-hydra-alger-algerie-d33391286
[d34059266]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-wr-manager-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-ain-oussara-djelfa-algerie-d34059266
[d35537273]: https://www.ouedkniss.com/logiciels-abonnements-promo-logiciel-de-gestion-commercial-bab-ezzouar-ben-aknoun-birtouta-dely-brahim-hydra-alger-algerie-d35537273
[d38319701]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-pressing-birtouta-kouba-mohammadia-alger-algerie-d38319701
[d39031109]: https://www.ouedkniss.com/hotellerie-restauration-salles-logiciel-programme-de-gestion-pour-magasins-restaurants-et-dalimentation-setif-algerie-d39031109
[d39454663]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-la-distribution-mobile-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A5%D8%AF%D8%A7%D8%B1%D8%A9-%D8%A7%D9%84%D8%AA%D9%88%D8%B2%D9%8A%D8%B9-%D8%A8%D8%A7%D8%B3%D8%AA%D8%AE%D8%AF%D8%A7%D9%85-%D8%AA%D8%B7%D8%A8%D9%8A%D9%82-%D9%87%D8%A7%D8%AA%D9%81%D9%8A-bouzareah-alger-algerie-d39454663
[d39656135]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-flexy-fastransfer-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D9%81%D9%84%D9%8A%D9%83%D8%B3%D9%8A-oran-algerie-d39656135
[d39658646]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-restaurant-pizzeria-cafeteria-cremerie-oran-algerie-d39658646
[d40249015]: https://www.ouedkniss.com/applications-logiciels-logiciel-pos-zsoft-gestion-de-ventestockcommerciale-alger-centre-algerie-d40249015
[d40287712]: https://www.ouedkniss.com/autre-pack-logiciel-pc-digital-imprimante-ticket-caisse-lecteur-code-a-barre-draria-alger-algerie-d40287712
[d40978507]: https://www.ouedkniss.com/logiciels-abonnements-smart-gym-logiciel-de-gestion-salle-sport-bab-ezzouar-alger-algerie-d40978507
[d44596393]: https://www.ouedkniss.com/logiciels-abonnements-procommerce-logiciel-de-gestion-stock-performant-pour-pme-bordj-el-kiffan-alger-algerie-d44596393
[d44602069]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A8%D9%8A%D8%B9-%D8%A7%D9%84%D8%B0%D9%87%D8%A8-logiciel-de-gestion-des-bijouteries-batna-algerie-d44602069
[d44928828]: https://www.ouedkniss.com/all-in-one-logiciel-de-gestion-commercial-dar-el-beida-alger-algerie-d44928828
[d45334057]: https://www.ouedkniss.com/securite-alarme-logiciel-salle-de-sport-avec-tourniquet-birkhadem-alger-algerie-d45334057
[d45414283]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-patisserie-oran-algerie-d45414283
[d45414436]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-professionel-boucherie-oran-algerie-d45414436
[d45454081]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-professionel-bijouterie-oran-algerie-d45454081
[d45540038]: https://www.ouedkniss.com/autre-systeme-de-caisse-pour-restaurant-logiciel-ecran-imprimante-tique-el-mouradia-alger-algerie-d45540038
[d46334731]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-bijouterie-birtouta-kouba-mohammadia-alger-algerie-d46334731
[d47974462]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-de-bijouterie-blida-algerie-d47974462
[d48086740]: https://www.ouedkniss.com/logiciels-abonnements-menu-max-logiciel-de-digitalisation-et-gestion-des-restaurants-cafes-alger-centre-algerie-d48086740
[d50074436]: https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-المحلات-التجارية-gestion-commercial-tipaza-algerie-d50074436
[d50571559]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-restauration-itri-resto-pro-mohammadia-alger-algerie-d50571559
[d50572699]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-pme-pro-mohammadia-alger-algerie-d50572699
[d51024226]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-shoperra-gestion-de-stock-et-clients-oran-algerie-d51024226
[d51171823]: https://www.ouedkniss.com/logiciels-abonnements-sama-vision-erp-logiciel-complet-pour-la-gestion-des-entreprises-et-points-de-vente-khraissia-alger-algerie-d51171823
[d51405412]: https://www.ouedkniss.com/ecrans-logiciel-de-vente-bzpos-original-bordj-bou-arreridj-algerie-d51405412
[d51577181]: https://www.ouedkniss.com/logiciels-abonnements-windev-25-cracke-kolea-tipaza-algerie-d51577181
[d52265413]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-hirosystem-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A3%D9%8A-%D9%86%D8%B4%D8%A7%D8%B7-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-birtouta-alger-algerie-d52265413
[d52503089]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-simple-fiable-et-efficace-mohammadia-alger-algerie-d52503089
[d52615477]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D8%A3%D9%82%D8%B3%D8%A7%D8%B7-%D9%84%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-logiciel-de-gestion-facilite-dar-chioukh-djelfa-algerie-d52615477
[d53418317]: https://www.ouedkniss.com/logiciels-abonnements-pack-boucherie-birtouta-alger-algerie-d53418317
[d54074597]: https://www.ouedkniss.com/logiciels-abonnements-kers-pos-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-%D9%81%D9%84%D9%83%D8%B3%D9%8A-%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D9%86%D9%82%D8%A7%D8%B7-%D8%A8%D9%8A%D8%B9-%D8%A7%D8%AD%D8%AA%D8%B1%D8%A7%D9%81%D9%8A-souk-ahras-algerie-d54074597
[d54257126]: https://www.ouedkniss.com/logiciels-abonnements-klick-stock-standard-logiciel-de-gestion-facturation-oran-algerie-d54257126
[d54491714]: https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-المحلات-التجارية-باحترافية-ghardaia-algerie-d54491714
[d55015843]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-facturation-en-ligne-innova-invoice-bir-el-djir-oran-algerie-d55015843
[d55066585]: https://www.ouedkniss.com/autre-caisse-logiciel-clyo-alger-centre-algerie-d55066585
[d55107943]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A3%D8%B1%D9%83%D8%A7%D9%86-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D8%B3%D9%88%D8%A8%D8%B1%D9%85%D8%A7%D8%B1%D9%83%D8%AA-%D9%86%D8%B8%D8%A7%D9%85-%D9%83%D8%A7%D8%B4%D9%8A%D8%B1-%D9%88%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D8%A7%D8%AD%D8%AA%D8%B1%D8%A7%D9%81%D9%8A-sidi-amar-annaba-algerie-d55107943
[d55281649]: https://www.ouedkniss.com/logiciels-abonnements-smartstock-logiciel-de-gestion-stock-professionnel-pour-windows-alger-centre-algerie-d55281649
[d55290118]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-caisse-et-gestion-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D9%83%D8%A7%D8%B4%D9%8A%D8%B1-%D9%88%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-bou-saada-msila-algerie-d55290118
[d55642837]: https://www.ouedkniss.com/logiciels-abonnements-kiober-food-pos-logiciel-de-caisse-pour-les-cafes-restaurants-pizzeria-et-snacks-oran-algerie-d55642837
[d55690765]: https://www.ouedkniss.com/logiciels-abonnements-istock-pro-2026-kouba-alger-algerie-d55690765
[d55706998]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-omni-caisse-pro-alger-centre-algerie-d55706998
[d55739143]: https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-محلات-احترافي-في-الجزائر-bordj-bou-arreridj-algerie-d55739143
[d55740700]: https://www.ouedkniss.com/logiciels-abonnements-kio-pos-logiciel-de-caisse-et-gestion-stock-alger-centre-algerie-d55740700
[d55784365]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D9%84%D8%A8%D9%8A%D8%B9-%D8%A8%D8%A7%D9%84%D8%AA%D8%AC%D8%B2%D8%A6%D8%A9-logiciel-du-ventesuperette-flamepos-alger-centre-algerie-d55784365
[d55843231]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-salle-sport-bir-chouhada-oum-el-bouaghi-algerie-d55843231
[d56041582]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-pressing-bab-ezzouar-alger-algerie-d56041582
[d56045476]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-kiopos-alger-centre-algerie-d56045476
[d56061973]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-csi-stock-gestion-point-de-vente-achats-et-ventes-grossiste-detail-multi-magasin-birkhadem-alger-algerie-d56061973
[d56087176]: https://www.ouedkniss.com/logiciels-abonnements-orbis-logiciel-de-gestion-pour-distributeurs-grossistes-commandes-stock-livraison-facture-cheraga-alger-algerie-d56087176
[d56142529]: https://www.ouedkniss.com/logiciels-abonnements-ampro-le-logiciel-de-gestion-commerciale-caisse-parfait-application-mobile-alger-centre-algerie-d56142529
[d56207773]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-magasin-stock-facile-complet-version-2026-douera-alger-algerie-d56207773
[d56299357]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-caisse-pro-kolea-tipaza-algerie-d56299357
[d56301202]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-commercial-pos-dahabia-gestion-supermarche-ou-grossiste-a-vie-sans-abonnement-laghouat-algerie-d56301202
[d56350090]: https://www.ouedkniss.com/projets-etudes-systeme-de-gestion-boutique-telephonie-boumerdes-algerie-d56350090
[d56453599]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-de-stock-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D9%83%D8%B1%D9%8A%D8%AF%D9%8A-%D8%A8%D8%A7%D9%84%D8%A8%D8%A7%D8%B1%D9%83%D9%88%D8%AF-sidi-amar-annaba-algerie-d56453599
[d56463136]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-flexy-reparation-vent-kers-souk-ahras-algerie-d56463136
[d56562698]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-stock-et-caisse-pos-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-mekastore-starter-bab-ezzouar-alger-algerie-d56562698
[d56573309]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-caisse-pos-multisphere-com-silver-setif-algerie-d56573309
[d56592551]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-salle-sport-titaou-gym-ultimate-برنامج-تسيير-قاعات-الرياضة-احترافي-khenchela-algerie-d56592551
[d56709041]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-facturation-digifact-tlemcen-algerie-d56709041
[d56746856]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-salle-de-sport-oran-algerie-d56746856
[d56826623]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-dipposoft-%D8%A7%D9%84%D8%B4%D8%A7%D9%85%D9%84-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D9%85%D8%A4%D8%B3%D8%B3%D8%A7%D8%AA-gestion-de-stock-facturation-djelfa-algerie-d56826623
[d56860799]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-complet-pour-entreprises-et-pme-batna-bejaia-biskra-blida-tlemcen-algerie-d56860799
[d56978620]: https://www.ouedkniss.com/maintenance-informatique-logiciel-de-vente-pos-restaurant-draria-alger-algerie-d56978620
[d56993778]: https://www.ouedkniss.com/logiciels-abonnements-alternative-algerienne-a-sage-et-ebp-alger-centre-bir-el-djir-oran-algerie-d56993778
[d57214320]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-caisse-gestion-premium-echo-pour-salons-barbershops-et-spas-alger-centre-algerie-d57214320
[d57221382]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-caisse-et-gestion-stock-point-vente-pc-alimentation-boucherie-telephonie-ghriss-mascara-algerie-d57221382
[d57221406]: https://www.ouedkniss.com/logiciels-abonnements-ds-rapide-logiciel-de-caisse-100-clavier-vente-gros-et-demi-ghriss-mascara-algerie-d57221406
[d57312381]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D9%88%D8%A7%D9%84%D9%85%D8%A8%D9%8A%D8%B9%D8%A7%D8%AA-flampex-logiciel-de-gestion-hussein-dey-alger-algerie-d57312381
[d57434988]: https://www.ouedkniss.com/bureautique-internet-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D9%88%D8%B8%D9%81%D9%8A%D9%8A%D9%86-etat-204-el-attaf-ain-defla-algerie-d57434988
[d57473655]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-specialise-pour-restaurantcafefast-foodfranchise-bir-el-djir-oran-algerie-d57473655
[d57517611]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-pro-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D8%AD%D8%AA%D8%B1%D8%A7%D9%81%D9%8A-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D9%83-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-bab-ezzouar-alger-algerie-d57517611
[d57584481]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%B5%D9%8A%D8%A7%D9%86%D8%A9-%D8%A7%D9%84%D9%87%D9%88%D8%A7%D8%AA%D9%81-dukan-pro-alger-centre-algerie-d57584481
[d57648717]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-salle-sport-hadjout-tipaza-algerie-d57648717
[d57659580]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-restaurant-cafeteria-pizzeria-caisse-tactile-el-eulma-setif-algerie-d57659580
[d57732597]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-stock-gestock-khemis-el-khechna-boumerdes-algerie-d57732597
[d57850917]: https://www.ouedkniss.com/pc-de-bureau-logiciel-boucherie-tlemcen-algerie-d57850917
[d57871113]: https://www.ouedkniss.com/pc-de-bureau-logiciel-restauration-cafes-et-patisseries-tebessa-algerie-d57871113
[d57939003]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-inventaire-supperette-ats-pos-320-base-de-donne-configuration-annaba-algerie-d57939003
[d57957894]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-caisse-pos-et-gestion-stock-pour-tout-commerce-tlemcen-algerie-d57957894

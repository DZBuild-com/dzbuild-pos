# 09 — Completeness critique: gaps in files 01–08 and the additions that close them

> Research date: 2026-09-28. Role: completeness critic for DZBuild POS (free, offline-first, installable Windows POS + gestion commerciale for Algeria, optionally linked to dzbuild.dev via OAuth).
> Input read in full: `02-algeria-verticals-arabic.md`, `03-global-pos-features.md`, `04-algeria-legal-fiscal.md`, `05-hardware-environment.md`, `06-dzbuild-integration.md`, `07-tech-stack.md`, plus the draft text of report 01 (see gap P1: that file was not yet in the folder).

---

## 0. Method, tags and limits

**How the gaps were found.** Every file was read and cross-checked against the other seven and against the user's goal: "the most complete POS in Algeria, everything users use and need, import/export, offline, installable, linkable to dzbuild.dev". Each gap was then researched directly.

**How the additions were researched.** Bing, Google, DuckDuckGo, Qwant, Mojeek, Ecosia and SearXNG all served bot challenges or poisoned results. So every addition below comes from **primary sources fetched directly**:
- JORADP PDFs (Journal officiel), extracted with `pdftotext`;
- the Ministry of Commerce regulation portal (commerce.gov.dz);
- the DGI 2026 codes and circulars (mfdgi.gov.dz, already downloaded by track 04, re-read here);
- the Ministry of Religious Affairs site (marw.dz);
- GIE Monétique (giemonetique.dz);
- the **Ouedkniss public GraphQL API** (the same method track 01 used): 26 new searches on 2026-09-28, with listing details and view counts;
- vendor sites (stock.klick.dz, almawarid.app, yassir.com, slick-pay.com);
- the GitHub API (`gh`), Microsoft Learn, the World Bank API;
- **local verification** on this Windows 11 machine (ICU time zones, Hijri formatting, the Windows OCR engine).

**Tags used**
- **[V]** verified from the cited source.
- **[L]** verified locally on this machine (command shown or described).
- **[S]** seen only in a listing or vendor marketing (a claim, not independently checked).
- **[U]** unverified: design recommendation or inference, no source found.

Ouedkniss links use the pattern `https://www.ouedkniss.com/<slug>-d<ID>`; the full URLs are in §13.

---

## 1. Executive summary: the 30 gaps that matter most

| # | Gap | Where it should have been | Addition (this file) | Priority |
|---|---|---|---|---|
| 1 | **Report 01 was missing from the research folder**; only a draft existed | docs/research/01-*.md | Save the 01 draft as `01-algeria-pos-vendors.md` before the design phase (§2.1) | Blocker |
| 2 | **A truly free, pan-African competitor already exists:** Klick **Stock Express** is "GRATUIT à vie", Windows 7–11, full sales/purchase cycle + counter sales. 01 said "limits unknown" | 01 | Verified feature split: the free edition lacks customer balances, multi-depot, price tiers, colisage, inventory, reports and treasury (those are in Stock Standard, **10,000 DA** perpetual in Algeria) (§3.1) | High |
| 3 | **02 says no pressing software and no butcher-specific software exists in Algeria. Both are wrong.** | 02 §3.3, §3.15 | 5 pressing products (18–30k DA; one listing has 11,902 views) and 4 butcher offers incl. scale packs (§4.1, §4.2) | Correction |
| 4 | **Jewellery (bijouterie) vertical is missing**, and it now carries **new 2026 legal duties** | 01/02/04 | 4 Algerian jewellery products; features (gold price per gram, broken-gold exchange, melting, repairs); **CII art. 359 as amended by LF 2026 art. 62**: special client register (nature, number, weight, fineness, price, buyer identity), 5-year retention, activity authorisation (§4.3, §5.4) | High |
| 5 | **Gym / membership vertical missing** (21 Ouedkniss listings, up to 10,623 views) | 02 | Subscriptions, visit counters, RFID/fingerprint/face access via ZKTeco, turnstiles (§4.4) | V1 pack |
| 6 | **Consumer warranty law missing**: décret exécutif 13-327 | 04 | Mandatory warranty certificate mentions, minimum 6 months new / 3 months used, +30 days when immobilised, 30-day replace/refund (§5.1) | High (phones, appliances) |
| 7 | **Sales/promotions law missing**: décret exécutif 06-215 | 03 promo engine, 04 | Soldes only 2×/year, 6 continuous weeks, Jan–Feb and Jul–Aug; goods held ≥ 3 months; prior declaration with prior prices; promos also declared (§5.2) | High |
| 8 | **Regulated consumer prices missing** (oil, sugar, milk sachet, semolina) | 04 §9 flags "prix réglementé" without data | Verified ceilings from JO: oil 5 L 650 DA, 2 L 250, 1 L 125; sugar 90/95 DA/kg; milk sachet 25 DA/L; semolina tables (§5.3) | High (superettes) |
| 9 | **"Fictive invoices / non-billable stock" legal exposure not quantified** (01 only says "grey area") | 01 §12, 04 | CIDTA art. 303 as amended by LF 2026 art. 25: fraud **by IT means** → 5–10 years + 5–10 M DA; **accomplices get the same penalties**; CTCA 118 lists "ventes sans facture" and fictive entries as fraudulent. Combined with CPF 51 sexies (publisher must hand over code), a publisher shipping such features is exposed (§5.5) | Design decision |
| 10 | Zakat is listed as a differentiator but **no official nisab source** | 02 | Ministry of Religious Affairs: **nisab 1447 AH = 2,295,000 DA** (§5.6) | V1 |
| 11 | **Annual inventory / "livre d'inventaire"** not modelled | 03, 04 | Code de commerce arts. 9–10 referenced by CIDTA 193 and CTCA 118 (§5.7) | V1 |
| 12 | Cheques & **traites** (bills of exchange) portfolio, due dates, bounced cheques, **dunning letters** | 03 PAY-028 only captures references | Seen in a 25k DA Sétif product (§6.4) | V1 |
| 13 | **Dot-matrix multi-copy printing** (Epson LX-350 / LX-300 / LQ-2090) ignored | 05 | 48 Ouedkniss listings; LX-350 at 46,300–53,000 DA; "1 original + 4 copies" (§7.1) | V1 |
| 14 | **Standalone cash registers** (Casio SE-G1, Sharp XE-A107, ~25–26k DA) are a competitor nobody mentions | 01, 05 | §3.3, §7.2 | Positioning |
| 15 | **GIE Monétique certification** scope not stated; ECR integration unknown | 04, 05 | Certification covers "equipment and all software needed to interact with the interbank system"; **21 certified TPEs**, most Android (PAX A8900/A920, SZZT KS8226, Topwise, Morefun, Verifone X990…) with the embedded "CIBPAY" app; 3 GIE-approved mobile payment solutions (§5.8) | Watch |
| 16 | **Slick-Pay** marked unverified | 04 §11.2 | Official SDKs verified (accounts, transfers, aggregations, invoices, merchant invoices; sandbox) (§5.9) | Opt |
| 17 | **Aggregator orders (Yassir)** unmodelled | 03 RST-015 "watch" | Yassir restaurants use a separate "Yassir Store" app; no POS API advertised → manual "channel" + commission (§6.6) | V1 (resto) |
| 18 | **Offline SMS/USSD via a USB GSM modem** (flexy, debt reminders) not specified | 02, 05 | Huawei E3531 at 3,800 DA sold explicitly for "send/receive SMS & USSD" (§7.3) | V1 |
| 19 | **CCTV POS text overlay** marked n/a | 03 AUD-006 | Hikvision NVR "POS" function receives receipt text over TCP and overlays it on video, searchable in iVMS-4200 (§7.4) | V2 |
| 20 | **Legacy data migration** from competitors (WinDev/HFSQL, DBF) is a "feasibility to verify" | 01 §12.3 | PC SOFT distributes an HFSQL ODBC driver; an MIT/Apache Rust reader for `.fic/.ndx/.mmo` exists (v0.1); a competitor exports DBF (§8) | V1 |
| 21 | **Bank statement import** (AFB format) not listed | 03 IMP | A Sétif vendor claims automatic bank reconciliation "via Net banking, format AFB" [S] (§8) | V2 |
| 22 | **Contradiction: Windows 7.** 05 says Tauri can still target Win7; 07 shows **Tauri 2.12 (2026-09-26) dropped Win7** and Rust needs Win10+ | 05 §7.2 vs 07 §2.1 | 07 supersedes 05; Win7 is out (§2.2) | Fix doc |
| 23 | **Contradiction: Almawarid pricing** (01 = 20,000 DA/yr; 02 = 10,000/18,000 DA/yr) | 01 vs 02 | The live pricing page now says "Tarif sur demande" for every plan, with document caps; both figures are stale (§2.2) | Fix doc |
| 24 | **Contradiction: timbre wording.** 02 calls the 2025 stamp duty "progressive"; 04 (DGI circular 14/2025) says explicitly **not progressive** | 02 §1.4 vs 04 §4.2 | 04 is authoritative (§2.2) | Fix doc |
| 25 | 04 open questions on **69 wilayas** | 04 §16.9 | **Verified:** loi 26-06, JO n° 25 of 5 April 2026 (69 wilayas / 1,541 communes); **Bir El Ater = Bir El Ater, Negrine, El Ogla El Melha, Ferkane**, taken from Tébessa (now 24 communes); names/seats by **décret présidentiel 26-206** (JO n° 40/2026), official spelling "**Aïn Ouessara**" (04 wrote "Aïn Oussera") (§5.10) | Fix data |
| 26 | 07 left **Africa/Algiers = UTC+1, no DST** unverified | 07 §5.4 | [L] verified: UTC+01:00 in January and July (ICU) (§10.2) | Closed |
| 27 | 07 left **Microsoft Store in Algeria** unverified | 07 §13.2 | Algeria **is** in the Microsoft Store consumer-markets list [V]; developer-account eligibility from Algeria still unverified (§10.4) | Closed/partial |
| 28 | **Supplier-invoice OCR offline in Arabic** assumed easy (competitors sell "AI OCR") | 01, 02 | [L] Windows' built-in OCR engine on this PC offers only en-US and fr-FR; Arabic needs an extra language pack or Tesseract (§10.3) | V2 |
| 29 | **UX for older / low-literacy users and AZERTY laptops** barely specified | 03 L10N/UX | Data: literacy 75.1 % (2008, latest WB), 65+ = 6.8 % (2025); AZERTY digits need Shift; concrete UI rules (§9) | M |
| 30 | **No unified requirements backlog, competitor scoring or go-to-market/support plan** (a free product still needs support at scale and a hardware-reseller channel) | all | §11 lists what the next document must contain | Next step |

---

## 2. Process gaps and contradictions in the research set

### 2.1 Missing or structural gaps

| ID | Gap | Evidence | What to do |
|---|---|---|---|
| P1 | **01 is not in the folder.** `docs/research/` contains 02–08 only (listed 2026-09-28). The full 01 text, with its Ouedkniss link definitions, exists only as a draft. | `ls docs/research` | Write the 01 draft verbatim to `01-algeria-pos-vendors.md`. It is the market map every other file points to ("confirm in DZ docs"). |
| P2 | No **single consolidated requirements list**. Five overlapping lists exist: 01 §9 (catalogue), 02 §7, 03 §3/§5 (547 features, but global), 04 §15, 05 §13. | files | Merge into one backlog with IDs, source file, legal/market/UX origin and release (M/V1/V2). §6 of this file adds the missing rows. |
| P3 | No **competitor scoring matrix** (DZBuild vs the top local products on the ~60 features buyers ask about). | 01 has lists, not a matrix | Build it for: Klick Stock Express, Crescendo Free, Almawarid, AlgoStock, KERS POS, TifawtTech PME PRO, Dolisoft, Zedkira, CaisseKom, GestiumPRO. |
| P4 | Everything is **desk research**. No shopkeeper interviews, no hands-on test of competitor software. | method sections of 01–05 | Install the free/demo versions (Stock Express, Crescendo Free, Almawarid Desktop) and interview 10–15 shops (superette, butcher, phones, clothing, wholesale, café) before freezing MVP. |
| P5 | **Licence model of DZBuild POS itself is undecided** (proprietary freeware vs open source). It changes code-signing options (§10.4), trust, and the exposure created by CPF 51 sexies (the tax office may demand code). | 06 §5.7, 07 §11 | Decide early; see §10.4. |
| P6 | **DZBuild as "éditeur" in 2027.** 04 explains what users must show; nobody analysed what DZBuild signs (the publisher engagement), its liability, or its obligation to provide "tous codes, données, traitements ou documentation" (CPF 51 sexies). | 04 §6.1 | Legal review before 1 Jan 2027; design the fiscal journal so the engagement is truthful. |
| P7 | **Support model for a free product** is missing. Competitors sell installation over AnyDesk/TeamViewer, WhatsApp support and "we enter your products" (01 §6, §8). | 01 | §11. |

### 2.2 Contradictions and stale facts to fix

| Topic | File A says | File B says | Verified now | Fix |
|---|---|---|---|---|
| Windows 7 support | 05 §7.2: Tauri 2 targets Win7 with the embedded bootstrapper; "Tauri is the only mainstream option" | 07 §2.1: **Tauri 2.12 (2026-09-26) dropped Win7**; Rust targets need Win10+ since 1.78 | 07 cites the Tauri 2.12 blog and the Rust blog | Treat 05 §7.2 as superseded. Minimum is Windows 10 1809+. |
| Almawarid price | 01: Standard **20,000 DA/yr**, Enterprise 500 docs/month | 02: Standard **10,000 DA/yr** (≤ 5 employees), Enterprise **18,000 DA/yr** | almawarid.app/tarifs (fetched 2026-09-28) shows every plan (Duo/Standard/Entreprise/Bronze→Diamond) as **"Tarif sur demande"**, with limits such as "50 pièces/mois · 1 000 produits" and "100 pièces/mois · 1 500 produits", "licence à vie desktop disponible". An Ouedkniss listing by Almawarid says "dès 15 000 DA/an" [S] | Record Almawarid as "price on request (Sept 2026), historical 10–20k DA/yr, document-capped plans". |
| Stamp duty | 02 §1.4: "progressive 1 / 1.5 / 2 DA per 100 DA tranche" | 04 §4.2: DGI circular 14/2025 says it "n'est pas soumis à une progressivité par tranche" | 04 quotes the circular | Use 04's algorithm; remove "progressive" wording. |
| Pressing / butcher software | 02: none verified in Algeria | — | Ouedkniss API: 5 pressing + 4 butcher offers (§4.1–4.2) | Correct 02. |
| Klick "free" | 01: "free, limits unknown" | — | stock.klick.dz: free for life; feature split verified (§3.1) | Update 01. |
| Wilaya 65 name | 04 §12.3 "Aïn Oussera" | — | Décret 26-206: "Wilaya de **Aïn Ouessara**" | Use the JO spelling in datasets. |
| Bir El Ater parent | 04: "verify against the JO" | — | Loi 26-06 art. 52 bis 13 lists the 4 communes; Tébessa's article now lists 24 communes (§5.10) | Closed. |
| Payroll | 03 EMP-009: payroll "n/a" | 01/02: Almawarid, Winalco, Megasoft, Devoro sell CNAS/IRG payroll | IRG annual barème verified in CIDTA 2026 art. 104 (§5.11) | Offer a "salaires & avances" lite module (V2), not full payroll. |
| ar-DZ number format | 07: `1.234.567,50 د.ج.‏` | 04: suggests `1 234,56 دج` with U+202F | [L] `Intl.NumberFormat('ar-DZ')` → `1.234.567,5` (dot grouping, Latin digits) | Pick one house style and override ICU; don't mix. |

---

## 3. Algerian products and competitors not covered (or mis-described)

### 3.1 The free competitor: Klick Stock Express (Fouka, Tipaza) [V]

Source: https://stock.klick.dz/ and https://stock.klick.dz/fr/prix (fetched 2026-09-28).

| Item | Finding |
|---|---|
| Price | Stock Express: **"GRATUIT à vie"**. Stock Standard: **59.99 € / 10,000 DA** (Algeria, local partner), perpetual licence; also sold in Morocco (840 MAD), Tunisia (210 DT) and ~13 African countries. |
| OS | "Fonctionne sur : Windows 7, 8, 8.1, 10 et 11". Version 2026.7 (June); Standard 2026.8 (August). |
| Free edition features | Full sales cycle (devis, commandes, BL, factures, avoirs clients), full purchase cycle (commandes, bons de réception, factures/avoirs fournisseurs), **ventes comptoir**, contacts, POS hardware (scanners, ticket printers, drawers), print customisation (invoices, tickets, barcodes), flexible taxes and withholdings. |
| Headline feature | **"Séparez facilement votre stock facturable de celui non facturable"**: two stocks per product. This is the grey-area practice analysed in §5.5. |
| Only in the paid Standard | monoposte/multiposte/cloud, **customer payments and balances**, **multi-depot**, **price tiers**, **colisage**, **inventory**, **reporting**, **treasury** (tills, banks, expenses). Client/server edition available. |
| International extras | e-MECeF normalised invoices (Benin) and **Peppol** XML e-invoicing (Belgium). Not relevant to Algeria. |

**Implication:** "free" alone is not a differentiator. DZBuild must be free **and** include everything Klick paywalls (credit/kridi, multi-depot, price levels, colisage, inventory, reports, treasury), plus Arabic RTL, scales and multi-till offline, and compliance (2027 journal, timbre).

### 3.2 Other Algerian products missing from 01/02 (Ouedkniss API, 2026-09-28)

| Product / seller | Where | Price (DA) | Views | What's notable | Source |
|---|---|---|---|---|---|
| **SolutionInfoDZ** | Bordj Bou Arreridj | **from 3,500**, lifetime activation, free trial | 43 | Cashier + stock + barcode + **customer credit (كريدي)**, offline. New price floor. | d55739143 [S] |
| "Logiciel de vente… vendu à plus de 10 000 clients" | Bab El Oued, Alger | 30,000 | **21,631** | Claims 15 years and 10,000+ clients (unverified claim) | d20975347 [S] |
| **Logiwell** (#LOGIWELL) | Tipaza | 15,900 | 237 | Wholesale or retail; free trial install + training; COD delivery to all wilayas. Buyer comment: an **instalment-sales company** wants to replace its old software. | d50074436 [S] |
| Ghardaïa gestion commerciale | Ghardaïa | 35,000 once | 96 | Restaurants, cafés, superettes, cosmetics, clothing, phones; installation + training | d54491714 [S] |
| **ProCommerce** (Dif Softs; "contrairement à ProPOS") | Bordj El Kiffan | 12,000 | 193 | Mid-size businesses; **Windows and macOS** claimed; treasury with due-date alerts | d44596393 [S] |
| Sétif gestion commerciale | Sétif | 25,000 | 299 | **Cheques/traites due dates (clients & suppliers), dunning letters, multi-bank/multi-till treasury, automatic bank reconciliation "via Net banking, format AFB"**, monthly TVA declaration, chart of accounts + accounting export, data-collector import, lots/serials, composite items, PMP | d32976718 [S] |
| **GECO SOFT** | Sétif | 40,000 | **11,373** | Multi-company; XP → Win10; **exports Excel, DBF, TXT, RTF**; dead stock, expired stock, stock images | d16024589 [S] |
| **Supersoft** | Bab Ezzouar / Constantine / Oran | n/a | 4,734 | Range: retail/wholesale, fast food, **mobile app for vendeur ambulant**, instalment shops, **fuel station**, **dégraissage**, **gym**, dental/medical, production | d26141727 [S] |
| **TradMan** | Alger centre | n/a | **12,518** | Gestion commerciale, "tout domaine du commerce et prestation de service" | d11537219 [S] |
| **UniSoft** | Dar El Beida | n/a | 865 | Range of commercial software | d15825713 [S] |
| Tébessa resto/café/pâtisserie | Tébessa | 85,000 | 6 | Restaurant + pastry | d57871113 [S] |
| **CaisseKom** verticals (Computime, Oran) | Oran | Pâtisserie 30,000; Bijouterie / Boucherie / Salle de sport on request | 117–10,623 | One vendor, many vertical editions | d45414283, d45454081, d45414436, d14437207 [S] |
| Phone-shop management system | Boumerdès | 35,000 | — | Dedicated phone/electronics shop system | d56350090 [S] |
| YBServices | Dar El Beida | n/a | — | POS for superette, cosmetics, **bureau tabac**, restaurant, clothing, appliances, spare parts, tea room | d24408171 [S] |

### 3.3 The "old-school" competitor: standalone electronic cash registers [V listing]

- **Casio SE-G1** (new, boxed): 26,000 DA, 1,015 views: d27284116.
- **Sharp XE-A107**: 25,000 DA, 1,345 views: d26906791.

No stock, credit or Arabic. They show the price a very small shop pays for "a till" today. DZBuild's pitch to this segment is an old PC plus the free software, at the same money or less. Refurbished PCs cost 18,500 DA (05 §6.3).

### 3.4 Foreign / ERP presence: small verifications

- **Odoo partners listed for Algeria** on odoo.com (2026-09-28): Insidjam COM, SIBIC, EURL BUSINESNA, FINOUTSOURCE Sarl, Cetelex, AH Consulting, HORIA TECHNOLOGIES [V] (https://www.odoo.com/fr_FR/partners/country/algerie-61). This complements 01's list (ITHRYTEC, Digitaliac, Digisys, Numidit are not in the official directory page).
- **Loyverse, Daftra, DEXEF:** still **unverified** for Algeria. No Algeria-specific page found (daftra.com/dz → 404; dexef.com/dz → 404).

---

## 4. Verticals not covered, or corrected

### 4.1 Pressing / dry cleaning: products exist (correction to 02 §3.15)

| Product | Price | Views | Features claimed | Source |
|---|---|---|---|---|
| **ITRI Pressing** (General IT) | on request | 948 | Orders, customer tracking, invoicing | d38319701 [S] |
| Tpv Technologies "LOGICIEL PRESSING" | 30,000 | **11,902** | Purchases, sales, suppliers, customers, charges and losses, statistics | d18280273 [S] |
| Bab Ezzouar pressing & blanchisserie | 18,000 | 12 | Orders and customer follow-up | d56041582 [S] |
| Constantine "pressing dégraissage laverie" | n/a | 564 | Sales and **receipt slips (الوصل)**, garment categories, service price list, customers, reports | d30243863 [S] |
| REVEXN System (Oran) | 25,000 | 5,496 | Cash sales, products, categories, customers, admin | d25229062 [S] |

**Needs (from 02 §3.15 + these listings):** numbered deposit ticket per garment with tag, service price grid (lavage/repassage/nettoyage à sec), promised date, statuses, deposit/balance, "ready" SMS, uncollected-items report.

**Build once, reuse everywhere:** implement a generic "**service ticket**" module (intake → status → pickup). It covers pressing, phone repair, tailoring, cordonnerie and optician lab orders.

### 4.2 Butcher / scale: offers exist (correction to 02 §3.3)

- CaisseKom **Boucherie** (Oran, Computime): d45414436 [S].
- **"Pack boucherie"** (Birtouta): Enigma touch PC + fixed scanner + **electronic scale** + drawer + printer + lifetime-activated software: d53418317 [S].
- "Logiciel boucherie + PC" (Tlemcen): d57850917 [S]. "Logiciel pour boucherie" (Bordj El Kiffan): d28923331 [S].
- DS SOFTWARE lists a boucherie edition (01).

**Still an opening:** none advertises carcass-yield ("achat carcasse → découpe"), PLU push to label scales, or live serial scale reading. 05 §4 already covers the technical side.

### 4.3 Jewellery (bijouterie / محلات الذهب): missing vertical, with 2026 legal duties

**Products** [S]:
- **ITRI Bijouterie** (General IT; 462 views): stock with weight, metal and stone; unique numbering and traceability; repairs; special orders; quotes and invoices; loyalty. d46334731.
- Blida jewellery software: stock of gold/silver/stones; **exchange of broken jewellery against cash** (الاستبدال بين المجوهرات المكسورة والنقد); **melting (الصهر / fonte)** management; customer and supplier credit; repairs and special orders; reports on sales, exchanges, melting and credit. d47974462.
- Batna: AR/FR/EN; **"automatic price change as soon as the gold price changes"**; customer instalments. d44602069.
- CaisseKom Bijouterie (Oran): d45454081.

**Features to add (design, from the above + §5.4):**
- per-piece weight (g, 2–3 decimals);
- fineness/titre (e.g. 750‰ = 18k);
- stone details;
- **daily gold price per gram per titre**, with sale price = weight × price + façon (making charge);
- piece-level serial/tag and label printing;
- old-gold buyback and exchange (weight, titre, loss %, buyer ID);
- melting batches;
- repairs (service ticket);
- the **legal client register** printout;
- a cash ceiling warning: ≥ 1,000,000 DA for precious metals must go through bank channels (décret 15-153, already in 04 §11.1).

### 4.4 Gym / subscriptions / memberships: missing vertical

**Evidence** [S]: **21 listings** for "logiciel salle de sport":
- CaisseKom salle de sport (Oran): **10,623 views**
- LOG103 (Sétif): **8,248 views**
- Kinetic Gym (Tipaza): 15,000 DA
- Gym Manager Pro (Oum El Bouaghi): 20,000 DA, "adhérents, abonnements, contrôle d'accès RFID"
- Smart Gym (Bab Ezzouar)
- Titaou Gym Ultimate (Khenchela)
- Oran: face recognition, RFID cards and tags
- Tlemcen: ZKTeco fingerprint/face/RFID
- Birkhadem: with tripod turnstile
- ProAccess (with swimming pool)

Supersoft (§3.2) and Almawarid also cover gyms.

**Needs:**
- members with photo;
- plans (monthly, per-session, **N-visit cards**);
- expiry alerts;
- entry/exit log;
- access control through ZKTeco (TCP 4370; the libraries are in 05 §10) or turnstile relay;
- product sales (supplements, drinks) on the same ticket;
- debt/credit.

This is the same "subscriptions" engine a **private school**, **hammam** or **parking** would use. Ship it as an optional pack (V1/V2).

### 4.5 Other verticals: still thin

| Vertical | Status | What's needed | Evidence |
|---|---|---|---|
| **Bureau de tabac / kiosque journaux / multiservices** (flexy, cards, photocopies, bill payment) | Mentioned only (easykes, YBServices) | Many sub-€1 items with no barcode (department keys, §6.1), newspaper returns (invendus) to distributor, scratch-card stock with PIN log, flexy float, photocopy/print counters per page | d12993513, d24408171 [S]; "logiciel multiservices cybercafé" returned 0 results [V] |
| **Returnable containers (consignes)**: butane gas cylinders, beverage crates/bottles | Only COMMSoft (01) | Deposit amount per container, empty-for-full exchange, container balance per customer, deposit line on invoice (décret 05-468 art. 9 already requires it: 04 §2.1) | "logiciel gaz butane" / "dépôt boissons" returned no software [V] → opening [U] |
| **Regulated staples in superettes** | Not covered | Ceiling-price checks and "subventionné" flags (§5.3) | JO |
| **Pâtisserie** | Covered by 02 (bakery) | CaisseKom Pâtisserie 30,000 DA (d45414283) confirms a dedicated market | [S] |
| **Fuel stations** | Supersoft, GestiumPRO | Out of scope (pump controllers) | [S] |
| **Veterinary / agri-inputs** | Only clinic software (Vetosoft 45,000 DA) | Lots and expiry, regulated products; low priority | [S] |
| **Car wash, tailoring, cordonnerie** | none found | Covered by the generic service-ticket module (§4.1) | "logiciel lavage auto" → 0 [V] |
| **Vente ambulante / van sales** | 01 (Orbis, PMO Pro), Supersoft mobile app | Van as a depot, offline Android pre-sale; the Windows POS needs a van-stock transfer + reconciliation screen | [S] |

---

## 5. Regulatory additions (verified from primary texts)

### 5.1 Warranty of goods and services: décret exécutif 13-327 (JO n° 49 of 2 Oct 2013) [V]

Source: https://dcwalger.dz/fr/images/stories/PDF/garantie/dec13327fr.pdf (JO extract). Applies loi 09-03 (consumer protection) art. 13.

| Rule | Article | POS implication |
|---|---|---|
| Warranty takes effect at delivery and is materialised by a **certificat de garantie** handed "de plein droit" | art. 5 | Print a warranty certificate from the sale (per serial/IMEI line) |
| Mandatory mentions: guarantor's name/raison sociale, address, **RC number**, email if any; **buyer's name**; **number and date of the invoice / ticket de caisse / bon d'achat**; nature of the good (**type, brand, serial number**); **price**; **warranty duration**; representative handling the warranty, if any | art. 6 | Certificate template = exactly these fields; capture buyer name for warranted goods |
| Model certificate fixed by arrêté | art. 7 | Arrêté **[U]** (not fetched); keep the template editable |
| If no certificate, the invoice or ticket proves the warranty | art. 8 | Receipt search by serial |
| Remedy: repair, replacement or refund, at no cost; repeated failure → replace or refund | art. 12 | RMA workflow: repair / replace / refund |
| Replace or refund within **30 days** of declaring the defect if repair is impossible | art. 15 | SLA timer on warranty claims |
| **Minimum duration 6 months (new)**; per-product durations by arrêté | art. 16 | Default 6 months; per-category override; per-product arrêté list **[U]** |
| **Minimum 3 months for second-hand goods** | art. 17 | Phone shops selling used devices: flag "occasion" |
| **+30 days** added when the good is immobilised for repair | art. 20 | Auto-extend warranty end date |
| Claim, optional joint inspection within 10 days, 30-day execution, then formal notice + 30 days | arts. 21–22 | Claim log with dates |

**Gap closed:** 03 has only "warranty per serial" (PRD-054, V1). In Algeria this is a legal document for phone, electronics and appliance shops, so MVP for those packs.

### 5.2 Soldes, promotions, liquidation, factory shops, déballage: décret exécutif 06-215 (JO n° 41 of 21 June 2006) [V]

Source: https://www.dcwalger.dz/fr/images/stories/solde/Decret%20executif%20n%2006_215%20du%2018%20juin%202006.pdf (also listed at https://commerce.gov.dz/fr/reglementation/decret-executif-n-deg-06-215).

| Rule | Article | POS implication |
|---|---|---|
| Soldes may only cover goods **held ≥ 3 months** before the soldes start | 2 | Stock-age check (needs receipt dates per lot, already in the FIFO design) |
| **Twice a year, 6 continuous weeks**, winter period **January–February**, summer **July–August**; exact dates fixed yearly by **arrêté du wali** | 3–4 | "Soldes" campaign type limited to configured windows per wilaya; warn outside them |
| Display start/end dates, goods concerned, **previous prices** and reductions (fixed or graduated); soldes goods shown separately | 5 | Promo engine must store **prix pratiqué auparavant**; print "was/now" labels |
| **Prior declaration** to the Direction de wilaya du commerce with RC extract, **list and quantities of goods**, and **table of reductions with previous prices** → authorisation issued on the spot | 6 | One-click export: "état des articles soldés" (designation, qty, old price, new price, %) |
| **Promotional sales** (any technique to attract/retain customers) also require a declaration: start/end, techniques and promo prices, list of goods; bailiff if a prize draw | 7–8 | Same export for promotions; store campaign dates and techniques |
| Prize draws **cannot be tied to a purchase** or payment | 9 | Don't build "ticket = lottery entry" features |
| Liquidation of stock needs a declaration + inventory with prices | 10–12 | Export of the liquidation inventory |
| Undeclared soldes/promos → **immediate stop**; regularisation only within 3 days; deceptive advertising punished (loi 04-02 art. 38); seizure possible | 23–29 | Compliance warning in the promo UI |

**Gap closed:** 03's promotions engine (DSC-005…020) has no legal layer. Algeria's rules make "prior price" storage and the declaration export mandatory for clothing/shoe shops (02 §3.8 "soldes").

**Amendments:** none found on the Commerce portal's "Vente en soldes" recueil. Whether 06-215 was amended after 2006 is **[U]**.

### 5.3 Regulated consumer prices (prix plafonnés) [V]

Sources: the JO issues listed below (fetched and parsed); index on https://www.commerce.gov.dz/fr/reglementation/recueil/prix-reglementes

| Product | Text | Consumer ceiling (TTC) | Other duties |
|---|---|---|---|
| Refined ordinary edible oil | décret 11-108 (JO 15/2011) as amended by **décret 21-383** (JO n° 77, 10 Oct 2021) | **5 L: 650 DA** (was 600); 2 L: 250 DA; 1 L: 125 DA | Packs for consumers must say "huile alimentaire raffinée ordinaire subventionnée", show the capped price and carry a **5 cm red band**; professional packs ≥ 10 L |
| White sugar | décret 11-108 (unchanged by 21-383) | **90 DA/kg bulk; 95 DA/kg prepacked** | "sucre blanc subventionné"; professional packs ≥ 10 kg; **cafés, restaurants and fast-food must use sugar in dosettes** |
| Pasteurised milk in sachet (partially skimmed, subsidised) | décret 01-50 as amended by **décret 22-186** (JO n° 33, 17 May 2022) | **25.00 DA/L** (factory 21.00, delivered 23.00) | — |
| Durum-wheat semolina | décret 07-402 as amended by **décret 20-242** (JO n° 52, 2 Sept 2020) | Courante / Extra: **1 kg 38.5 / 42.5; 2 kg 76 / 84; 5 kg 187.5 / 207.5; 10 kg 370 / 410 DA**; retail margins 2/3, 4/6, 10/15, 20/30 DA | Bags must say "semoule courante/extra subventionnée", show the price and a **10 cm red band** |

Loi 04-02 arts. 22/22 bis/36 (in 04) punish breaches with fines of 20,000 to 10,000,000 DA.

**POS implications:**
- a "prix réglementé" product flag with ceiling and margin data, shipped as an **updatable data pack** (like the tax "LF pack" in 04 §3.1);
- a block or warning when the sale price exceeds the ceiling;
- the IFU margin-based base for these products (04 §5.1 CIDTA 282 quater);
- default products in the superette template.

**Not found / [U]:** the current bread price text, and any post-2022 revision of these ceilings (the portal lists nothing newer). Re-check yearly.

### 5.4 Jewellers and precious metals: CII art. 359 as amended by LF 2026 art. 62 [V]

Source: DGI **circulaire n° 34/MF/DGI/LF.2026** (effective 1 Jan 2026): https://www.mfdgi.gov.dz/files/804/2026/3779/Circulaire-n34-du-05-05-2026-relative-a-la-Garantie---Encadrement-des-activites-portant-sur-le-commerce-des-metaux-precieux

- **Artisan jewellers** are now expressly covered, alongside manufacturers, exporters and gold/silver/platinum merchants.
- The activity needs an **authorisation from the tax administration** (arrêté pending; instruction n° 06/MF/DGI of 3 June 2025 applies meanwhile). It is withdrawn if conditions are breached.
- They must keep a **special client register, coté et paraphé by the tax administration**, recording for each sale of gold/silver/platinum works: **nature, number, weight, fineness (titre), price, and the identity and address** of the buyer.
- This is distinct from the "registre de police" (CII 359) and from the AML **special client register** required by the arrêté of the Minister of Finance of **3 July 2025** (art. 16).
- Documents must be kept **5 years** and shown to AML/CFT inspectors.

Related 2026 circulars (titles verified in the DGI list, not read): n° 27 (tax regime for precious stones), n° 28 (garantie: AML/CFT prevention), n° 32 (TIC on precious stones), n° 33 (regularisation of gold/silver/platinum works).

**POS:** the jewellery pack must capture buyer ID on every precious-metal sale and print the register (paged, numbered, never editable), keep 5+ years, and store the authorisation number in settings.

### 5.5 Why DZBuild should not ship "fictive invoices" or "non-billable stock" [V]

Several competitors sell these as features: Klick Stock Express (§3.1), Smart Gestion "factures virtuelles", WINSTO, ZSOFT "factures fictives", PRO SOLUTION "factures non comptabilisées" (01 §4). The texts:

- **CTCA art. 118** (2026 ed.) and **CIDTA art. 193**: "manœuvres frauduleuses" include "**les ventes sans facture**" and "écritures inexactes ou **fictives** au livre-journal et au **livre d'inventaire** prévus par les articles 9 et 10 du code de commerce". The fiscal fine is **100 %** of evaded duties (CTCA 116-II).
- **CIDTA art. 303** as amended by **LF 2026 art. 25**: penal fines and prison scale with the evaded amount. From 50,000–100,000 DA up to 2–5 years and 2–5 M DA. The penalty rises to **5–10 years and 5–10 M DA when the fraud is committed "au moyen des technologies de l'information et de la communication"**, organised, or with several accomplices. **§2: accomplices incur the same penalties as the authors.**
- From 2027: **CTCA 51 bis / CPF 20 ter / CPF 51 sexies** (04 §6.1). The publisher attests inalterability, and must hand over code and documentation on request.

**Recommendation (design):** no dual/"non-billable" stock and no invoice deletion. Offer legitimate alternatives instead:
- non-invoiced consumer sales still recorded with a ticket (loi 04-02 art. 10);
- stock-state distinctions that are legal: available, reserved, damaged, **own use**, samples, **consignment/dépôt-vente**, in transit;
- IFU TTC mode.

This is a **selling point** for the 2027 certification ("conforme, inaltérable").

### 5.6 Zakat: official nisab [V]

The Ministry of Religious Affairs and Wakfs homepage (https://marw.dz/, fetched 2026-09-28) shows in its "key figures" box: **"نصاب زكاة النقود وعروض التجارة لعام 1447هـ (دج): 2295000"**. That is, **2,295,000 DA** for cash and trade goods, year 1447 AH.

**POS:**
- Zakat report = (stock at sale value per the chosen method + cash + collectible receivables − debts due) compared with the nisab.
- Standard rate 2.5 % after a lunar year. The rate is standard fiqh, **[U]** here.
- Ship the nisab as updatable data with year and source, and let the user edit it, since the ministry publishes a new figure each Hijri year.
- Don't present it as a fatwa.

### 5.7 Livre d'inventaire / annual inventory [V reference]

CIDTA 193 and CTCA 118 (2026 editions) refer to the "livre-journal et livre d'inventaire prévus par les **articles 9 et 10 du code de commerce**". The Code de commerce text (ordonnance 75-59) on the Commerce portal is a scan and was not read. The article content (a yearly inventory of assets and liabilities) is standard but **[U]** here.

**POS:**
- a year-end "**Inventaire de fin d'exercice**" workflow: frozen count date, counted vs book quantity, valuation at CMUP, variance posting;
- a **paged, numbered print** suitable for the livre d'inventaire;
- archived with the fiscal journal.

03 has counts (INV-007..009) but not the legal year-end artefact.

### 5.8 Card payments: what GIE Monétique certifies [V]

- The **Politique d'homologation** (https://giemonetique.dz/activites/homologation/politique-homologation/) certifies "**Equipements interbancaires ainsi que l'ensemble des logiciels nécessaires à leur interaction avec ce système**" and "Solutions, dans leurs dimensions logicielles et matérielles". Web-merchants need an **authorisation** via their bank.
  - **Implication:** a POS that drives a CIB terminal (ECR link) would likely need GIE certification. Keep v1 "manual card tender" (04/05) and treat integration as a partner project with a bank.
- **Certified TPEs (21 entries,** https://giemonetique.dz/activites/homologation/referentiel-des-tpe-certifies/**):**
  - SZZT/ENIE: KS8226 (Android 10, cert. valid to 10/07/2027), KS8223, ZT8225, I90
  - **PAX / Trois S Pay:** A8900 (PayDroid, Android 12), IM30, A920 Pro, A920
  - MoreFun: MF919, M90
  - Topwise: T3, T1
  - Enatel/Tianyu: TP50, P30
  - Verifone: X990, V240M variants
  - Ingenico Move 2500 (Telium 3)
  - Nexgo G2

  The embedded payment app is "**CIBPAY** (contact/contactless) 2.20.06 (français/arabe)". Most are **Android smart POS**. App-to-app payment requests from a POS app on the same device are technically common on these models, but **whether Algerian banks or GIE allow third-party apps is [U]**.
- **Approved mobile payment solutions** (card-based): **SPARK MPAY** (EURL MS Solutions Algeria), **PS-mPAY** (ProgressSoft), **SSI MPAY** (Smart Solution and Innovation) (https://giemonetique.dz/activites/homologation/approbations-des-solutions-de-paiements-mobile/). Whether any is a tap-to-phone SoftPOS a merchant can use with a POS is **[U]**.

### 5.9 Slick-Pay API [V]

The official GitHub org **Slick-Pay-Algeria** publishes SDKs:
- slickpay-npm (updated 2026-09-21), slickpay-laravel, a Python package, Flutter and Ionic samples, a WHMCS module, a WordPress plugin.

The Node SDK exposes:
- user side: **Account, Contact, Transfer (+commission), Aggregation, Invoice**;
- merchant side: **MerchantInvoice**;
- public-key auth and a sandbox flag; docs at devapi.slick-pay.com.

Source: https://github.com/Slick-Pay-Algeria/slickpay-npm. DZBuild itself takes CIB/Edahabia through Slick-Pay (06 §1.3).

**POS:** optional "lien de paiement" tender next to Chargily (04 §11.2), created only when online.

### 5.10 69 wilayas: resolved [V]

- **Loi n° 26-06 du 4 avril 2026**, JO n° **25** of 5 April 2026 (https://www.joradp.dz/FTP/jo-francais/2026/F2026025.pdf): "69 wilayas et 1541 communes" (new art. 3 of loi 84-09).
- The new art. 52 bis 13 lists **Bir El Ater, Negrine, El Ogla El Melha, Ferkane**. Tébessa's article now has **24** communes, so the four came from Tébessa (it had 28 before, per the 58-wilaya datasets cited in 04).
- **Décret présidentiel n° 26-206 du 25 mai 2026**, JO n° **40**/2026 (https://www.joradp.dz/FTP/jo-francais/2026/F2026040.pdf) fixes names and seats 59–69: Aflou, Barika, El Kantara, Bir El Ater, El Aricha, Ksar Chellala, **Aïn Ouessara**, Messaad, Ksar El Boukhari, Bou Saâda, El Abiodh Sidi Cheikh.

### 5.11 IRG barème (for a "salaires & avances" lite module) [V]

CIDTA 2026 art. 104 (last amended by LF 2026 art. 11), annual taxable income:

| Band | Rate |
|---|---|
| ≤ 240,000 DA | 0 % |
| 240,001–480,000 DA | 23 % |
| 480,001–960,000 DA | 27 % |
| 960,001–1,920,000 DA | 30 % |
| 1,920,001–3,840,000 DA | 33 % |
| > 3,840,000 DA | 35 % |

**Still [U]:** the salary-specific abatements and exemptions, and the CNAS contribution rates. Recommendation: a V2 "salaires" lite module (employees, advances from the till, monthly pay record, IRG estimate, G50/G50 ter helper); defer full payroll.

### 5.12 Updated list of unverified regulatory items

| Item | Status after this pass |
|---|---|
| Arrêté on the 2027 publisher engagement / certificate (CPF 20 ter) | Still not published: the DGI 2026 circulars list (n° 01–49, to 13 June 2026) has none on cash software [V absence] |
| Implementing decree for ticket-de-caisse mentions (loi 04-02 art. 12) | [U] |
| Joint arrêté on telematic invoices (05-468 art. 11) | [U] |
| Arrêtés fixing warranty duration per product type (13-327 art. 16–17) and the certificate model (art. 7) | [U] (new) |
| Wali arrêtés fixing each year's soldes dates (06-215 art. 4) | Per wilaya; must be user-entered [U] |
| Arabic language on invoices (loi 91-05, ordonnance 96-30) | The Commerce portal lists both under "Généralisation de l'utilisation de la langue arabe", but the 1991 JO is a scan; content [U]. Keep AR+FR documents. |
| Barcode obligation "from 29 March 2023" (05 §3.2) | The FAQ page cited by 05 only confirms GS1 Algeria prefix **613** [V]; the obligation date remains [U] |
| Bread price, post-2022 staple price revisions | [U] |
| CNAS rates; IRG salary abatements | [U] |
| Interbank QR standard; ECR protocol | GIE certification scope verified (§5.8); protocol [U] |
| Weekend decree, 3-day Eid, Ramadan hours | [U] (unchanged from 04) |

---

## 6. Features missing from the catalogues (add these rows to the backlog)

Suggested release tags: **M** = MVP, **V1**, **V2**, as in 03.

### 6.1 Till / checkout

| New ID | Feature | Why (evidence) | Rel. |
|---|---|---|---|
| DZ-CHK-01 | **Department keys / "calculatrice" mode**: open-price sale by department ("Rayon 1 – Légumes", "Pain", "Flexy"), numeric keypad first, no catalogue needed | Small épiceries and kiosks sell unbarcoded items; standalone ECRs (Casio/Sharp, §3.3) work this way. 03 has only a single "custom item" (CHK-018) | M |
| DZ-CHK-02 | **Price-ceiling guard** for regulated products (block or warn above ceiling; show "subventionné") | §5.3 | M (superette) |
| DZ-CHK-03 | **Channel field** per sale (comptoir, téléphone, WhatsApp, Facebook, DZBuild, **Yassir**, other) with per-channel commission | §6.6 | V1 |
| DZ-CHK-04 | **Warranty certificate** printed from the sale, per serial/IMEI line (13-327 art. 6 mentions); buyer name required for warranted items | §5.1 | M (phones/appliances) |
| DZ-CHK-05 | **Used-goods flag** (occasion) with 3-month minimum warranty | 13-327 art. 17 | V1 |
| DZ-CHK-06 | **Container deposits (consignes)**: deposit line, empty-for-full exchange, container balance per customer | 05-468 art. 9; gas/beverage shops (§4.5) | V1 |
| DZ-CHK-07 | **Gold sale line**: weight × gold price per gram (by titre) + façon; old-gold trade-in line with weight, titre, loss %; buyer ID | §4.3, §5.4 | V1 (jewellery pack) |
| DZ-CHK-08 | **Membership check-in** at the till (scan card/RFID → validity, remaining visits) | §4.4 | V2 |
| DZ-CHK-09 | Audible **scan beep** (ok/error) and big "last scanned item" banner for busy tills | UX for older staff (§9) [U] | M |

### 6.2 Pricing, promotions, labels

| New ID | Feature | Why | Rel. |
|---|---|---|---|
| DZ-PRM-01 | Promotions store **"prix pratiqué auparavant"** and campaign type (solde / promotion / liquidation / déballage), with legal-window checks per wilaya | 06-215 arts. 2–6 | M |
| DZ-PRM-02 | **Declaration export** (XLSX/PDF): list, quantities, old price, new price, % per campaign | 06-215 arts. 6, 8, 12 | M |
| DZ-PRM-03 | "Was / now" shelf labels | 06-215 art. 5 | V1 |
| DZ-PRM-04 | **Label reprint queue after price changes** (any price edit, bulk update or promotion start/end queues the affected shelf labels) | Prices must be displayed and match (loi 04-02 art. 4-6, in 04); 03 has bulk price update (PRD-063) but no reprint queue | M |
| DZ-PRM-05 | **Gold-price board**: daily price per gram per titre, auto-reprice all jewellery pieces | Batna listing (§4.3) | V1 |
| DZ-PRM-06 | Stock-age guard for soldes (goods held ≥ 3 months) | 06-215 art. 2 | V1 |

### 6.3 Stock and documents

| New ID | Feature | Why | Rel. |
|---|---|---|---|
| DZ-STK-01 | **Year-end inventory (livre d'inventaire)**: frozen date, count, CMUP valuation, paged numbered print, archived | §5.7 | V1 |
| DZ-STK-02 | **Melting batches (fonte)** for jewellers: input pieces → metal weight out, loss | §4.3 | V2 |
| DZ-STK-03 | **Jeweller client register** (CII 359): nature, number, weight, titre, price, buyer identity and address, uneditable print, 5-year retention | §5.4 | V1 |
| DZ-STK-04 | **Service tickets** (pressing / repair / tailoring / optician lab): intake, tag numbers, promised date, statuses, deposit, "ready" SMS/WhatsApp, uncollected report | §4.1 | V1 |
| DZ-STK-05 | **Container ledger** (consignes by container type and customer) | §4.5 | V1 |
| DZ-STK-06 | **Van stock** (van as depot) with end-of-route reconciliation | 01 Orbis/PMO; Supersoft (§3.2) | V2 |
| DZ-STK-07 | **Subscriptions / memberships** (plans, N-visit cards, expiry, freeze, renewals) | §4.4 | V2 |
| DZ-DOC-01 | **Dot-matrix output profile**: text-mode, fixed-pitch, continuous forms, **N copies with "ORIGINAL / DUPLICATA / TRIPLICATA"** captions | §7.1 | V1 |
| DZ-DOC-02 | **Dunning letters** (lettre de relance → mise en demeure) with ageing buckets, printable and WhatsApp-able | Sétif product (§3.2); 03 CRD-006 covers only reminders | V1 |

### 6.4 Finance / treasury

| New ID | Feature | Why | Rel. |
|---|---|---|---|
| DZ-FIN-01 | **Cheque & traite portfolio**: received/issued, due date, bank, status (en portefeuille, remis, encaissé, **impayé**), deposit slips (bordereau de remise), due-date alerts | Sétif product (§3.2); B2B practice | V1 |
| DZ-FIN-02 | **Bank-statement import** for reconciliation (CSV/XLSX first; AFB/CFONB fixed-width if Algerian banks export it) | Vendor claim [S]; which formats Algerian banks export is **[U]** | V2 |
| DZ-FIN-03 | **SCF account mapping** for the accountant export. Proposed defaults, **[U] to verify against the SCF nomenclature**: 700 ventes de marchandises, 600 achats de marchandises vendues, 30 stocks de marchandises, 411 clients, 401 fournisseurs, 4457 TVA collectée, 4456 TVA déductible, 53 caisse, 512 banque, 709/609 rabais | 03 IMP-018 has no mapping; Sétif product advertises "plan comptable & export" | V1 |
| DZ-FIN-04 | **Salaries & advances lite**: employees, advances from the till, monthly pay record, IRG estimate (§5.11) | Competitors bundle payroll (§2.2) | V2 |
| DZ-FIN-05 | **Zakat report** with official, editable nisab data (§5.6) | 02; ministry figure | V1 |

### 6.5 Customer / compliance

| New ID | Feature | Why | Rel. |
|---|---|---|---|
| DZ-CMP-01 | "**Conforme, inaltérable**" mode is the only mode: no hidden/dual stock, no invoice deletion; corrections by avoir | §5.5 | M |
| DZ-CMP-02 | **Regulatory data packs** (updatable without reinstall): tax rates (04), timbre brackets (04), **price ceilings (§5.3)**, **nisab (§5.6)**, wilaya/commune lists (§5.10), holidays (04) | Several yearly-changing values | M |
| DZ-CMP-03 | **Prize-draw guard**: no draw tied to a purchase | 06-215 art. 9 | M (design rule) |
| DZ-CMP-04 | Jewellery activity **authorisation number** and AML fields in settings | §5.4 | V1 |

### 6.6 Integrations

| New ID | Feature | Why | Rel. |
|---|---|---|---|
| DZ-INT-01 | **Aggregator orders**: manual "Yassir" entry screen (menu mapping, commission %, courier status) | Yassir partners use a separate "Yassir Store" app; the partner page advertises order management and promotions but no POS API [V page]; API existence **[U]** | V1 (resto) |
| DZ-INT-02 | **Offline SMS/USSD gateway** through a USB GSM modem (AT commands; flexy USSD sessions; debt-reminder SMS) | Modem sold "for SMS & USSD" (§7.3); KERS FLEXY uses USSD (02) | V1 |
| DZ-INT-03 | **CCTV text overlay** (send each receipt line over TCP to a Hikvision-compatible NVR "POS" input) | §7.4 | V2 |
| DZ-INT-04 | **Slick-Pay / Chargily payment links** as optional online tenders | §5.9, 04 | V2 |
| DZ-INT-05 | **ZKTeco access control** for memberships (reuse 05 §10 libraries) | §4.4 | V2 |
| DZ-INT-06 | **Remote support hand-off**: a "Get help" button that starts a remote-support session. Algerian vendors all use AnyDesk/TeamViewer (01 §6). If RustDesk is used, note it is AGPL-3.0: ship it as a separate executable, not linked into the POS **[U licence reading]** | Support at scale (§11) | V1 |

---

## 7. Hardware not covered by 05

### 7.1 Dot-matrix (impact) printers for multi-copy documents [V listings]

"Imprimante matricielle" returns **48 listings** on Ouedkniss:
- **Epson LX-350**: 46,300 DA (sarl vitsys), 46,800 (Media plaza, Oran), 47,500 (CMB), 49,500 (Blida Computer), 53,000 DA (Oceanic, Bab Ezzouar).
- **Epson LX-300 / LX-300+**: 31,500–34,000 DA. Clyo Systems says it "imprime jusqu'à 5 exemplaires (1 original + 4 copies)".
- **LQ-680**: 67,000–69,500 DA. **LQ-2090**: 142,000–180,000 DA.
- Vendors pitch them as "idéal multi-copies & factures". Ribbons sell at ~500 DA.

**Implications:**
- Support Windows-driver text printing to 9-pin/24-pin printers with **continuous-form** page lengths (A4, A5, 12").
- Fixed-pitch 80/136-column layouts.
- Copy captions; no graphics dependency.
- For Arabic, use the driver's graphics mode (slow) or recommend a laser printer for AR documents. Arabic in impact text mode is **[U]**.

### 7.2 Standalone ECRs (competition, not integration)

Casio SE-G1 (26,000 DA), Sharp XE-A107 (25,000 DA) (§3.3). No integration needed; relevant to pricing and positioning.

### 7.3 USB GSM modems (SMS/USSD) [V listing]

Huawei **E3531** 3G HiLink at 3,800 DA, advertised "envoyer et recevoir commande sms ussd" (IBsoft-dz, Bab Ezzouar, d54946393); Huawei **K3772** at 3,800 DA (d54947458, d54947683).

Design:
- AT-command driver (`AT+CMGF`, `AT+CMGS`, `AT+CUSD`) behind a "messaging" interface.
- HiLink models may need the switch to serial/modem mode **[U per model]**.

### 7.4 CCTV recorders with POS overlay [V feature]

Hikvision NVRs/DVRs have a "POS" function: the recorder listens on a TCP port, overlays the received text on a chosen camera (scroll or page mode), and makes it searchable in **iVMS-4200**. Not all recorders have it (tested on "I" series NVRs and e.g. DS-7316HUHI-K4). Source: https://github.com/sutty74/Homeassistant-Hikvision-POS (README).

Hikvision's share of the Algerian CCTV market is **[U]**. POS vendors do bundle cameras (General IT, "Logiciel Superette + camera", 01).

### 7.5 Membership access hardware

ZKTeco fingerprint/face/RFID terminals and **tripod turnstiles** are sold with gym software (d45334057, d21116539, d44267665 "Pointeuse Biométrique Algérie") [S]. Reuse 05 §10 (TCP 4370).

### 7.6 Other peripherals seen

- **Banknote counter with customer display** (21,000 DA, d38313595) [S]. 05 covered counters without displays.
- **Counting scale** CAS PR-C (d26262623) and price-computing scales (Preciamolen Mistral 520 at 85,000 DA, d49341647; PRO PESAGE 30 kg poids-prix, d22748294) [S]. These are the serial checkout scales 05 §4.3 designed for.
- **SMARTPOS A3** all-in-one at **61,500 DA** (J1900, 4 GB, 64 GB SSD, customer display; Chrono Informatique, d57947376) [S]. This is a new low price point for the reference hardware; 05 had 85,000+.
- **TPE UPT1000** sold on Ouedkniss (d54466271) [S]. It is **not** in the GIE certified list (§5.8), so resale TPEs may not be usable.
- **Self-order kiosks / pagers:** no Algerian listing found ("borne de commande" → 0) [V absence]. Keep V2.

---

## 8. Import / export formats missing

| Format / source | Evidence | Recommendation | Rel. |
|---|---|---|---|
| **HFSQL (WinDev) `.fic/.ndx/.mmo`**: most local desktop competitors are WinDev apps (01 §7: cracked WinDev sold openly; "erreur dans propriété ligne 381" typical WinDev message) | PC SOFT provides an **ODBC driver for HFSQL** (installable alone from the "modules communs" download, per node-hfsql README, https://github.com/gilhardl/node-hfsql). **FIC Engine & Inspector** (Rust + Node, MIT or Apache-2.0, v0.1.0) reads HFSQL Classic files and exports JSON/CSV (https://github.com/Samvel070903/HFSQL-Classic-Fic-inspector). **hfsql-pg-migrator** (Python) migrates HFSQL C/S to PostgreSQL (https://github.com/ImBasilio60/hfsql-pg-migrator) | "**Import from my old software**" wizard: (a) ODBC route when the user has the HFSQL driver; (b) direct `.fic` reading via the MIT/Apache reader (evaluate maturity: 3 stars, early). Map products/customers/balances with the column-mapping UI | V1 |
| **DBF (dBase/FoxPro)** | GECO SOFT exports "Excel, DBF, txt, rtf" (§3.2) [S] | Read DBF in the importer (small, well-known format) | V1 |
| **MS Access (.mdb/.accdb)** | Not evidenced in listings | Only if user research shows demand | opt |
| **Bank statements** (CSV/XLSX; AFB/CFONB fixed-width) | Sétif vendor claim [S] | CSV/XLSX mapping first | V2 |
| **Regulatory exports** | New in this file | Soldes/promo declaration (§5.2), jeweller register (§5.4), year-end inventory (§5.7), warranty certificates (§5.1), Zakat statement (§5.6) | M–V1 |
| **Accountant export with SCF accounts** | §6.4 | CSV/XLSX journal with SCF account codes | V1 |
| **Data-collector TXT** | 05 covered | — | — |
| **Peppol UBL / e-MECeF** | Klick supports them for other countries | Not required in Algeria (04 §6.2) → n/a | n/a |
| **Google Sheets** | DZBuild's own export path (06 §1.4) | Optional online export when linked | V2 |

---

## 9. UX needs not specified (Arabic/French, older users, keyboard-heavy)

**Data points**
- Adult literacy **75.1 %** (2008, latest World Bank value, SE.ADT.LITR.ZS) [V].
- Population 65+ = **6.8 %** (2025) [V].
- Internet users **77.4 %** (2024) [V].

Source: https://api.worldbank.org/v2/country/DZA/indicator/...

Many owners and cashiers read French poorly or Arabic poorly, and some read neither well. The UI must work with icons, pictures, colour and numbers.

| ID | Rule | Rationale |
|---|---|---|
| UX-DZ-01 | **"Mode simple"** toggle: big tiles with product photos, 3 buttons (Vendre / Crédit / Fermer caisse), 18–20 px base font, 56 px touch targets; the full back office stays hidden until unlocked | Low literacy, older owners [U design] |
| UX-DZ-02 | **AZERTY-aware numeric entry**: in quantity/price fields, map unshifted `& é " ' ( - è _ ç à` to `1–0` (the same physical-key decoding 05 §3.2 uses for scanners), and accept Arabic-Indic digits `٠–٩` | On AZERTY laptops without a numpad, digits need Shift [V: 05 layout facts] |
| UX-DZ-03 | **Arabic search normalisation**: fold أ/إ/آ → ا, ة → ه, ى → ي, strip tashkeel and tatweel; match Arabizi (e.g. "7lib" ↔ حليب) through a user-editable alias field | 03 CHK-011 only says "diacritic-insensitive" [U design] |
| UX-DZ-04 | **Bidi isolation** (Unicode FSI/PDI) around amounts, barcodes, phone numbers and Latin product names inside Arabic lines, on screen, receipts and PDFs | Mixed AR/FR catalogues; 04/05 cover digit grouping only [U design] |
| UX-DZ-05 | **Keymap presets** that mimic popular local tills (e.g. DS RAPIDE: F1 hold, F4 search, F9 pay, F12 cancel…), selectable at setup | Competitors win switchers by remapping keys (01 §4.2); 07 proposes a different default (F2 search, F9 pay) |
| UX-DZ-06 | **Voice/sound cues**: scan ok/error, drawer open, credit limit reached | Busy counters [U] |
| UX-DZ-07 | Printed **one-page quick card** (AR/FR) generated from the current keymap and settings | Offline learning [U] |
| UX-DZ-08 | Show **prices in centimes on request** ("prix en centimes", e.g. 250 DA → "25 000") on the customer display and labels | 04 §8.1 notes older customers count in centimes |
| UX-DZ-09 | **Hijri date** optional on tickets/reports. [L] `Intl.DateTimeFormat('ar-DZ-u-ca-islamic-umalqura')` renders 2026-09-28 as "17 ربيع الآخر 1448 هـ". Allow a ±1-day correction, since Algerian dates follow official moon sighting | 04 §10 |
| UX-DZ-10 | **Tamazight (Latin) UI** as a community-translated option | Official language; Kabylie market [U demand] |
| UX-DZ-11 | Robust install: a **portable (USB) mode** and an **offline full installer**; demo data | Buyer complaint "la version demo ne s'installe pas" (01 §8) |

---

## 10. Platform and tech gaps

### 10.1 Windows 7

Out, per 07 (Tauri 2.12 + Rust ≥ 1.78). 05 §7.2 and §13 must be updated (§2.2). Tell resellers "Windows 10 1809+ or Windows 11".

### 10.2 Time zone and calendar [L]

`Intl.DateTimeFormat('fr-DZ',{timeZone:'Africa/Algiers',timeZoneName:'longOffset'})` gives **UTC+01:00** on both 2026-01-15 and 2026-07-15. There is no DST, which closes 07's open point. The business-day cut-off for after-midnight Z-closes (04 §10) is still needed.

### 10.3 Offline OCR for supplier invoices [L]

Via Windows PowerShell 5.1 WinRT, `Windows.Media.Ocr.OcrEngine.AvailableRecognizerLanguages` on this PC = **en-US, fr-FR**; `IsLanguageSupported('ar-SA')` = **False**. Built-in OCR is fine for French invoices. Arabic needs an OCR language pack (availability **[U]**) or **Tesseract** (ara+fra). AI OCR (competitors' "Scaneye", "photo → facture") should run in the DZBuild cloud when online.

### 10.4 Code signing and distribution options (additions to 07 §13.2)

- **Microsoft Store:** Algeria **is listed** in the Microsoft Store consumer markets for MSI/EXE apps [V] (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/market-selection). Developer-account registration from Algeria is **[U]** (the account page requires sign-in).
- **If DZBuild POS were open source:** the **SignPath Foundation** signs OSI-licensed projects for free, but the certificate is issued to SignPath Foundation, which appears as publisher; proprietary dual-licensing is not allowed [V] (https://signpath.org/terms). **Certum "Open Source Code Signing"** for individual developers costs from €69 on a crypto card, subject name prefixed "Open Source Developer" [V] (https://shop.certum.eu/open-source-code-signing.html, currently out of stock).
- Neither suits a proprietary DZBuild-branded product. They matter only if P5 (licence) goes open source. Otherwise 07's OV-certificate + Store plan stands.

### 10.5 Support and remote assistance

See DZ-INT-06 and §11.

---

## 11. What the next document (requirements and plan) must add

1. **Unified backlog:** 03's 547 rows + 01 §9 + 02 §7 + 04 §15 + 05 §13 + this file's DZ-* rows, de-duplicated, each with legal/market/UX origin.
2. **Competitor matrix** (P3), with Klick Stock Express, Crescendo Free and Almawarid as the three "free" baselines.
3. **Vertical packs list**, with a default template each:
   - superette (incl. regulated prices)
   - butcher/greengrocer (scale)
   - bakery/pâtisserie
   - restaurant/café
   - phones (IMEI + warranty + repairs + flexy)
   - clothing (variants + soldes compliance)
   - quincaillerie/matériaux
   - auto parts
   - parapharmacy
   - wholesale/distribution
   - **jewellery**
   - **pressing & services**
   - **gym/memberships**
   - **kiosque/multiservices**
   - instalment sales
   - optician
4. **Support & channel plan:**
   - in-app help and videos in Darija/AR/FR;
   - a remote-support hand-off;
   - a **hardware-reseller programme**: hardware shops are the main sales channel (01 §6), and a free POS gives them a reason to sell PCs, printers and scales with it;
   - a community forum;
   - WhatsApp support hours.
5. **Legal review before 1 Jan 2027:** the publisher engagement text, the code-disclosure process (CPF 51 sexies), and the "no fictive features" policy (§5.5).
6. **Field validation** (P4) before freezing MVP scope.
7. **Data packs pipeline** (DZ-CMP-02): who updates rates, ceilings, nisab and wilayas, how often, and how they are signed.

---

## 12. Remaining unknowns

- Loyverse, Daftra and DEXEF usage in Algeria (no Algeria-specific pages found).
- Whether Algerian banks allow third-party POS apps on GIE-certified Android TPEs, or any ECR link.
- Yassir partner API.
- AFB/CFONB exports from Algerian e-banking.
- Warranty-duration arrêtés by product (13-327).
- Whether 06-215 was amended.
- The Code de commerce art. 10 text (scan).
- Loi 91-05 invoice-language scope (scan).
- CNAS rates.
- Bread price.
- Microsoft Store developer registration from Algeria.
- Hikvision market share.
- Arabic on dot-matrix printers.

---

## 13. Sources (all accessed 2026-09-28)

**Official texts and portals**
- Décret exécutif 13-327 (garantie), JO n° 49/2013 extract: https://dcwalger.dz/fr/images/stories/PDF/garantie/dec13327fr.pdf
- Décret exécutif 06-215 (soldes, promotions…), JO n° 41/2006: https://www.dcwalger.dz/fr/images/stories/solde/Decret%20executif%20n%2006_215%20du%2018%20juin%202006.pdf ; https://commerce.gov.dz/fr/reglementation/decret-executif-n-deg-06-215 ; https://www.joradp.dz/FTP/jo-francais/2006/F2006041.PDF
- Commerce portal, regulated prices recueil: https://www.commerce.gov.dz/fr/reglementation/recueil/prix-reglementes ; décret 21-383 page: https://www.commerce.gov.dz/fr/reglementation/decret-executif-n-deg-21-383-du-5-octobre-2021-modifiant-et-completant-le-decret-executif-n-deg-11-108-du-6-mars-2011 ; décret 22-186 page: https://www.commerce.gov.dz/fr/reglementation/decret-executif-n-deg-22-186-du-15-mai-2022-jo-n-deg-33-du-17-mai-2022 ; décret 20-242 page: https://www.commerce.gov.dz/fr/reglementation/decret-executif-n-deg-20-242-du-31-aout-2020-jo-n-deg-52-du-2-septembre-2020 ; loi 91-05 page: https://www.commerce.gov.dz/fr/reglementation/loi-n-deg-91-05
- JO 2011 n° 15 (décret 11-108): https://www.joradp.dz/FTP/jo-francais/2011/F2011015.pdf
- JO 2021 n° 77 (décret 21-383): https://www.joradp.dz/FTP/jo-francais/2021/F2021077.pdf
- JO 2022 n° 33 (décret 22-186): https://www.joradp.dz/FTP/jo-francais/2022/F2022033.pdf
- JO 2020 n° 52 (décret 20-242): https://www.joradp.dz/FTP/jo-francais/2020/F2020052.pdf
- JO 2026 n° 25 (loi 26-06): https://www.joradp.dz/FTP/jo-francais/2026/F2026025.pdf
- JO 2026 n° 40 (décret présidentiel 26-206): https://www.joradp.dz/FTP/jo-francais/2026/F2026040.pdf
- DGI circulaire n° 34/2026 (métaux précieux): https://www.mfdgi.gov.dz/files/804/2026/3779/Circulaire-n34-du-05-05-2026-relative-a-la-Garantie---Encadrement-des-activites-portant-sur-le-commerce-des-metaux-precieux
- DGI 2026 codes (CIDTA arts. 104, 193, 303; CTCA arts. 116–118), as downloaded by track 04: https://www.mfdgi.gov.dz/files/803/2026/3726/CodedesImpotsDirectsetTaxesAssimilees2026fr.pdf ; https://www.mfdgi.gov.dz/files/803/2026/3725/CodedesTaxessurle-Chiffred-Affaires2026fr.pdf
- DGI 2026 circulars index (n° 01–49), as downloaded by track 04 from the DGI files API (category 804)
- Ministry of Religious Affairs (nisab 1447 AH): https://marw.dz/
- Ministry of Commerce FAQ (GS1 Algeria prefix 613): https://commerce.gov.dz/fr/questions-frequentes?page=47
- GIE Monétique homologation policy: https://giemonetique.dz/activites/homologation/politique-homologation/ ; certified TPEs: https://giemonetique.dz/activites/homologation/referentiel-des-tpe-certifies/ ; approved mobile payment solutions: https://giemonetique.dz/activites/homologation/approbations-des-solutions-de-paiements-mobile/ ; payment solutions: https://giemonetique.dz/activites/homologation/solutions-de-paiements/
- World Bank API (DZA): SE.ADT.LITR.ZS, SP.POP.65UP.TO.ZS, IT.NET.USER.ZS: https://api.worldbank.org/v2/country/DZA/indicator/SE.ADT.LITR.ZS?format=json

**Vendors and platforms**
- Klick Stock Express: https://stock.klick.dz/ ; pricing: https://stock.klick.dz/fr/prix ; https://www.klick.app/
- Almawarid pricing (plans "sur demande"): https://almawarid.app/tarifs/
- Odoo partners in Algeria: https://www.odoo.com/fr_FR/partners/country/algerie-61
- Yassir restaurant partners: https://yassir.com/fr/restaurant
- Slick-Pay: https://slick-pay.com/ ; SDK: https://github.com/Slick-Pay-Algeria/slickpay-npm
- Microsoft Store markets: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/market-selection
- SignPath Foundation terms: https://signpath.org/terms
- Certum Open Source Code Signing: https://shop.certum.eu/open-source-code-signing.html
- Hikvision POS overlay (community integration README): https://github.com/sutty74/Homeassistant-Hikvision-POS
- HFSQL tools: https://github.com/gilhardl/node-hfsql ; https://github.com/Samvel070903/HFSQL-Classic-Fic-inspector ; https://github.com/ImBasilio60/hfsql-pg-migrator

**Ouedkniss listings** (public GraphQL API, 2026-09-28)
- d46334731 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-bijouterie-birtouta-kouba-mohammadia-alger-algerie-d46334731
- d47974462 https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-de-bijouterie-blida-algerie-d47974462
- d44602069 https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-محلات-بيع-الذهب-logiciel-de-gestion-des-bijouteries-batna-algerie-d44602069
- d45454081 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-professionel-bijouterie-oran-algerie-d45454081
- d38319701 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-pressing-birtouta-kouba-mohammadia-alger-algerie-d38319701
- d18280273 https://www.ouedkniss.com/alger-kouba-algerie-applications-logiciels-logiciel-pressing-d18280273
- d56041582 https://www.ouedkniss.com/logiciels-abonnements-logiciel-pressing-bab-ezzouar-alger-algerie-d56041582
- d30243863 https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-pressing-degraissage-laverie-constantine-algerie-d30243863
- d25229062 https://www.ouedkniss.com/oran-algerie-applications-logiciels-logiciel-pressing-d25229062
- d45414436 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-professionel-boucherie-oran-algerie-d45414436
- d53418317 https://www.ouedkniss.com/logiciels-abonnements-pack-boucherie-birtouta-alger-algerie-d53418317
- d57850917 https://www.ouedkniss.com/pc-de-bureau-logiciel-boucherie-tlemcen-algerie-d57850917
- d28923331 https://www.ouedkniss.com/alger-bordj-el-kiffan-algerie-applications-logiciels-logiciel-pour-boucherie-en-promo-d28923331
- d57648717 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-salle-sport-hadjout-tipaza-algerie-d57648717
- d55843231 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-salle-sport-bir-chouhada-oum-el-bouaghi-algerie-d55843231
- d40978507 https://www.ouedkniss.com/logiciels-abonnements-smart-gym-logiciel-de-gestion-salle-sport-bab-ezzouar-alger-algerie-d40978507
- d56592551 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-salle-sport-titaou-gym-ultimate-برنامج-تسيير-قاعات-الرياضة-احترافي-khenchela-algerie-d56592551
- d56746856 https://www.ouedkniss.com/logiciels-abonnements-logiciel-salle-de-sport-oran-algerie-d56746856
- d44267665 https://www.ouedkniss.com/logiciels-abonnements-logiciel-salle-de-sport-blida-cheraga-hydra-oran-boumerdes-alger-algerie-d44267665
- d21116539 https://www.ouedkniss.com/tlemcen-sidi-bel-abbes-oran-naama-ain-temouchent-algerie-applications-logiciels-gestion-salle-de-sport-avec-poiteuse-d21116539
- d21165618 (LOG103, Sétif; 8,248 views): Ouedkniss ID via API
- d14437207 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-salle-sport-oran-algerie-d14437207
- d45334057 https://www.ouedkniss.com/securite-alarme-logiciel-salle-de-sport-avec-tourniquet-birkhadem-alger-algerie-d45334057
- d55739143 https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-محلات-احترافي-في-الجزائر-bordj-bou-arreridj-algerie-d55739143
- d20975347 https://www.ouedkniss.com/alger-bab-el-oued-algerie-applications-logiciels-logiciel-de-vente-d20975347
- d50074436 https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-المحلات-التجارية-gestion-commercial-tipaza-algerie-d50074436
- d54491714 https://www.ouedkniss.com/logiciels-abonnements-برنامج-تسيير-المحلات-التجارية-باحترافية-ghardaia-algerie-d54491714
- d44596393 https://www.ouedkniss.com/logiciels-abonnements-procommerce-logiciel-de-gestion-stock-performant-pour-pme-bordj-el-kiffan-alger-algerie-d44596393
- d32976718 https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-commercial-setif-algerie-d32976718
- d16024589 https://www.ouedkniss.com/setif-algerie-applications-logiciels-logiciel-de-gestion-commerciale-d16024589
- d26141727 https://www.ouedkniss.com/alger-constantine-oran-bab-ezzouar-bir-el-djir-algerie-applications-logiciels-logiciel-pour-commerçant-et-entreprise-d26141727
- d15825713 https://www.ouedkniss.com/securite-alarme-logiciel-de-gestion-commerciale-dar-el-beida-alger-algerie-d15825713
- d11537219 https://www.ouedkniss.com/applications-logiciels-tradman-logiciel-gestion-commerciale-alger-centre-algerie-d11537219
- d57871113 https://www.ouedkniss.com/pc-de-bureau-logiciel-restauration-cafes-et-patisseries-tebessa-algerie-d57871113
- d45414283 https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-patisserie-oran-algerie-d45414283
- d56350090 https://www.ouedkniss.com/projets-etudes-systeme-de-gestion-boutique-telephonie-boumerdes-algerie-d56350090
- d24408171 https://www.ouedkniss.com/alger-dar-el-beida-algerie-all-in-one-vente -installation-gestion-de-caisse-d24408171
- d12993513 (easykes; see 01 appendix)
- d56260597 https://www.ouedkniss.com/imprimantes-matricielles-imprimante-epson-lx-350-matricielle-a-impact-oran-algerie-d56260597
- d49323893 https://www.ouedkniss.com/imprimantes-matricielle-imprimante-epson-lx-350-bab-ezzouar-alger-algerie-d49323893
- d56118616 https://www.ouedkniss.com/imprimantes-matricielles-epson-lx-300-imprimante-matricielle-9-aiguilles-80-colonnes-ideal-multi-copies-comptabilite-100-neuf-dely-brahim-alger-algerie-d56118616
- d46677755 https://www.ouedkniss.com/imprimantes-matricielle-imprimante-epson-lx-300-pro-kouba-alger-algerie-d46677755
- d47525036 https://www.ouedkniss.com/imprimantes-matricielles-imprimante-epson-lq2090-matricielle-lq-2090-ii-ain-naadja-alger-algerie-d47525036
- d36430660 https://www.ouedkniss.com/imprimantes-laser-imprimante-matricielle-epson-lq-680-lq680-pro-106-colonnes-ain-naadja-gue-de-constantine-alger-algerie-d36430660
- d27284116 https://www.ouedkniss.com/sidi-bel-abbes-algerie-lecteurs-video-audio-caisse-enregistreuse-casio-se-g1-d27284116
- d26906791 https://www.ouedkniss.com/alger-les-eucalyptus-algerie-magasins-caisse-enregistreuse-sharp-لاكاس-شارپ-d26906791
- d54466271 https://www.ouedkniss.com/autre-tpe-upt1000-terminal-de-paiement-electronique-birkhadem-alger-algerie-d54466271
- d57947376 https://www.ouedkniss.com/autre-caisse-tactile-smartpos-a3-intel-j1900-4-go-ssd-64-avec-afficheur-client-hydra-alger-algerie-d57947376
- d38313595 https://www.ouedkniss.com/autre-compteuse-de-billets-haute-detection-faux-avec-2-afficheurs-ecran-client-dar-el-beida-alger-algerie-d38313595
- d54946393 https://www.ouedkniss.com/autre-modem-huawei-e3531-bab-ezzouar-alger-algerie-d54946393
- d26262623 https://www.ouedkniss.com/industrie-fabrication-balance-de-comptage-cas-pr-c-birtouta-alger-algerie-d26262623
- d49341647 https://www.ouedkniss.com/composants-electroniques-balance-electronique-guelma-algerie-d49341647
- d22748294 https://www.ouedkniss.com/magasins-balance-30-kg-mizane-ميزان-الكتروني-pro-pesage-birtouta-alger-algerie-d22748294

**Local verification [L]** (this machine, Windows 11 Pro 10.0.26200, Node 24, Windows PowerShell 5.1):
- `Intl.DateTimeFormat('fr-DZ',{timeZone:'Africa/Algiers',timeZoneName:'longOffset'})` gives UTC+01:00 in January and July.
- `Intl.NumberFormat('ar-DZ').format(1234567.5)` gives `1.234.567,5`.
- `Intl.DateTimeFormat('ar-DZ-u-ca-islamic-umalqura')` renders 2026-09-28 as "17 ربيع الآخر 1448 هـ".
- `Windows.Media.Ocr.OcrEngine.AvailableRecognizerLanguages` gives en-US, fr-FR; ar-SA is not supported.

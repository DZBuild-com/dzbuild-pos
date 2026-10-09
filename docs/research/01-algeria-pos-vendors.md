# 01 — Algerian POS / Logiciel de caisse / Gestion commerciale / Gestion de stock: full market map

Research date: 2026-09-28. Scope: every product sold or popular in Algeria (local editors and foreign products sold there). For each: pricing in DZD, platforms, languages, verticals, full feature lists, licensing, what users say, and piracy signals. Built for the DZBuild free, offline-first Windows POS project.

---

## 0. Method, sources and caveats

**Sources**

1. **Web search** in French, Arabic and English. Queries included:
   - "logiciel de caisse Algérie"
   - "logiciel gestion commerciale Algérie"
   - "logiciel gestion de stock Algérie prix DA"
   - "logiciel point de vente Algérie superette"
   - "logiciel facturation Algérie NIF NIS"
   - "برنامج تسيير المحلات التجارية الجزائر"
   - "برنامج كاشير الجزائر فليكسي ميزان"
   - "logiciel caisse Algérie licence à vie"
   - and others.
2. **Vendor websites and their feature and pricing pages**, fetched directly (see section 13).
3. **Ouedkniss.com**, Algeria's dominant classifieds marketplace, where most small POS vendors sell.
   - The site is a JavaScript app, so I queried its public GraphQL endpoint (`api.ouedkniss.com/graphql`, the same data the site shows) on 2026-09-28.
   - I ran 28 searches (FR/AR), from "logiciel gestion commerciale" and "برنامج تسيير" to vertical terms, hardware terms, "logiciel cracké", sage, odoo, dolibarr and ebp.
   - This gave **505 unique listings, 456 of them software-related**. For each I pulled the view count and the comments with replies. I read the full description of about 110 listings.
   - View counts accumulate over each listing's life, so they measure interest, not sales.

**Caveats**

- Feature lists are **vendor claims**. I did not install or test any product.
- Many Ouedkniss sellers post a symbolic price (1 DA, 10 DA) to hide the real one. These show as "n/a".
- Facebook pages, YouTube descriptions and Ouedkniss mirror sites (fibladi) could not be read.
- The search stopped before every lead was followed. Foreign-product presence (Loyverse, Daftra, DEXEF, PC Compta, EBP) is therefore only lightly covered and marked **unverified**.
- Ouedkniss listings are cited as `[dNNNNNNNN]`. The links are defined at the end of the report.

---

## 1. Executive summary (20 key findings)

1. **The market is very fragmented.** It has three layers:
   - About **15–20 established editors** selling ERP and commercial-management software through integrators.
   - **Hundreds of micro-editors and hardware resellers** selling Windows desktop POS and stock software on Ouedkniss.
   - A **new cloud/SaaS wave**: Almawarid, Codeshade, LogiTools, Symloop, Fisira, eFawtara, SouqDzayer, Restorium, Inyad/Mahaal.
2. **Windows desktop, offline, lifetime licence is the default.** Listings repeat "fonctionne sans internet", "licence/activation à vie", "sans abonnement" and "mises à jour gratuites".
3. **Typical small-shop licence: 8,000–35,000 DA, paid once.** Across 194 Ouedkniss software listings with a real price (≥ 3,000 DA), the **median is 18,000 DA**:

   | Price band | Listings |
   |---|---|
   | under 10k DA | 31 |
   | 10–20k DA | 72 |
   | 20–40k DA | 49 |
   | 40–80k DA | 21 |
   | 80–150k DA | 16 |
   | 150k DA and above | 5 |

   Mid-market packages cost 70,000–133,000 DA (SmartCom is 95,000 / 110,000 DA TTC). Full hardware and software packs cost 85,000–170,000 DA.
4. **Free products exist, but all are limited:**
   - **CRESCENDO POS Free:** 0 DA. Single PC only, capped at 2 tills, 2 users and 2 company files. Needs an activation key. French only.
   - **Klick "Stock Express":** "gratuit" per the vendor. Limits unknown, and a reseller sells "Klick Stock Standard" at 9,900 DA.
   - **Almawarid free plan:** cloud. 3 invoices, 1 payslip and 1 contract per month, no PDF.
   - **Dipposoft:** has a "free version". Limits unknown.

   No product is at once free, full-featured, offline, Arabic right-to-left and multi-PC. **This is the opening.**
5. **Features buyers expect everywhere:**
   - customer credit ("كريدي" / ardoise) with partial payments (versements) and a credit ceiling (plafond)
   - several price levels (retail, semi-wholesale, wholesale, reseller); 3–6 is usual, COMMSoft allows 13
   - carton/pack/unit selling (colisage)
   - weighed sales with barcode-printing scales
   - several barcodes per product
   - expiry-date alerts
   - supplier debts
   - cash float (fond de caisse) and daily close with variance
   - several tills on a LAN
   - NIF/NIS/RC/AI on documents, VAT at 19 % and 9 %, stamp duty (droit de timbre)
   - invoice, delivery note (BL), proforma, credit note (avoir)
   - barcode label printing
   - Arabic and French interface
6. **Algeria-specific differentiators:**
   - **Flexy** (mobile top-up) management.
   - "Real / fictive / virtual" invoices and **billable vs. non-billable stock**. This is widespread but a legal grey area.
   - **État 104, G50 and Zakat** calculations.
   - **Bon de route** (delivery slip with driver and vehicle).
   - **Instalment sales** (facilité / تقسيط).
   - **CHIFA/CAMSSP** export for pharmacies.
   - Phone **repair** tracking.
   - **WhatsApp receipts**.
   - **Edahabia / CIB / BaridiMob** payment modes.
7. **Till speed sells.** Keyboard-only operation with reprogrammable F-keys, several open tickets and several clients served at once (DS RAPIDE, PME PRO, SAFE POS).
8. **Old PCs matter.** Sellers advertise Windows 7 and even XP support, 512 MB RAM, a 3.24 MB installer, "ultra-léger".
9. **Tills that keep selling when the server fails are a premium feature.** Offline multi-till with later sync appears in TifawtTech Supermarché, ATS POS 3.20 and Megatheque DELTA ("serveur non stop").
10. **An owner mobile app is now expected.** An Android app showing live sales, cash and stock is bundled even with 15–20k DA desktop products (AMPRO, MekaStore, AlgoStock, Zedkira Pro, Dipposoft).
11. **AI features have reached cheap products.** Supplier-invoice OCR is offered by GestiumPRO Scaneye, LogiTools, AlgoStock Elite, Factor Facile and Fisira. Some add a French/Darija chat assistant.
12. **Licences are tied to the machine.** Each licence gets a serial per PC, "stays valid after formatting", can be re-issued when the CPU changes, or ships on a USB key. Installation is done over AnyDesk or TeamViewer, and support runs over WhatsApp.
13. **Piracy and trust are real problems.**
    - Cracked **WinDev 25** (the usual tool for local desktop apps) sells openly on Ouedkniss at 30,000 DA.
    - Almawarid warns publicly against cracked management software.
    - Buyers doubt cheap tools: "a program at 8,900 DA can't be serious".
14. **What buyers keep asking:**
    - "prix?"
    - "version démo?"
    - "multi-stock?"
    - "2nd PC?"
    - "sales and repairs in one?"
    - "jewellery / instalments / packaging shop?"
    - "demo won't install", "refund?" (the answer is no)
15. **Hardware sellers are the main distribution channel.** Many "editors" are really POS-hardware shops bundling a licence: Digital Company / General IT, Asalix/Auradev, Tpv Technologies, Ecopos, Danisoft, Expert POS, Clyo, Computime.
16. **Restaurants are a separate sub-market.** Desktop products cost 15,000–42,000 DA; SaaS starts around 25,000 DA. Common features: tables, a kitchen printer per station, add-ons (suppléments), order display on a TV, QR menu, waiter phone app, delivery.
17. **Enterprise and foreign ERPs are present but priced on quote.** Sage 100 (via BSC Algérie), Odoo (via ITHRYTEC, Digitaliac, Digisys, Finoutsource, Numidit), Dolibarr (via Doliflex) and Gestimum (via NTIC-SOFTPRO).
18. **SaaS priced in dinars is emerging:**

    | Product | Price |
    |---|---|
    | Almawarid | 20,000 DA/yr |
    | LogiTools | 15,000 DA/yr |
    | Fisira | 11,900–35,000 DA/yr |
    | Symloop | 35,000–120,000 DA/yr |
    | Codeshade | 7,000–9,000 DA/month |
    | Restorium, GestiumGo | from 25,000 DA HT |
19. **Certification from 1 January 2027.** The **Loi de finances 2026** requires cash-register software to be certified, or covered by a vendor conformity commitment, for VAT-registered sellers doing B2C sales.
    - The software must guarantee inalterability, security, conservation and archiving with an audit trail.
    - Cirtasoft cites a 25,000 DA fine as an example.
20. **The opportunity.** A free, polished Windows POS covering the **union** of these features would undercut everyone. It should be:
    - in Arabic (right-to-left) and French
    - offline
    - importable from Excel and from competitors
    - linked to a mobile owner app through DZBuild OAuth
    - able to send WhatsApp receipts
    - ready for the 2027 fiscal rules

---

## 2. Market structure

| Segment | Buyers | Platform | Price | Examples |
|---|---|---|---|---|
| **A. Established ERP / commercial-management editors** (10–35 yrs) | SMEs, distributors, pharma, industry, chains | Windows client/server (SQL Server, Oracle), increasingly web | On quote; 95k–200k+ DA per module or pack | Onyx (S2I / Ultimate Solutions), Megasoft, IntelliX Silwane, Inabex, Devoro, BIG Informatique, Smart Partner, Megatheque, Cirtasoft, Elmoumtez, Winalco, BBS, Tayssir |
| **B. Micro-editors: Windows desktop POS + stock** | Superettes, grocers, cosmetics, clothing, phones, auto parts, hardware stores | Windows desktop, offline, single PC or LAN | 5k–50k DA lifetime | PME PRO (TifawtTech / PRO SOLUTION / General IT), Quick Fact, SMART-Com, ITSCOM, GFS, Zsoft, SAFE POS, DS Software, AMPRO, MekaStore, AlgoStock, Zedkira, Dolisoft, Crescendo, Flampex, FlamePOS, KERS POS… |
| **C. Hardware integrators bundling software** | Same as B | Touch terminal + printer + drawer + scanner + licence | 24.5k–170k DA per pack | Digital Company / General IT, Asalix (Auradev), Tpv Technologies, Ecopos, Danisoft, Expert POS, CLYO Algérie, Computime (CaisseKom), Eurequat, OptiQ |
| **D. Restaurant / café POS** | Fast food, pizzeria, café, tea room | Windows touch; SaaS on tablets | 15k–42k DA lifetime; SaaS from 25k | Restorium, FoodMax, CoffeLine, Msk Ozone, Kiober Food POS, Menu Max, CaisseKom Resto, Crescendo Restauration, Symloop, INNOVA POS, PRO_Resto |
| **E. Cloud / SaaS / mobile** | Tech-savvy SMEs, multi-branch | Web + Android (some with a Windows offline client) | 7k DA/month to 120k DA/yr | Almawarid, Codeshade, LogiTools, Symloop, SouqDzayer, FlustockX, Inyad (Mahaal), Tayssir, SAMA VISION, CSI-Stock |
| **F. Invoicing-only SaaS** | Services, self-employed | Web | 11.9k–35k DA/yr | Fisira, eFawtara, Costy, Factury, Fatoura, InnovaInvoice |
| **G. Foreign ERPs via local partners** | Mid-size and large companies | Desktop / cloud | On quote (Sage 100 from about €192/month in Morocco) | Sage 100/X3 (BSC), Odoo (ITHRYTEC, Digitaliac, Digisys, Finoutsource, Numidit), Dolibarr (Doliflex, plexus maze), Gestimum (NTIC-SOFTPRO) |
| **H. Vertical niche tools** | Flexy/phone shops, salons, pharmacies, instalment sellers, jewellers | Windows desktop | 3.5k–120k DA | KERS FLEXY, Easy Life Flexy, FASTransfer, Dukan Pro, ECHO, sCom_pharm, PharmaX, "Gestion de Facilité" |

---

## 3. Established editors (segment A)

### 3.1 Ultimate Solutions Algérie / S2I: Onyx Pro ERP (OnyxPOS, OnyxStock)

- **Location:** Bir Mourad Raïs, Alger. Tel +213 28 185 380.
- **Tech:** modular ERP on **Oracle or SQL Server**.
- **Languages:** French, Arabic, English, Turkish.
- **Peripherals:** scales, **CIB payment terminals**, scanners, cash drawers, automatic coin dispensers.
- **Verticals:** retail chains, malls, supermarkets and hypermarkets, confectionery, clothing, shoes, jewellery, perfume.
- **OnyxPOS features:**
  - till and article management
  - multiple payment methods, barcodes
  - damaged/expired article tracking
  - loan management
  - transfers between stores
  - mobile inventory
  - stock by size and colour
  - weighed goods (produce, meat)
  - customers and loyalty
  - promotions and orders
  - users and printers
  - custom receipt layout
  - multi-user reports
  - payment-terminal (TPE) integration
- **Back office (synced):** finance, fixed assets, HR, stock, purchasing, supply chain, production, treasury, dashboards.
- **Terms:** installation by engineers, "one year of updates and free maintenance". No public price.
- **Sources:** https://www.solutionsinformatiques.dz/Logiciel-de-gestion-de-points-de-vente-POS-algerie?lang=fr, https://www.solutionsinformatiques.dz/Inventory-systems

### 3.2 Megasoft (MEGASOFT Office), since 1990

- **Location:** Hydra, Alger.
- **Claims:** 300+ clients, 12,000+ users, 11 products.
- **Desktop suite:** ERP, "MEGA COMMERCIAL" (client/server or single PC), finance (SCF and IAS/IFRS), HR and payroll, GPAO, GMAO.
- **Vertical modules:** auto, pharma, real estate, labs, stock.
- **Megasoft Digital:** TMS, WMS, MES, cloud.
- **Services:** SAP integration, an AI supply-chain agent.
- **Source:** https://www.megasoft-office.com/

### 3.3 IntelliX Group: Silwane ERP (25+ yrs)

- **Offices:** Alger (Bordj El Kiffan) plus East and West branches, Casablanca, Tunis.
- **Products:** Silwane ERP and Silwane .Net (cloud), **iCom** (70+ commercial modules), Compta, Paie/GRH, Mobile/**Androway** (distribution with GPS), Parc, VPN (multi-site retail), Immobilisation/BIP, **Comptoir** (retail POS), Signature (pharmacy), Pharma, Santymed.
- **Languages:** French, Arabic, English.
- **Trial:** 30 days.
- **Source:** https://www.intellixgroup.com/

### 3.4 Inabex, since 1998

- **Commercial management:**
  - **Works:** purchase and sales cycle, stock, cash, inventory.
  - **AfterWave:** SQL Server; also a Pharma edition.
  - **Zorg:** Commercial, Payroll, Accounting (SCF/IFRS).
- **Pharmacy:** **PharmaX Connect** (CHIFA, barcode, loyalty), PharmaCom (distributor, e-invoice), PharmaLab.
- **Mobile and web:**
  - **DirectWay:** field sales, returns, orders and collections with barcode.
  - **ArreniA:** multi-store sync with central purchasing.
  - **LiveData:** real-time stock in the cloud.
  - **Xycomat:** picking.
  - **InaBox:** mobile POS receipts.
  - CloudSave backup.
- **Platforms:** Windows, web, Android. **Languages:** French, English, Arabic.
- **Source:** https://inabex.com/logiciels-de-gestion-en-algerie/

### 3.5 Devoro ERP (Oran)

- **13 modules:**
  - purchasing (quotation requests, EDI)
  - sales (price levels, commissions, multi-warehouse)
  - stock (lots and serials, expiry, FIFO/LIFO/CUMP)
  - distribution (routes, geolocation, e-proof of delivery)
  - PDA (offline order capture, portable printing, collections)
  - BI
  - SCF accounting
  - MRP
  - HR (CNAS/IRG payroll)
  - CRM
  - warehouse management
  - fleet
  - e-commerce
- **Source:** https://devoro-erp.com/

### 3.6 BIG Informatique (Annaba, Alger, Oran): BIGSOFT

- **Products:** Achats Stock, Stock Vente, Paie GRH, time tracking, analytical accounting, hospital, medical practice, pharmacy, WinHotel, transport, education, public procurement.
- **Achats Stock features:**
  - multi-warehouse with automatic valuation
  - receipts, internal consumption and transfers
  - three inventory methods
  - purchase request through to supplier invoice
  - automatic accounting entries
- **Source:** https://biginformatique.com/produits/achats-stock

### 3.7 Smart Partner (Dar El Beida, Alger; since 1998, claims "+4,000 users")

- **Products:** **SmartCom**, **SmartPOS**, SmartAvicenne, **Tawsil** (mobile distribution), barcode hardware.
- **SmartCom prices:** Commercial **95,000 DA TTC**; Counter/POS **110,000 DA TTC**; Plus on quote.
- **SmartCom features:**
  - full sales cycle, multi-warehouse, price management
  - receipts and payments, **bank reconciliation**
  - barcode generation, real-time POS tracking, loyalty
  - automated archiving, **import/export**
  - POS peripherals (ticket printer, drawer, customer display), **PDA inventory**
  - multi-user network
- **Sources:** https://www.smart-dz.com/produits-services/smartcom, http://www.smart-dz.com/
- **Note:** an Ouedkniss seller lists a "SMART-Com" at 25,000 DA [d22499274]. It may be a different product.

### 3.8 MEGATHEQUE (Tizi-Ouzou): DELTA POS and COMMSoft

- **Contact:** Tel +213 26 182 682.
- **DELTA POS features:**
  - invoicing and payment tracking, planning
  - photo catalogue
  - bulk products, generic codes, pack products
  - customer accounts, **loyalty cards**, discounts
  - **offline till**, **scale**
  - **EAN-13 and EAN-8 generation**
  - access control by profile, stock tracking, revenue reports
  - **"serveur non stop"**: replication and automatic sync
  - peripherals: printers, displays, drawers, scales, readers, tablets, PDAs
- **COMMSoft** has the richest feature list found in Algeria:
  - **Documents:** purchase and sales invoices (HT/TTC), quote/proforma, delivery note, credit notes (goods and financial), supplementary invoices, customer orders.
  - **Stock:** multi-depot and transfers, lots with **FIFO/LIFO/CUMP**, expiry, rotating inventory.
  - **Receivables:** batch payments, **aging analysis**, cheques and drafts, **multi-currency debts**.
  - **Articles:** 13-character references, **up to 13 sales tariffs**, images.
  - **Production:** bills of materials and routings, work orders, assembly/disassembly, real cost, cutting plans.
  - **Imports:** landed costs (frais d'approche) folded into CUMP, foreign suppliers, exchange differences, customs rates.
  - **Warranty and serial numbers** (1–36 months; warranty certificate in Word).
  - **Returnable packaging (consignes).**
  - **Sales reps, routes and commissions.**
  - **Pharma distribution:** PPA/SHP prices, wholesaler and pharmacist margins, quotas.
  - **Dashboards:** hourly footfall, VAT collected vs. paid curves, 4-year comparisons.
  - **Other:** multi-user rights, export to Excel/Word/HTML, compressed backup to removable media, multiple company files, multiple banks and tills, transfer to accounting.
- **Sources:** https://megatheque.net/logiciel-point-de-vente-algerie/, https://megatheque.net/logiciel-gestion-stock-commerciale-algerie/

### 3.9 CIRTASOFT (since 2005): Gestium family and Restorium

- **Prices:**
  - GestiumGo (invoicing): **25,000 DA HT**
  - GestiumPRO: on quote
  - GestiumERP (190+ modules): on quote
  - Restorium Starter: **25,000 DZD HT**; Pro and Enterprise on request
- **Languages:** French, English, Arabic.
- **GestiumPRO features:**
  - quotes, POs, invoices, credit notes, payment tracking
  - multi-warehouse stock with rupture alerts, valuation and traceability
  - touch POS with **daily closing**
  - purchasing
  - CRM and loyalty with automated follow-ups
  - delivery and routes with a mobile app and driver tracking
  - treasury: banks, **cheques**, deadlines
  - price grids, seasonal promotions, **quantity pricing**
  - TVA, TAP, standard-form invoices
  - **"Scaneye" AI OCR** of supplier invoices
- **Restorium features:**
  - orders shown on a TV, KitApp kitchen app
  - stock with automatic recipe deduction
  - waiter mobile ordering, BI
  - **160+ access rights**
  - multi-establishment, **QR code per table**, queue display, bill splitting, daily closing
- **Sources:** https://cirtasoft.com/gestiumpro/, https://www.cirtasoft.com/logiciel-restaurant/en/
- **Also:** Cirtasoft publishes the clearest summary of the 2027 certification rule: https://www.cirtasoft.com/certification-des-logiciels-de-caisse-en-algerie-contexte-et-obligations-a-partir-de-2027

### 3.10 Elmoumtez Informatique (Annaba, since 1994)

- **Products:** accounting, payroll, materials accounting, invoicing/quotes, commercial management, **POS**, **restaurant**, treasury, production.
- **Platforms:** PC, all-in-one, tablet, cloud/browser.
- **Source:** https://www.elmoumtez.com/

### 3.11 Winalco (Sétif, since 2002)

- **Products:** Winalco SCF (accounting), Winalco Paie (CNAS/IRG), **Winalco GSCOM** (stock, purchasing, invoicing, quotes, delivery notes), Smart Check (free cheque printing for Algerian banks).
- **Platform:** Windows 8.1/10/11. Free demos.
- **Source:** https://winalco.dz/

### 3.12 Other established and mid-market names

- **NTIC-SOFTPRO** (Azazga): resells and implements **Gestimum** (French, SQL Server).
  - multi-depot, serial and batch tracking, expiry
  - six valuation methods, bills of materials
  - EDI/PDF/XML
  - Source: https://ntic-softpro.com/s_g.html
- **DSSI, Planète CAISSE** (Alger):
  - seller management and till security, profile duplication
  - function unlocking at the till
  - customer risk and credit alerts
  - order → delivery note → invoice → credit note
  - central and per-site stock, transfers
  - A4/A5 printing
  - Sources: https://www.d2si.eu/solutions/gestion_des_points_de_vente, [d22047875]
- **Adisoft:**
  - 5 price categories
  - supplier invoice updates prices automatically, HT/TTC
  - labels
  - full chain quote/order/delivery note/return/invoice/credit note
  - advances and payment terms
  - weighted-average (PMP) inventory
  - sales reps, custom print templates, charts
  - Source: https://www.adisoft-dz.com/logiciel-gestion-commerciale/
- **Tayssir ERP** (Baraki, Alger): web ERP with double-entry inventory, multi-warehouse, reorder points, lots and serials, **dropshipping, cross-docking**, CRM, purchasing. Source: https://tayssir-erp.dz/…/tayssir-erp-logiciel-gestion-stocks-algerie/
- **BBS / BBSCom** (Oran), 35,000 DA, Windows network [d10912645]:
  - stocked and service items
  - composite items (assembly/disassembly)
  - retail, semi-wholesale and wholesale tariffs
  - PUMP, LIFO or last-price valuation
  - price by customer and by quantity
  - depot transfers
  - **automatic customer blocking on unpaid balances**
  - VAT-exempt sales
  - full quote → order → delivery note → invoice chain
  - One buyer comment reports "erreur dans propriété ligne 381".
- **SAMA DIGITAL COMPANY, SAMA VISION ERP** (Khraïssia) [d51171823]:
  - online; subscription or enterprise licence
  - modules: sales, purchasing, stock, invoicing, debts, POS, CRM, marketing, projects, HR, accounting, website/e-commerce, repairs, production
  - multi-company, multi-store, multi-POS
- **INNOVA SOFT EURL** (Bir Mourad Raïs):
  - **products:** INNOVA POS Store, Fashion, Clean (dry cleaning), Beauty; INNOVA COM
  - **features:** touch till, scales, size/colour, **waterproof tickets** for dry cleaning, **commission per service provider**, loyalty
  - Sources: https://www.innova-dz.com/, https://www.innova-dz.com/Solutions-point-de-vente.html

---

## 4. SME desktop POS and commercial management (segment B: the core competition)

Prices are in DA as posted. "LT" = lifetime licence. Unless noted: Windows desktop, works offline.

### 4.1 Summary table

| Product | Editor / seller | Location | Price (DA) | Languages | Network | Notable | Source |
|---|---|---|---|---|---|---|---|
| **PME PRO** | TifawtTech | Dar El Beida | 70,000 listed ("from 20,000" in reply) | FR | single PC + LAN | Most-viewed software ad (39,497 views); 6 sale modes; commissions; GMAO add-on | [d12505797] |
| **Logiciel Gestion Supermarché** | TifawtTech | Dar El Beida | 70,000 | FR | LAN with offline tills | Loyalty, credit and consumption cards; tills sync with a button | [d30370559] |
| **POS Light** | TifawtTech | Dar El Beida | n/a | FR | single PC | **Flexy option** | [d30369909] |
| **PRO-PME/PMI**, **PME PRO** + Android distribution app | PRO SOLUTION | Bab Ezzouar | 30,000 | FR | single PC / LAN | "Accounted / non-accounted invoices"; safe (coffre); support 7 days, 8h–21h | [d11162776], [d23967633] |
| **PME PRO**, SMART packs | Digital Company / General IT | Mohammadia, Kouba | packs | FR + AR | 1 server + clients | GMAO, production, import; Excel/PDF export; action history | [d50572699] |
| **Quick FACT** | PC LINE | Alger centre | 25,000 | FR | single PC + server | Multiple company files and depots, expiry; export to Excel/Word/PDF; email | [d16488297] |
| **ITSCOM**, **ITSpoint / ITSstock** | Its2010 | Akbou | 28,000 / 35,000 | FR | single PC | "Key for life"; XP–8.1, 512 MB RAM | [d17834397], [d6894011] |
| **SMART-Com** | seller | Chlef / West | 25,000 | FR, AR product names | single PC / LAN | Activation survives formatting; backup on close | [d22499274] |
| **StockCom** | seller | Biskra | 20,000 | FR/AR documents | single PC / LAN | Groups several delivery notes into one invoice | [d23906352] |
| **GFS** | Chrono Informatique; Chlef seller | Hydra; Chlef | 25,000 | FR/AR | – | Popular (YouTube tutorials) | [d33391286], [d30034727] |
| **Smart Gestion V2.0** | M. Idris | Aokas | 35,000 (single PC) | FR | single PC | Real and "virtual" invoices and stock; 21-day trial | [d24550112] |
| **WINSTO** | LOG103 | Sétif | n/a | FR | client/server, multi-till | **Separates billable and non-billable stock** | [d21173889] |
| **WINCOM** | seller | Sétif | 85,000 (up to 120k with hardware) | FR | – | Wholesale, semi-wholesale, retail | [d15298462] |
| **ZSOFT** | seller | "58 wilayas" | n/a | AR/FR | – | Real, fictive and proforma invoices; dead stock | [d40249015] |
| **SAFE POS**, **PMO Pro** | VAST SOFT | Bouzaréah | n/a | FR/AR | single PC / LAN | Several barcodes per product; loyalty; mobile distribution | [d24881513], [d39454663] |
| **PROMOOO** | trSOFT | Alger | 22,000 | FR | single PC / LAN | SAFE POS feature set + production + import/export | [d26730980] |
| **easykes** | seller | Oran | 25,000 | FR | – | Turnover by hour (peak hours) | [d12993513] |
| **CRESCENDO POS** | Crescendo Software | – | **Free 0**; Commerce Pro 20k; Fashion 25k; Resto 30k; Loisir 35k | FR | Free: single PC; Pro: LAN | Support: Silver 10k/yr, Gold 20k/2 yrs, Platinum 30k/3 yrs | crescendo-software.com |
| **Zedkira Inventory** | Zedkira | Tizi Ouzou | 32k / 49k / 65k; +4k per user, +8k per PC, +6k per phone | FR | multi-user | Invoice-rule compliance (D.E. 05-468); 15-day trial | inventory.zedkira.com |
| **AlgoStock** | AlgoSynapse | – | 19,500 LT; +5k/yr (Pro Connect); +12k/yr (Elite AI) | AR (RTL) / FR / EN | LAN | Local PostgreSQL; 5 roles; repairs; CIB/Edahabia/BaridiMob | algosynapse.com/algostock |
| **Dolisoft PRO** | Dolisoft | – | **12,000 per PC**, LT | FR/AR | multi-user | USB-key delivery; TVA, stamp duty, **état 104, Zakat**, production | dolisoft-dz.com |
| **ProPOS**, **AMPRO** | Dif Softs (2022, 21 staff) | – | AMPRO 12,000–15,000 (LT key, free updates for life) | FR (+EN) | unlimited tills | Currency gain/loss; 45+ reports (ProPOS); free Android app (AMPRO) | difsofts.com, [d56142529] |
| **Multisphere COM Silver** | seller | Sétif | 25,000 | FR/AR | multi-user | Debt-collection module | [d56573309] |
| **MekaStore** (Starter / Pro / Network / Cloud / E-Com+ / Park) | seller | Bab Ezzouar | 20,000 / 50,000 / from 100,000 | AR/FR/EN | Network edition | Phone monitoring app | [d56562698] |
| **DS SOFTWARE**, **DS RAPIDE**, DS MARKET | seller | Ghriss, Mascara | 15,000 each | AR/FR/EN | single or multi-PC | Keyboard-only with reprogrammable keys; butcher (by weight) edition | [d57221382], [d57221406] |
| **Gestion POS** | seller | Tlemcen | 26,000 | – | – | Credit, permissions | [d57957894] |
| **SmartStock** | seller | Alger | 40,000 | FR/AR/EN | – | Local data only | [d55281649] |
| **FA_Gstock** | seller | Bougara | 10,000 | **FR only** | single PC | Serial tracing; 100 parallel tabs; licence re-issued if PC changes | [d27434885] |
| **Stock Standard** | Net Informatique Services | Blida | 18,000 | FR | – | Full purchases, sales, stock | [d21919164] |
| **Stock Express / Klick Stock** | Klick Algérie (Fouka) | – | "free" (vendor); Standard 9,900 (reseller) | FR | Standard / Server / Cloud | Multi-depot, colisage | klick.app, [d54257126] |
| **KIO POS / kiopos** | sellers | Alger | 52,900 or 15,000 | AR/FR | – | "Delivery to 69 wilayas" | [d55740700], [d56045476] |
| **OMNI CAISSE PRO** | seller | Alger | 12,000 LT | FR | – | Expiry alerts | [d55706998] |
| **Dahabia POS** | seller | Laghouat | 15,000 (title) / 35,000 (body) | – | – | Weighed goods, credit | [d56301202] |
| **GDS PRO** | Digideel | Bab Ezzouar | 25,000 | FR/AR | LAN | Claims 3,000+ clients | [d57517611] |
| **Flampex** | Flampex | Hussein Dey | 8,500 LT | AR | – | Repairs; 32- and 64-bit | [d57312381] |
| **FlamePOS** | FlamePOS | Alger | 5k per year, 10k for 3 yrs, 15k for 5 yrs, 20k LT | AR/FR/EN | – | Theme customisation | [d55784365] |
| **KERS POS**, **KERS FLEXY** | KERS | Souk Ahras | 10,000 / 25,000 | AR/FR | – | See 4.2 | [d54074597], [d56463136] |
| **WR manager** | seller | Aïn Oussara | 19,999 | AR | – | 3 independent till screens; can sell into negative stock | [d34059266] |
| **Tajir** | seller | Oran | 12,000 | AR | – | 4 price levels; "smart purchase advice" | [d22296334] |
| Superette-Grossiste | seller | Oran | 9,900 | AR/FR | single PC / LAN | SMS to debtors; SMS when a wanted product is back; CCTV support | [d18778898] |
| **Dipposoft** | seller | Djelfa | 16,000 + **free version** | multi-language | LAN | Google Drive backup; fraud audit; mobile app | [d56826623] |
| **ATS POS 3.20** | seller | Annaba | 8,900 | FR | multi-till with offline sync | Reads scale barcodes; sensitive-action controls | [d57939003] |
| **SuperettePro** | seller | Koléa | n/a | FR | – | Runs on Windows 7 and old PCs | [d56299357] |
| Smart Barcode System | seller | Annaba | 3,500 launch (15,000 normal) | AR | – | Data survives reinstall | [d56453599] |
| "Magasin & Stock 2026" | seller | Douera | 5,000 launch (9,000 normal) | AR/FR | Wi-Fi sharing | Claims Windows, Mac and Linux; sells by weight, length or volume | [d56207773] |
| Bou Saâda till | seller | M'Sila | 8,000 | AR/FR/EN | – | Built for weak PCs; installed over AnyDesk | [d55290118] |
| **HiroSystem** | seller | Birtouta | packs | AR | – | Starter / Pro / Premium hardware packs | [d52265413] |
| **Arkan** | seller | Annaba | n/a | AR | – | – | [d55107943] |
| **Idipos / CAISSE PRO** | seller | Rouiba | 85,000 (with PC) | FR | – | – | [d26735464] |
| **CaisseKom** | Computime | Oran | 25,000 | FR | – | Butcher and restaurant editions | [d14435454] |
| **ShopLine** / CoffeLine / RestoLine | seller | West | 18,000 | FR | single PC / LAN | Many vertical editions | [d25951270] |
| **CSI-Stock** | Codage Solution Industrie | Birkhadem | 150,000 | FR | Web app on a local or cloud server | Multi-store; dark mode | [d56061973] |
| **Shoperra** | seller | Oran | 59,000 | AR | – | Stock + ready-made website | [d51024226] |
| **GESTOCK** | seller | Khemis El Khechna | 12,000 | AR/FR | – | Warehouse focus | [d57732597] |
| **Orbis** | seller | Chéraga | 15,000 | FR/AR | web + Android | Wholesale pre-sale offline, delivery tours | [d56087176] |
| iStock Pro 2026 | seller | – | 12,000 | – | – | – | [d55690765] |
| BzPos | seller | – | n/a | – | – | – | [d51405412] |
| GestaCaisse | seller | – | n/a | – | – | – | [d18252240] |
| FASTSHOP | Tpv Technologies | – | n/a | – | – | – | [d11018311] |
| AMSOLUTION | seller | – | n/a | – | – | – | [d28927989] |
| Expert POS | Expert POS | – | n/a | – | – | – | [d12373295] |
| Ecopos | Ecopos | – | n/a | – | – | – | [d40287712] |
| Danisoft packs | Danisoft | – | 118,000–133,000 (i3 touch all-in-one + printers + drawer + scanner) | – | – | – | [d44928828] |
| **Asalix / Auradev** packs | Asalix | Alger | from 24,500; promo 20,000 | AR/FR | – | 8–9 packs per vertical; 90 days remote support | [d30782671], [d35537273] |
| **SOFTEL GestCom** | SOFTEL Solutions | Mohammadia | n/a | FR | multi-PC, multi-company, multi-user | Customers by wilaya; serials; weighted-average cost; cheques; bank reconciliation; full action history | [d52503089] |
| **Factor Facile** | Devion | – | one-off purchase | AR/FR/EN | multi-device | AI assistant; photo → invoice; encrypted data; also on Android | devion.cloud/factorfacile |
| **ECHO** (salons) | seller | Alger | 8,000 | AR | – | Worker commission %; WhatsApp ticket | [d57214320] |
| **Dukan Pro** (phone repair) | seller | Alger | 9,000 | AR | – | Technician PIN; audit log; 3.24 MB | [d57584481] |
| **Gestion de Facilité** (instalments) | seller | Dar Chioukh | 11,000 | AR/FR | – | Instalment schedules; late payments | [d52615477] |
| El Eulma gestion commerciale | seller | Sétif | "affordable" | FR | – | Staff management; **état 104, G50, Zakat** | [d29886941] |
| "etat 204" program | seller | El Attaf | 5,000 | AR | – | – | [d57434988] |

### 4.2 The richest small-vendor feature sets

**KERS POS** (10,000 DA). The richest list in segment B. Its "Matplotlib" charts suggest it is written in Python (inferred). [d54074597]

- **Till:**
  - touch screen and barcode
  - **reads the scale's weight automatically**
  - carton vs. unit pricing
  - **credit sale with ceiling alert**
  - **mixed payment (cash + credit)**; cash, cheque or transfer
  - discounts in % or DA
  - **routes items to a kitchen printer**
  - hold invoices
  - returns and cancellations need permission
  - **RFID loyalty cards**
  - table management
- **Products:** recipes and composite items that deduct their components; variants (size, colour).
- **Dashboard:**
  - live mode
  - KPIs: revenue, net profit, money owed by customers, returns, expenses
  - **daily sales target bar**
  - peak hour, best product, best cashier, **average basket**
  - charts
  - **AI-written summary of the financial situation**
  - net margin, ROI, **stock rotation**
- **Stock:**
  - lots and expiry dates
  - **automatic EAN-13 generation**, several barcodes per product
  - **removes backgrounds from product photos**
  - stock and expiry alerts
  - movement history
  - adjustments with a reason
  - favourites
- **Documents:**
  - standalone invoice generator
  - A4 and 80 mm invoices, proforma, delivery note
  - **bon de route** (driver, vehicle, destination)
  - **VAT + stamp duty**
  - document archive with reprint
  - **CSV/Excel export of every table**
- **Customers and suppliers:**
  - NIF/NIS/RC/AI
  - credit ceiling
  - partial payments with **payment receipts**
  - supplier purchase orders
  - debt tree
  - **overdue supplier debts**
  - loyalty points
- **Finance:** expenses by category, bank accounts.

**TifawtTech Supermarché** (70,000 DA) [d30370559]

- **Purchasing:** purchase order, goods receipt, return, invoice.
- **Sales:** quote, order, delivery note, return, invoice, counter sales.
- **Tills:** unlimited, with **transfers between tills** and Excel export.
- **Cards:** loyalty, credit and consumption cards.
- **Other:** promotions, unlimited company files, stock-minimum and expiry alerts, bank balances, **inventory from a mobile app**, charges, losses and depreciation, multi-depot, cashier task rights.
- **Resilience:** tills keep working offline if the server or network fails. Product changes are pushed to tills with a manual sync button.

**TifawtTech PME PRO** [d12505797]

- **Base licence:** 1 user, 2 company files, 2 till/bank accounts and 4 payment modes (cash, cheque, on account, transfer). Each is extendable to unlimited as a paid option.
- **Selling:** 6 sale modes. Products can be filtered by quantity, user, seller, family, **wilaya, commune**, customer or supplier.
- **Sales force:** commission scales.
- **Data:** backup, restore and database repair.
- **Peripherals:** scanner, ticket printer, drawer, **customer display, scale**.
- **Operation:** function keys and touch screen.
- **Accounting:** export to the accountant's software format.

**SAFE POS** (VAST SOFT) and **PROMOOO** (trSOFT) share a description [d24881513] [d26730980]:

- **Selling:** by barcode, name or favourites list.
- **Barcodes:** **several per product** ("for the other flavours").
- **Stock and prices:** real time across the server and all tills.
- **Sale modes:** three (wholesale or retail).
- **Suppliers:** balances and payments.
- **Customers:** balances, payments and **loyalty points** (as cheques or discounts).
- **Profit:** **net profit after charges and losses**.
- **Daily close:** detailed day report with a **float for each till**.
- **Network:** single PC or LAN.
- **Reports:** histogram of the most profitable products.
- **Data:** one-click backup and restore.
- **Security:** user rights.

PROMOOO adds production (buy raw material, sell finished goods) and export/import/printing of documents, barcodes and price tickets.

**DS RAPIDE** (15,000 DA): a keyboard-first till [d57221406]

- **Shortcuts:** F1 hold, F2/F3 quantity, F4 search, F5 delete line, F6 clear, F9 pay, F10 history, F12 cancel, Ctrl+D discount, Ctrl+R return, Ctrl+C customer, Ctrl+B label.
- **Switcher offer:** "we remap your keys to match your old software".
- **Display:** dense numbers; light and dark panels.
- **Tickets:** several open tickets, merge tickets, free price, advances.
- **Wholesale:** wholesale price with carton/pack/unit and variants.
- **Data:** Excel/CSV import and export.
- **Setup included:** installation, entry of the existing product list, training.

**AlgoStock** (19,500 DA lifetime)

- **Payments:** cash, **CIB/Edahabia, BaridiMob**, customer credit.
- **Documents:** thermal or A4 printing, delivery notes, quotes.
- **Stock:** multi-warehouse, reorder suggestions, expiry, EAN, **bulk price update**.
- **Compliance:** NIF/NIS/RC.
- **Repairs module:** tickets, parts, labour.
- **Roles:** 5.
- **Mobile app:** synced over **local Wi-Fi, no cloud**.
- **Elite (AI) tier:** OCR, natural-language questions, forecasts, anomaly alerts.
- **Data:** automatic backups to USB or network.
- **Language:** switch without restarting; Arabic right-to-left.

**Dolisoft PRO** (12,000 DA per PC)

- **Documents:** quotes, delivery notes, A4 invoices, receipts.
- **Stock:** alerts, barcodes, batch tracking.
- **Production:** raw material to finished goods.
- **Money:** till, customer debts.
- **Tax:** **VAT, stamp duty, "statement 104", Zakat**.
- **Other:** statistics, multi-user, French and Arabic.
- **Sales terms:** delivered on USB key, cash on delivery, "buy 2, 3rd free".

**Zedkira Inventory**

- **Base:** POS, stock, invoicing, customers and suppliers, barcodes, credit, expiry, inventory, activity log, statistics.
- **Pro:** mobile app, credit at the till, multiple price tiers, customer statements, bundle sales, repair workshop.
- **Business:** services, credit limits with alerts, accounting export.
- **Activation:** local, with an optional online licence check.

**CRESCENDO POS**

- **Free edition:**
  - product families, automatic CUMP, basic barcode, stock
  - **max 2 tills, 2 users, 2 company files**
  - **single PC only**
  - activation key sent by email (can be activated offline)
  - Windows 7+ and Windows 10 tablets; no Android, iOS or Mac
- **Pro:** departments, promotions, photos, composite items, expiry, unlimited everything, LAN.
- **Fashion:** sizes and colours.
- **Restaurant:** floor plans, menus, pizza composition, kitchen orders.
- **Also:** gift vouchers, loyalty, automatic restocking, multi-site, losses, free items (offerts).

### 4.3 Restaurant and café specialists

| Product | Price (DA) | Features | Source |
|---|---|---|---|
| El Eulma resto system | Basic 15,000 LT (max 2 printers); Normal from 25,000; menu entry service 3,500 | Normal adds unlimited printers, **QR menu**, **waiter phone app**, **orders on TV**, 6 months support | [d57659580] |
| **FoodMax** (Oran) | 18,000 | Tables, cash/kitchen/table tickets, **add-ons**, expenses, **customer credit with limit**, refunds | [d24009309] |
| **Msk Ozone** (Tlemcen) | 35,000 | Touch; stock, staff, rotation by hour; **cloud backup**; key survives formatting | [d30642759] |
| Sétif resto / shop | 35,000 | Cooking modes and kitchen notes, prep tickets, **order taking from a touch remote**, floor plan, **home delivery**, size/colour, loyalty, view from phone over internet | [d39031109] |
| Oran resto/pizzeria | from 38,000 | Waiter orders on phone; chef receives on phone or tablet | [d39658646] |
| POS Restaurant (Draria) | 39,000 | **Online ordering (site + app)**, **card payment**, **table booking**, **caller ID**, email confirmations | [d56978620] |
| **Menu Max** | 42,000 | Digital menu, QR or tablet ordering, kitchen display | [d48086740] |
| Oran restaurant SaaS | subscription | Runs on a 10–15" tablet + Bluetooth printers; **self-order kiosk**; order status board | [d57473655] |
| **Kiober Food POS** (Oran) | 12,000 LT | Tables, counter and delivery; 30-day trial through the **Microsoft Store**; reseller programme | [d55642837] |
| **Restorium** (Cirtasoft) | from 25,000 HT | See 3.9 | cirtasoft.com |
| **Symloop** (Sétif) | 35,000 / 65,000 / 120,000 per year | **Kitchen display**, time-based pricing, SMS loyalty, **API**, offline sync. Hardware prices: touch till 85k, printer 25k, drawer 15k, scanner 12k, tablet 35k, kitchen display 45k | symloop.com |
| **CLYO** (Kouba) | 120,000–160,000 with hardware | Restaurants, bakeries, butchers, fish shops, superettes | [d6203275], [d55066585] |
| Logiwell Resto, RESTO-PRO, ITRI RESTO PRO, PRO_Resto (Facebook), INNOVA POS | n/a | – | [d26432450], [d18723112], [d50571559] |

### 4.4 Vertical niche tools

- **Flexy (mobile top-up):**
  - KERS FLEXY (25,000 DA): Mobilis, Djezzy, Ooredoo plus repairs [d56463136]
  - Easy Life Flexy v5.4 (13,000 DA): Mobilis bill payment, Ooredoo Maxy [d19431931]
  - FASTransfer (20,000 DA) [d39656135]
  - flexy through a USB modem [d9607035]
  - TifawtTech POS Light flexy option
- **Pharmacy:**
  - sCom_pharm pro (120,000 DA): **CHIFA/CAMSSP export**, prescription register, officinal preparations, search by generic name (DCI) [d31231999]
  - Inabex PharmaX (CHIFA)
  - A Thales CHIFA card reader costs 19,500 DA on Ouedkniss.
- **Phone repair:** Dukan Pro, AlgoStock, Flampex, Zedkira Pro, KERS FLEXY. Buyers explicitly ask for "commercial management + repairs" in one product.
- **Salons:** ECHO, INNOVA Beauty.
- **Instalment sales:** Gestion de Facilité [d52615477].
- **Jewellery:** [d44602069], General IT. Features not detailed.
- **Distribution / pre-sale:** Orbis, PRO SOLUTION, PMO Pro (van stock transfers) [d39454663], Tawsil, DirectWay, Androway, Devoro PDA.

### 4.5 Cloud, SaaS and invoicing (segments E and F)

**Almawarid** (Bir El Djir, Oran)

- **Platforms:** web, Android, and a Windows app with an offline/LAN mode.
- **Languages:** French, Arabic, English, Turkish, Chinese.
- **Pricing:**
  - Free plan: till, stock, 3 invoices per month; **PDF export only on paid plans**.
  - **Standard 20,000 DA per year** (100 documents a month); Enterprise 500 documents a month.
  - 14-day refund guarantee.
  - An Ouedkniss "complete pack" sells at 33,000 DA [d56860799].
- **Features:**
  - invoices to DGI rules with NIF, VAT, stamp duty and QR code
  - credit notes that correct VAT
  - sending by PDF, WhatsApp or email
  - 80 mm receipts, till sessions, cash counting (billetage)
  - lots, expiry, weighted-average cost (CMUP)
  - decimal quantities, QR labels
  - Excel/CSV import
  - price tiers per customer type
  - payroll (CNAS/IRG)
- **Migration:** free migration from **Sage, EBP, PC Compta, Ciel, Odoo or Excel** within 48 hours [d56993778].
- **Sources:** almawarid.app (commercial management, stock, blog, tarifs pages)

**Codeshade** (web, "works offline"): Starter 7,000 DA/month, Growth 9,000 DA/month. Arabic (Darija), French, English. Debt tracking, multi-site. codeshade.app

**LogiTools DZ**

- **Price:** 15,000 DA per year, +5,000 DA for product data entry, 1 month free.
- **Features:**
  - AI invoice photo → stock
  - waste and theft log
  - lots
  - customer and supplier credit
  - crossed-out promo prices
  - roles
  - **"tamper-proof fiscal register"**
  - French/Darija AI assistant
- **Source:** logitools-dz.com

**SouqDzayer:** web.

- stock with variants, multi-warehouse
- **reseller portal**, customer catalogue
- orders, quotes, returns, invoices
- POS

**FlustockX:** pan-African; Android, iOS, Windows, macOS, web. No prices shown.

**Inyad / Mahaal:** Mahaal claimed "the most used POS in Algeria, 6,000+ merchants" and has been merged into Inyad (350,000 businesses in 40+ countries).

- **Features:** POS, inventory, recipes, tables, delivery, multi-site, loyalty, gift cards, payroll, e-invoicing, QR ordering.
- **Unverified:** current Algerian pricing (the site's URLs point to Morocco).

**Fisira**

- **Prices:** 11,900 / 22,000 / 35,000 DA per year TTC.
- **Features:** NIF/NIS/RC/AI, VAT, **automatic stamp duty**, **automatic G50**, AI scanning, accounting, e-signature and QR (claims "2026 compliance").
- **Source:** fisira.app

**eFawtara**

- **Invoices:** RC/NIF/NIS/AI, VAT 19/9/exempt, digital stamp, **sequential numbering that can't be changed**, QR and signed XML invoices.
- **Payments:** SATIM card payments.
- **Other:** audit trail.

**Also seen:** Costy, Factury, Fatoura, InnovaInvoice, DigiFact (4,900 DA) [d56709041], innova-invoice [d55015843].

### 4.6 Foreign products and partners

- **Sage:**
  - BSC Algérie (El Achour) calls itself "the 1st Sage integrator in Algeria" and is a Premium partner.
  - Products: Sage 100 commercial management, X3, 100 Online, Paie.
  - Clients include Lafarge, Coca-Cola, Danone and Banque d'Algérie.
  - Priced on quote. Many Ouedkniss ads offer Sage 100 training [d28343846].
- **Odoo:**
  - **ITHRYTEC:** 50+ companies; localised for dinars, VAT, the national chart of accounts and payroll.
  - **Digitaliac:** SCF accounting, VAT, **stamp duty, G50**, payroll, **Yalidine integration**, POS; also runs a reseller programme.
  - Also Digisys, Finoutsource and NUMIDIT.
  - An Ouedkniss "Odoo" listing has 18,508 views.
- **Dolibarr:** Doliflex (page fetch failed; unverified); installers [d30720309].
- **Gestimum:** via NTIC-SOFTPRO.
- **EBP, Ciel, PC Compta:** seen only as migration sources and in job ads. No official Algerian EBP reseller found.
- **Daftra, DEXEF, Vatoce, Albadr, Siindbad:** surfaced in Arabic searches. **No Algerian localisation evidence** (unverified).
- **Loyverse, Zoho Inventory:** no Algerian evidence (unverified).

---

## 5. Pricing landscape (DZD)

| Model | Range | Examples |
|---|---|---|
| **Free** | 0 | Crescendo Free (limits above); Klick Stock Express (limits unknown); Almawarid free plan; Dipposoft free version |
| **Ultra-low, lifetime** | 3,500–10,000 | Smart Barcode 3,500; "Magasin 2026" 5,000; ECHO 8,000; Bou Saâda 8,000; Flampex 8,500; ATS POS 8,900; Superette-Grossiste 9,900; Klick Standard 9,900; FA_Gstock 10,000; KERS POS 10,000 |
| **Standard, lifetime** | 12,000–35,000 | Dolisoft 12,000 per PC; AMPRO 12,000; OMNI 12,000; Kiober 12,000; Tajir 12,000; DS 15,000; Orbis 15,000; Dipposoft 16,000; Stock Standard 18,000; AlgoStock 19,500; WR manager 19,999; FlamePOS 20,000 LT; MekaStore 20,000; StockCom 20,000; Crescendo Pro 20–35k; Quick Fact 25,000; GFS 25,000; SMART-Com 25,000; Multisphere 25,000; GDS PRO 25,000; KERS FLEXY 25,000; ITSCOM 28,000; PRO-PME 30,000; Zedkira 32,000; BBS 35,000; Smart Gestion 35,000; ITSpoint 35,000 |
| **Mid / pro, lifetime** | 40,000–70,000 | SmartStock 40,000; Menu Max 42,000; Zedkira Pro/Business 49–65k; MekaStore Pro 50,000; KIO POS 52,900; Shoperra 59,000; PME PRO 70,000; TifawtTech Supermarché 70,000 |
| **Editor packages** | 95,000–200,000+ | SmartCom 95,000 / 110,000 TTC; sCom_pharm 120,000; CSI-Stock 150,000; web PHP/SQL Server suite 200,000 per pack [d27522066]; Odoo projects up to 650,000 |
| **Hardware + software packs** | 24,500–170,000 | Asalix from 24,500; Idipos 85,000; WINCOM up to 120,000; Danisoft 118–133k; CLYO 120–160k; restaurant system 170,000 [d45540038] |
| **SaaS** | 11,900–120,000 per year; 7,000–9,000 per month | Fisira; LogiTools 15k; Almawarid 20k; GestiumGo / Restorium 25k HT; Symloop 35–120k; Codeshade 7–9k per month |
| **Support and add-ons** | 10,000–30,000 | Crescendo support tiers; Zedkira per user / per PC / per phone |

**Ouedkniss price statistics.** Software listings with a real price of 3,000 DA or more, n = 194:

| Measure | Value |
|---|---|
| Median | 18,000 DA |
| Minimum | 3,500 DA |
| Maximum | 500,000 DA |

---

## 6. Licensing, activation and distribution

- **Lifetime licences dominate.** Buyers dislike subscriptions. FlamePOS also sells time-limited keys (1, 3 or 5 years).
- **Activation is tied to the machine:**
  - a serial generated per PC at installation
  - licence re-issued within 3 years if the PC changes (FA_Gstock)
  - "still valid after formatting" (SMART-Com, Msk Ozone)
  - activation key required even for Crescendo Free
  - optional online check (Zedkira)
  - licence delivered on a USB key (Dolisoft)
- **Per-PC and per-user pricing** is common (Dolisoft, Zedkira, TifawtTech options).
- **Trial lengths:**

  | Length | Products |
  |---|---|
  | 7 days | AMPRO, MekaStore, Flampex, ProPOS |
  | 14 days | Almawarid, Bou Saâda |
  | 15 days | Zedkira |
  | 21 days | Smart Gestion |
  | 30 days | Kiober (Microsoft Store), IntelliX, Crescendo |

- **Demos** are usually sent by email. Factor Facile's download is password-protected ("213").
- **Installation and support:**
  - remote install over AnyDesk or TeamViewer
  - support over WhatsApp
  - "training included"
  - "we enter your products for you"
  - delivery to "58 / 69 wilayas" through Yalidine and ZR Express
- **No refunds** [d8059698].
- **Hardware shops are the main channel:** Digital Company / General IT, Asalix, Tpv Technologies, Ecopos, Expert POS, Danisoft, Clyo, Computime, OptiQ, Lamwas tech, Groupe SPV, Chrono Informatique.

---

## 7. Cracked software and piracy

- **"Windev 25 Cracké"** is sold as a full WinDev, WebDev and WinDev Mobile 25 package for 30,000 DA, in 4 identical listings [d51577181]. Search also surfaced a "WinDev 25 … cracké" listing in Blida.
  - Inference: WinDev with the HFSQL database is the most common stack for Algerian micro-editors.
- **"Logiciel Gestion Stock et Commerciale + Outils d'activation"** (Bouira) [d29117684].
- **Almawarid warns** that the "cracked version of desktop software" gets no fiscal updates, has no support, brings frequent viruses and creates legal exposure (almawarid.app/blog/logiciel-gestion-commerciale-algerie-gratuit-ou-payant/).
- **Arabic forums** (ahlamontada, 7olm) circulate "free" stock programs of unknown origin.
- **Protections competitors use:** serials tied to the PC, keys sent by email, online checks, time-limited keys.
- **Implication:** a genuinely free, signed, auto-updating product removes both the reason to pirate and the virus risk.

---

## 8. What users say

**Buyer questions and complaints (Ouedkniss comments):**

- **Hidden prices:** "prix svp". One commenter: "What's the point if no pack has a price!!!"
- **Demos:** "version démo?", "نسخة تجريبية", "15-day / 3–4-day trial?"
- **Demo won't install:** "la version demo ne s'installe pas". The seller answered "try another PC" [d8059698].
- **Refunds:** "refund if not satisfied?" Answer: no.
- **Scaling:**
  - "Does it support multi-stock?"
  - "Can I add a 2nd PC?" [d11162776]
  - "How many articles?" (answer: unlimited)
- **Several activities at once:**
  - "commercial management + repairs in one" (asked on 3 listings)
  - "spare-parts warehouse for storage only"
  - "does it handle jewellery?"
  - "packaging shop?"
  - "refrigerated truck with 20–50 products"
  - "instalment company using an old accounting setup"
- **Doubt:** "I don't think an 8,900 DA program is serious."
- **Bugs:** "erreur dans propriété ligne 381" (BBS), a typical WinDev runtime message.
- **After-sales:** "service après vente?"

**Selling points vendors push:**

- speed at the till
- works offline and on old PCs
- Arabic interface
- customer credit tracking
- lifetime licence, free updates
- remote installation and training
- WhatsApp support
- mobile app
- product-list take-over
- key remapping for people switching from another program

**Pain points vendors claim to solve:**

- paper credit notebooks
- calculation errors
- employee theft (audit logs, PINs, "the seller doesn't see everything")
- stock discrepancies
- expired goods
- lost orders
- not knowing who owes what
- data loss
- internet cuts
- expensive hardware

---

## 9. Master feature catalogue: everything found, combined

Vendors that offer each feature are in brackets.

### 9.1 Till (front office)

**Finding and selling items**
- Sell by barcode, name, reference, image or favourites grid [SAFE POS, WR manager, KERS].
- **Several barcodes per product** [SAFE POS, KERS, ShopLine].
- Touch mode and **keyboard mode with reprogrammable shortcuts** [DS RAPIDE, PME PRO].
- **Hold tickets, several open tickets or customers at once, merge tickets** [DS, KERS, Tajir].

**Prices**
- **Price levels** (retail, semi-wholesale, wholesale, reseller) [PME PRO 6; SAFE POS 3; Tajir 4; Adisoft 5; COMMSoft 13].
- Price by customer category and by quantity [GestiumPRO, BBS, Almawarid].
- **Free price at the till, sell with no stock, negative stock** [DS, WR manager].

**Discounts, promotions and loyalty**
- Discounts per line or ticket, promotions, **packs and bundles**, **crossed-out promo prices** [KERS, TifawtTech, LogiTools, Zedkira].
- **Loyalty points and cards (including RFID), gift vouchers, privilege, consumption and store-credit cards** [TifawtTech, KERS, Crescendo, Asalix, INNOVA].

**Returns and control**
- **Returns and cancellations that need permission** [DS, KERS, ATS POS].
- Controls on sensitive actions and a history per user [ATS POS, Dukan, SOFTEL].

**Payments and credit**
- **Payment modes:** cash, cheque, transfer, on account, **CIB/Edahabia, BaridiMob**, card terminal, customer credit, **mixed payment** [AlgoStock, KERS, PME PRO, Onyx].
- **Credit sale with ceiling alert and automatic blocking** [KERS, Zedkira, BBS, FoodMax, Orbis].
- **Advance payments (acomptes)** [DS, Adisoft].

**Weight, packaging and variants**
- **Weighed sales:** reads the scale, **decodes barcodes printed by scales**, decimal quantities [KERS, ATS POS, Onyx, DS, Almawarid, GDS PRO].
- **Carton / pack / unit** with dynamic price [DS, KERS, ATS POS, Klick].
- **Variants (size, colour)** [Onyx, Crescendo Fashion, KERS, INNOVA].
- **Recipes and composite items** [KERS, BBS, Crescendo].

**Printing and peripherals**
- Receipts on 58 or 80 mm paper; A4/A5 from the till.
- Customer display, drawer, **kitchen printer**, card terminal [PME PRO, SmartCom, Onyx, KERS].
- **WhatsApp or email receipt** [ECHO, Almawarid].

**Cash sessions and multi-till**
- **Cash session:** opening float, open/close, **variance**, cash counting, **daily report / Z**, safe [DS, ATS POS, Almawarid, SAFE POS, PRO SOLUTION].
- **Several tills with transfers** [TifawtTech, ShopLine, Almawarid].
- **Tills that work offline when the server is down, then sync** [TifawtTech, ATS POS, DELTA].

**Other till features**
- **Flexy module** [KERS FLEXY, TifawtTech, Easy Life, FASTransfer].
- **Repair tickets** [AlgoStock, Dukan, Zedkira, Flampex].
- **SMS debt reminders; SMS when a wanted product is back in stock** [Superette-Grossiste].
- **CCTV camera support** [Superette-Grossiste].

### 9.2 Articles and stock

**Product data**
- Unlimited products; families and sub-families, brands, units, storage locations, photos; automatic coding [ITSstock, SOFTEL, BBS].
- Product names in both French and Arabic [SMART-Com, StockCom].
- Services (non-stock items) [BBS, Zedkira].

**Warehouses and stock types**
- **Multi-depot with transfers** [most].
- **Real vs. virtual stock; billable vs. non-billable stock** [Smart Gestion, WINSTO].

**Lots, expiry and serials**
- **Lots, expiry dates, alerts, expired or damaged goods** [Onyx, KERS, ATS POS, Almawarid, ProPOS, LogiTools].
- **Serial numbers and warranty** [COMMSoft, FA_Gstock, SOFTEL].

**Alerts and reordering**
- Minimum-stock alerts, automatic reorder, sell-through advice [AlgoStock, Tajir, BBS].

**Inventory and valuation**
- **Inventory:** full or partial, from mobile or PDA; stock balance between two dates [ITSstock, TifawtTech, Onyx, SmartCom].
- **Valuation:** weighted average (PMP/CUMP), FIFO, LIFO, last price [COMMSoft, BBS, Devoro, Almawarid].

**Movements and analysis**
- Movements with reasons; waste, damage and theft log [ITSstock, LogiTools, KERS].
- Dead stock, slow movers, rotation [Zsoft, KERS, ATS POS].

**Labels, data transfer, packaging and production**
- **EAN-8/EAN-13 generation, labels** (A4 sheets, 40×30 and 50×30 rolls), **price tickets** [DELTA, FA_Gstock, KERS, Almawarid, PROMOOO].
- **Excel/CSV import and export** [DS, ITSstock, Almawarid].
- **Returnable packaging (consignes)** [COMMSoft].
- **Production, assembly, bills of materials** [COMMSoft, BBS, PROMOOO, Dolisoft, PME PRO].
- Product-photo background removal [KERS].

### 9.3 Purchasing and suppliers

- **Document chain:** request → order → goods receipt → purchase invoice → credit note or return [BIG, Smart Gestion, TifawtTech, WINSTO].
- **Supplier balances, payments, overdue debts** [SAFE POS, KERS, Almawarid].
- **Supplier invoice updates prices automatically** [Adisoft, COMMSoft].
- **Imports:** landed costs, multiple currencies, exchange gains/losses [COMMSoft, ProPOS].
- **AI OCR of supplier invoices** [GestiumPRO, LogiTools, AlgoStock, Factor Facile, Fisira].

### 9.4 Sales documents

- **Document types:** quote, proforma, customer order, delivery note, "bon pour", invoice, credit note (goods or financial), supplementary invoice, return, stock entry/exit slips, **bon de route** [COMMSoft, KERS, ShopLine, Stock Standard].
- **Document flow:** convert one document into the next; **group several delivery notes into one invoice** [StockCom, BBS, Adisoft].
- **"Real / fictive / proforma" invoices; "accounted / non-accounted" invoices** [Zsoft, Smart Gestion, PRO SOLUTION].
- **Formats:** A4, A5 or 80 mm; custom templates; documents in French or Arabic [Adisoft, StockCom, Planète].
- **Legal mentions and tax:** NIF, NIS, RC, AI; VAT 19/9/exempt; HT/TTC; **stamp duty**; withholding tax [KERS, Fisira, Costy, eFawtara].
- **QR code on invoices, numbering that can't be altered, digital stamp or e-signature** [eFawtara, Fisira, Almawarid].
- **Send by email, PDF or WhatsApp** [Quick Fact, AMPRO, Almawarid].

### 9.5 Customers, CRM and sales force

- **Customer records:** fiscal IDs, category, region/wilaya/commune, delivery and billing addresses, default tariff [BBS, SOFTEL, PME PRO].
- **Credit and collections:**
  - credit ceiling, automatic blocking, **aging analysis**, reminders and follow-ups [BBS, COMMSoft, Almawarid]
  - partial or full payments, receipts, debt tree, **show a debtor their full purchase history** [KERS, Tajir, Superette-Grossiste]
  - **SMS to debtors**
- **Portals:** customer portal; reseller portal [SouqDzayer].
- **Sales force:** reps with commissions, delivery tours, offline mobile pre-sale, GPS [PME PRO, COMMSoft, Orbis, PMO Pro, Tawsil, Devoro, Androway].

### 9.6 Treasury

- **Accounts:** several till and bank accounts, transfers between them, cheques and drafts, **bank reconciliation** [SmartCom, SOFTEL, Quick Fact, PRO SOLUTION].
- **Income and expenses:** charges, losses, **investments with depreciation**, net profit after charges [TifawtTech, SAFE POS, KERS].
- **Accounting:** export to the accountant's software; SCF [PME PRO, COMMSoft, Zedkira, BIG].

### 9.7 Reports and fiscal statements

- **Sales:** turnover by hour, day, week, month or year; peak hours; top products, customers, cashiers and families; average basket; daily target [easykes, KERS, AMPRO].
- **Margins and stock:** margins, net profit, ROI, stock value, rotation, 4-year comparisons, VAT collected vs. paid [COMMSoft, KERS].
- **Report counts:** 45+ (ProPOS), 20+ (AMPRO), 13 plus 3 dashboards (FA_Gstock).
- **Algerian fiscal statements:** **état 104, G50, Zakat, "état 204"** [Dolisoft, El Eulma, Fisira, Digitaliac].
- **Journals:** sales and purchase journals, activity log, audit trail [TifawtTech, Zedkira, SOFTEL].
- **Export** to Excel, Word, PDF, CSV or HTML [Quick Fact, FA_Gstock, COMMSoft, KERS].
- **AI:** summaries, natural-language questions, forecasts, anomaly detection [KERS, AlgoStock, LogiTools].

### 9.8 Administration and data

**Users and permissions**
- **Roles:** admin, manager, cashier, technician, warehouse, employee; fine-grained rights (160+ in Restorium); PIN per worker; copy a profile [AlgoStock, Planète, Dukan].
- **Audit log** of every action [Dukan, SOFTEL, ATS POS, eFawtara].

**Companies**
- **Several company files, companies or trade registers (RC)** [Quick Fact, TifawtTech, Almawarid, SAMA].

**Backup and data safety**
- **Backup:** automatic on close, manual or scheduled; to USB, network, **Google Drive** or cloud; restore; database repair [SMART-Com, FA_Gstock, AlgoStock, Dipposoft, PME PRO, Msk Ozone].
- **Data survives reinstall; encryption** [Smart Barcode, Factor Facile].

**Licence and deployment**
- Activation survives formatting; licence moves to a new PC.
- **Single PC → LAN** with a server and client PCs.

**Interface and support**
- **Interface:** Arabic (right-to-left), French, English, switchable without restart; light/dark and colour themes; dense mode [AlgoStock, DS, WR manager, CSI-Stock, FlamePOS].
- **Support:** remote support and online updates.

### 9.9 Mobile, cloud and integrations

- **Owner mobile app** with live sales, cash, stock and alerts [AMPRO, MekaStore, AlgoStock, Zedkira, Dipposoft, Superette-Grossiste].
- **Mobile or PDA inventory and pre-sale** [TifawtTech, Almawarid, Orbis].
- **E-commerce and website sync; Yalidine delivery** [Shoperra, SAMA, MekaStore E-Com+, Digitaliac Odoo, Devoro].
- **Restaurant online services:** online ordering, QR menus, reservations, caller ID, self-order kiosk [Draria, Menu Max, El Eulma, Oran].
- **SATIM card payments** [eFawtara, Draria].
- **API** [Symloop].

### 9.10 Restaurant

- **Tables and floor plan:** split or transfer bills, unlimited tables.
- **Menus:** options, add-ons, cooking modes; pizza composition.
- **Kitchen:** a printer per station; order display on screen or TV; queue display.
- **Order taking:** waiter phone or tablet, touch remote, self-order kiosk, QR per table.
- **Service modes:** eat-in, takeaway, delivery.
- **Recipes and pricing:** ingredient deduction; time-based pricing.

### 9.11 HR (broader suites)

- **Staff:** worker commission %, advances, leave [ECHO, El Eulma, PME PRO].
- **Time and pay:** time clock; CNAS/IRG payroll [Almawarid, Winalco, Megasoft, Devoro].

---

## 10. Hardware and prices (Ouedkniss, September 2026)

| Item | Price (DA) | Examples |
|---|---|---|
| 80 mm receipt printer | 13,000–23,500 | Xprinter XP-D200N 13,500–14,900; XP-S260M 14,800–16,500; XP-Q80K (Wi-Fi/USB/LAN) 19,500; Poslux Z822 13,000 |
| Barcode label printer | 17,500–35,000 | Xprinter XPDT427B 17,500; XP-350B / 370B / 410B / 233B / 235I |
| Cash drawer | 7,500–15,000 | RJ11/RJ12 models (Smart POS, Safe POS, Riva) |
| Handheld scanner | 6,000–7,500 | Smart SP1; Henex HC-3206R BT; Honeywell MS5145 |
| Barcode-printing scale | 61,000–89,900 | DIGI 30 kg 61,000; Rongta RLS1000/1100 89,900; Aclas LS2X |
| CHIFA card reader | 19,500 | Thales |
| Touch POS pack (J1900 / Celeron, 15") | about 85,000–95,000 | – |

**Common brands:** Smart POS / Safe POS, Xprinter, Rongta, Aclas, DIGI, Honeywell.

**What the software must support:**

- ESC/POS printers
- RJ11 drawer kick
- EAN-13 barcodes with embedded weight or price from scales
- USB HID scanners

---

## 11. Regulatory points every Algerian POS must handle

(Brief. Detailed regulatory research belongs in a separate report.)

- **Certification from 1 January 2027** (Loi de finances 2026; Art. 51 bis, Code des taxes sur le chiffre d'affaires):
  - **Who:** VAT-registered B2C sellers using any digital system.
  - **Four principles:** inalterability, security, conservation, archiving with an audit trail.
  - **Proof:** either a dated, **version-specific vendor commitment** or a third-party certificate, shown during tax audits.
  - **Penalty example:** a 25,000 DA fine plus possible reassessment of revenue.
  - **Source:** cirtasoft.com certification article.
- **Mandatory invoice mentions** (Commercial Code art. 7, CID art. 224 as cited by Almawarid, Costy and Factury):
  - seller name, address, **NIF, RC, NIS, AI**
  - invoice number and date
  - HT, VAT, TTC, payment method
  - Corrections must go through a **credit note** that references the original invoice.
- **VAT:** 19 %, 9 %, exemptions.
- **Stamp duty on cash payments.** Costy's tiers:
  - ≤ 30,000 DA: about 1 %
  - 30,001–100,000 DA: about 1.5 %
  - > 100,000 DA: about 2 %
  - minimum 5 DA; electronic or bank payments are exempt
  - **Check these against the current tax code before building.**
- **Other expectations buyers have:**
  - **D.E. 05-468** (the invoicing decree, cited by Zedkira)
  - **État 104, G50, Zakat**
  - a Z report "conforme DGI" (Almawarid)
  - QR code and e-signature (Fisira's claim, unverified)

---

## 12. Gaps and opportunities for the free DZBuild POS

1. **No product is free, complete, offline, in Arabic right-to-left and multi-PC at once.** Crescendo Free is single-PC, French-only and needs an activation key. Almawarid's free plan is cloud-only and capped at 3 invoices a month. Klick's free edition is undocumented.
   - DZBuild can own "free forever, all features, no activation key".
2. **Close the trust gap.**
   - Offer an instant, signed installer with no activation.
   - Publish changelogs and auto-update.
   - Publish a 2027 fiscal-conformity statement.
3. **Make switching easy.** Competitors win switchers by entering your product list for you and remapping keys. DZBuild should offer:
   - Excel/CSV import wizards for products, customers with opening debts, and suppliers
   - later, importers from competitor databases (WinDev/HFSQL .fic files, Access, Sage/EBP exports); feasibility to verify
   - keyboard-layout presets
4. **Combine the Algerian specifics no one else has in one product:**
   - credit with ceilings and SMS/WhatsApp reminders
   - price levels and carton/pack/unit
   - scale barcodes
   - flexy, repairs and instalments
   - état 104, G50 and Zakat helpers
   - stamp duty and NIF/NIS/RC/AI
   - bon de route
   - returnable packaging
   - serials and warranty
   - lots and expiry
   - multi-depot

   Consider **not** offering "fictive invoices": the practice is widespread but legally risky.
5. **Make offline multi-till with server failover standard.** It costs 70,000 DA at TifawtTech.
6. **Use DZBuild OAuth to link a mobile owner app and cloud features:** remote dashboard, cloud backup, multi-shop consolidation, and sync with the DZBuild store and e-commerce. Competitors charge extra for this.
7. **Put WhatsApp everywhere:** receipts, invoice PDFs, debt reminders, supplier purchase orders.
8. **Keep hardware requirements very low.** Target Windows 7/10/11, check whether 32-bit is needed, and keep the installer small.
9. **Include restaurant mode in the same app.** Many shops are hybrid "superette + pizzeria".
10. **Build for the 2027 rules from day one:**
    - an append-only, hash-chained sales journal
    - archives and an audit trail
    - Z reports
    - numbering that can't be altered
    - corrections only by credit note
    - an exportable audit file
    - a conformity commitment published for each version

---

## 13. Sources

**Editors, vendors and guides**

- **Onyx:** https://www.solutionsinformatiques.dz/Logiciel-de-gestion-de-points-de-vente-POS-algerie?lang=fr ; https://www.solutionsinformatiques.dz/Inventory-systems
- **INNOVA:** https://www.innova-dz.com/ ; https://www.innova-dz.com/Solutions-point-de-vente.html
- **Cirtasoft:** https://cirtasoft.com/gestiumpro/ ; https://www.cirtasoft.com/logiciel-restaurant/en/ ; https://www.cirtasoft.com/certification-des-logiciels-de-caisse-en-algerie-contexte-et-obligations-a-partir-de-2027
- **Megatheque:** https://megatheque.net/logiciel-point-de-vente-algerie/ ; https://megatheque.net/logiciel-gestion-stock-commerciale-algerie/
- **Crescendo:** http://www.crescendo-software.com/ ; https://www.crescendo-software.com/produits/ ; https://www.crescendo-software.com/foire-aux-questions-faq/ ; http://www.crescendo-software.com/logiciel-pour-superette-algerie/
- **Established editors:**
  - Inabex: https://inabex.com/logiciels-de-gestion-en-algerie/
  - Megasoft: https://www.megasoft-office.com/
  - Winalco: https://winalco.dz/
  - Devoro: https://devoro-erp.com/
  - Smart Partner: https://www.smart-dz.com/produits-services/smartcom ; http://www.smart-dz.com/
  - NTIC-SOFTPRO: https://ntic-softpro.com/s_g.html
  - IntelliX: https://www.intellixgroup.com/
  - Elmoumtez: https://www.elmoumtez.com/
  - BIG Informatique: https://biginformatique.com/produits/achats-stock
  - DSSI: https://www.d2si.eu/solutions/gestion_des_points_de_vente
  - Adisoft: https://www.adisoft-dz.com/logiciel-gestion-commerciale/
  - Tayssir: https://tayssir-erp.dz/tayssir-erp-logiciel-de-gestion-commercial-stock-crm-rh/tayssir-erp-logiciel-gestion-stocks-algerie/
- **SME desktop products:**
  - Dolisoft: https://dolisoft-dz.com/products/dolisoft-pro-logiciel-de-gestion-de-stock-special-algerie-2025
  - AlgoStock: https://algosynapse.com/algostock/
  - Zedkira: https://inventory.zedkira.com/
  - Dif Softs: https://www.difsofts.com/logiciel-gestion-de-stock-gestion-commerciale-algerie/ ; https://www.difsofts.com/
  - Factor Facile: https://devion.cloud/factorfacile
  - Klick: https://www.klick.app/
  - Computime: https://computime.store/
  - Eurequat: https://www.eurequat-algerie.com/point-de-vente-pos/
- **Cloud / SaaS:**
  - LogiTools: https://www.logitools-dz.com/
  - SouqDzayer: https://souqdzayer.com/fr/logiciel-gestion-commerciale-algerie
  - Codeshade: https://codeshade.app/
  - FlustockX: https://flustockx.com/blog/logiciel-de-gestion-de-stock-algerie/
  - Mahaal / Inyad: https://www.mahaal.app/ar ; https://inyad.com/ar/pos-system-software
  - Symloop: https://symloop.com/fr/blog/logiciel-caisse-pos-restaurant-commerce-setif-2026/
- **Almawarid:**
  - https://almawarid.app/logiciel-gestion-commerciale-algerie/
  - https://almawarid.app/logiciel-gestion-stock-algerie/
  - https://almawarid.app/blog/logiciel-caisse-pos-algerie-point-de-vente/
  - https://almawarid.app/blog/logiciel-gestion-commerciale-algerie-gratuit-ou-payant/
  - https://almawarid.app/tarifs
  - https://almawarid.app/blog/nif-nis-rc-article-imposition-algerie/
- **Invoicing and compliance:**
  - Fisira: https://www.fisira.app/
  - eFawtara: https://efawtara.com/fr/facturation
  - Costy: https://costy.app/facturation-en-algerie-en-2025-guide-complet/
  - ConformePro: https://conformepro.dz/resources/guide-rc-nif
- **Foreign partners:**
  - BSC Algérie: https://www.bsc-algerie.com/
  - Odoo: https://ithrytec.com/odoo-algerie ; https://www.digitaliac.com/integration-odoo ; https://odoo-algerie.com/ ; https://digisys.dz/odoo-algerie/ ; https://www.odoo.com/partners/grade/ready-3/country/algeria-61
  - Sage Maroc reference: https://www.sage.com/fr-ma/produits/sage-100/gestion-commerciale/
- **Context:**
  - Lamacta guide: https://lamacta.com/blog/logiciel-caisse-algerie-commerce/
  - WEBMINDS (foreign-card limits, WhatsApp, Arabic): https://webminds.dz/blog/crm-algerie-2026.html
- **Not readable:**
  - https://www.facebook.com/logicielcaisse/
  - https://www.facebook.com/logiciel.proresto/
  - https://www.youtube.com/watch?v=_XYTRhBah_s (GFS)
  - https://www.youtube.com/watch?v=5BRaAXm4uIg (CASH)

**Ouedkniss:** data came from the public GraphQL API on 2026-09-28. Listing links are defined below.

---

## Appendix A: Ouedkniss software listings, top 100 by cumulative views (2026-09-28)

Filtered to POS, stock, commercial-management and restaurant software. Excluded: jobs, training, hotels, schools, gyms, clinics, flexy-only tools, custom development. "n/a" means a symbolic or hidden price.

| # | Views | Title (as listed) | Price DA | Commune/Wilaya | Store | Link |
|---|---|---|---|---|---|---|
| 1 | 39497 | Logiciel gestions Commercial PME PRO | 70,000 | Dar el beida/Alger | TifawtTech | [d12505797][ok12505797] |
| 2 | 23838 | LOGICIEL SUPERETTE JOUET VETEMENT | n/a | Bab ezzouar/Alger | PRO SOLUTION | [d11162910][ok11162910] |
| 3 | 23194 | برنامج تسيير الفواتير باللغة العربية | 5,000 | El eulma/Sétif |  | [d6347367][ok6347367] |
| 4 | 22137 | Logiciel PME PMI STOCK & FACTURATION | 30,000 | Bab ezzouar/Alger | PRO SOLUTION | [d11162776][ok11162776] |
| 5 | 21331 | Logiciel point de vente Gestion de Stock | 35,000 | Alger centre/Alger |  | [d20833261][ok20833261] |
| 6 | 21320 | LOGICIEL SUPÉRETTE ET BOUTIQUE | 35,000 | Sidi bel abbes |  | [d5058901][ok5058901] |
| 7 | 19221 | Logiciel Superette | n/a | Guelma |  | [d9010110][ok9010110] |
| 8 | 18535 | برنامج تسيير المحلات التجارية | n/a | Tizi nbechar/Sétif |  | [d13827110][ok13827110] |
| 9 | 18508 | Logiciel de gestion d'entreprise Odoo | n/a | Cheraga/Alger |  | [d20319224][ok20319224] |
| 10 | 18073 | برنامج WR manager لتسيير المحلات التجارية | 19,999 | Ain oussara/Djelfa |  | [d34059266][ok34059266] |
| 11 | 17885 | Logiciel Superette + camera + redeau | n/a | El eulma/Sétif |  | [d9093894][ok9093894] |
| 12 | 17327 | logiciel Caisse pour supperette | n/a | Souk ahras |  | [d9350793][ok9350793] |
| 13 | 16936 | LOGICIEL DU STOCK ET CAISSE (WINCOM) | 85,000 | Chlef |  | [d15298462][ok15298462] |
| 14 | 16602 | LOGICIEL DE CAISSE | 25,000 | Sidi bel abbes |  | [d5967052][ok5967052] |
| 15 | 16263 | LOGICIEL POINT DE VENTE ET GESTION COM | 35,000 | Akbou/Béjaïa | Its2010 | [d6894011][ok6894011] |
| 16 | 16158 | Gestion Commerciale | n/a | Bordj Bou Arreridj |  | [d8364510][ok8364510] |
| 17 | 16153 | LOGICIEL FASTSHOP | n/a | Kouba/Alger | Tpv Technologies | [d11018311][ok11018311] |
| 18 | 15797 | Quick Fact Gestion Commerciale, Stock & Facturation | 25,000 | Alger centre | PC LINE | [d16488297][ok16488297] |
| 19 | 15302 | Logiciel GESTION D'INVENTAIRE Pro Immo | n/a | Dar el beida | TifawtTech | [d13258442][ok13258442] |
| 20 | 15037 | Logiciel Superette | n/a | Annaba |  | [d9470491][ok9470491] |
| 21 | 14941 | BBS Gestion Commerciale | 35,000 | Oran |  | [d10912645][ok10912645] |
| 22 | 14796 | Logiciels De Gestion supérette supermarché cosmétiques | n/a | Kouba/Alger |  | [d21540163][ok21540163] |
| 23 | 14500 | Logiciel pour supérette | n/a | Sétif |  | [d19564294][ok19564294] |
| 24 | 14442 | gestion stock et commerciale | 8,000 | Chlef |  | [d18994540][ok18994540] |
| 25 | 13774 | Logiciel et Caisse tactile | 120,000 | Kouba/Alger | CLYO ALGERIE | [d6203275][ok6203275] |
| 26 | 13479 | برنامج تسيير تجاري بالعربية | n/a | Tizi nbechar/Sétif |  | [d16407861][ok16407861] |
| 27 | 13391 | Logiciel gest supérette avec des pack | n/a | Dar el beida | TifawtTech | [d21755610][ok21755610] |
| 28 | 13372 | Logiciel et caisse tactile pour magasin | n/a | Blida |  | [d20764280][ok20764280] |
| 29 | 13295 | برنامج تسسير المخزون بالعربية | 120,000 | El matmar/Relizane |  | [d12051137][ok12051137] |
| 30 | 13062 | Logiciel Superette / Restaurant | n/a | Alger centre |  | [d20764626][ok20764626] |
| 31 | 13025 | Logiciel superette تصميم كل البرامج | 1,000 | Alger centre |  | [d13765224][ok13765224] |
| 32 | 13005 | easykes - Logiciel pour superette | 25,000 | Oran |  | [d12993513][ok12993513] |
| 33 | 12982 | GESTION COMMERCIALE POINT DE VENTE | n/a | Tizi nbechar/Sétif |  | [d13826797][ok13826797] |
| 34 | 12926 | Logiciel Superette | n/a | Skikda |  | [d9473957][ok9473957] |
| 35 | 12923 | Logiciel Superette - Grossiste | 9,900 | Oran |  | [d18778898][ok18778898] |
| 36 | 12743 | logiciel gestion restaurant fast food | n/a | Alger centre |  | [d4732486][ok4732486] |
| 37 | 12615 | logiciel de point de vente | n/a | Tizi nbechar/Sétif |  | [d16407886][ok16407886] |
| 38 | 12583 | Promotion Logiciel Superette/Restaurant | n/a | Blida |  | [d19762488][ok19762488] |
| 39 | 12431 | logiciel Gestion de stock لوجيسيال السوبيرات | n/a | Bouira |  | [d22429721][ok22429721] |
| 40 | 12339 | Logiciel Restauration et cafeteria | n/a | Dar el beida | TifawtTech | [d19254138][ok19254138] |
| 41 | 12304 | Logiciel de gestion RESTO-PRO | n/a | Bab ezzouar | PRO SOLUTION | [d18723112][ok18723112] |
| 42 | 12278 | LOGICIEL DE GESTION DE STOCK | n/a | Blida | Expert Pos | [d12373295][ok12373295] |
| 43 | 12144 | logiciel point de vent et materiel | n/a | Bab ezzouar |  | [d8853250][ok8853250] |
| 44 | 12087 | Logiciel PME PRO + APPLICATION DE DISTRIBUTION | n/a | Bab ezzouar | PRO SOLUTION | [d23967633][ok23967633] |
| 45 | 11973 | LOGICIEL DE GESTION DE STOCK / CAISSE | 35,000 | Dar el beida |  | [d22267608][ok22267608] |
| 46 | 11965 | Logiciel de gestion commercial | n/a | Ain charchar/Skikda |  | [d21591248][ok21591248] |
| 47 | 11780 | Logiciel de supérette / boutique | 14,000 | Mostaganem |  | [d21429035][ok21429035] |
| 48 | 11778 | Logiciel de gestion d'entreprise | n/a | Alger centre |  | [d23748056][ok23748056] |
| 49 | 11738 | LOGICIEL GESTION COMMERCIALE ITSCOM | 28,000 | Akbou/Béjaïa | Its2010 | [d17834397][ok17834397] |
| 50 | 11726 | logiciel gestion commerciale superette, resto et pizzeria | n/a | Ain naadja |  | [d7624187][ok7624187] |
| 51 | 11341 | برنامج التسيير التجاري | n/a | Sétif |  | [d19564652][ok19564652] |
| 52 | 11244 | logiciels superette | n/a | Alger centre |  | [d3110145][ok3110145] |
| 53 | 11213 | Logiciel Gestion Stock et Commerciale (Version Pro) | n/a | Bouira |  | [d19752913][ok19752913] |
| 54 | 11088 | Logiciel de gestion commerciale PVente | 15,000 | Sig/Mascara |  | [d19338248][ok19338248] |
| 55 | 11042 | Meilleur Logiciel Gestion Commerciale | 12,000 | Adrar |  | [d25191915][ok25191915] |
| 56 | 10823 | GESTION COMMERCIALE | 25,000 | El ouricia/Sétif |  | [d22519331][ok22519331] |
| 57 | 10738 | Logiciel De Gestion CaisseKom Pour Mini supérette | 25,000 | Oran | computime | [d14435454][ok14435454] |
| 58 | 10634 | Tajir - برنامج تسيير محل تجاري | 12,000 | Oran |  | [d22296334][ok22296334] |
| 59 | 10346 | Pack logiciels et matériel de commerce | n/a | Batna |  | [d16827647][ok16827647] |
| 60 | 10206 | برنامج تسيير محلات ملابس | n/a | Oum El Bouaghi |  | [d20770512][ok20770512] |
| 61 | 9868 | Logiciel restaurant | n/a | Alger centre |  | [d20374297][ok20374297] |
| 62 | 9852 | Logiciel gestion restaurant | 50,000 | Béjaïa |  | [d22011635][ok22011635] |
| 63 | 9673 | Logiciel de gestion de stock + Caisse | n/a | Constantine |  | [d13541790][ok13541790] |
| 64 | 8200 | Logiciel StockCom Gestion Commercial | 20,000 | Biskra |  | [d23906352][ok23906352] |
| 65 | 8169 | GESTION COMMERCIALE | 25,000 | El ouricia/Sétif |  | [d22881363][ok22881363] |
| 66 | 8143 | LOGICIEL DE GESTION STOCK-VENTES-ACHAT | 10,000 | Amoucha/Sétif |  | [d20127285][ok20127285] |
| 67 | 8055 | Logiciel de gestion commercial SAFE POS | n/a | Bouzareah | VAST SOFT | [d24881513][ok24881513] |
| 68 | 8034 | Logiciel de restaurant | n/a | Sétif | LOG103 informatique | [d21183383][ok21183383] |
| 69 | 7993 | Logiciel de gestion de caisse ultra si | 1,000 | Alger centre |  | [d23846124][ok23846124] |
| 70 | 7921 | WINSTO Gestion de stock | n/a | Sétif | LOG103 informatique | [d21173889][ok21173889] |
| 71 | 7885 | logiciel et matériels de la caisse | 35,000 | Bouira |  | [d8897087][ok8897087] |
| 72 | 7760 | PLANÈTE, LOGICIELS DE GESTION | n/a | Alger centre |  | [d22047224][ok22047224] |
| 73 | 7544 | e-Shop, Gestion commerciale pour magasin | n/a | Sétif |  | [d11536954][ok11536954] |
| 74 | 7526 | برنامج تسيير محل تجاري | 17,000 | Merouana/Batna |  | [d17762051][ok17762051] |
| 75 | 7413 | Logiciel de gestion commerciale simple, fiable et efficace (GestCom) | n/a | Mohammadia | SOFTEL SOLUTIONS | [d52503089][ok52503089] |
| 76 | 7152 | Logiciel gestion commercial SMART-Com | 25,000 | Chlef |  | [d22499274][ok22499274] |
| 77 | 7097 | برنامج تسيير المحلات | 30,000 | Annaba |  | [d24116972][ok24116972] |
| 78 | 6915 | Logiciel supérette - restaurant | 14,000 | Tlemcen |  | [d23568445][ok23568445] |
| 79 | 6902 | Logiciel caisse supérette magasin gros | 40,000 | Douera |  | [d24322509][ok24322509] |
| 80 | 6854 | GestaCaisse | n/a | Bouzareah |  | [d18252240][ok18252240] |
| 81 | 6826 | FoodMax Logiciel Restaurant Pizzeria | 18,000 | Oran |  | [d24009309][ok24009309] |
| 82 | 6763 | Logiciel de caisse pour restaurant | 15,000 | Chlef |  | [d24097980][ok24097980] |
| 83 | 6715 | logiciel de facturation Algérie | 25,000 | El mouradia |  | [d25214517][ok25214517] |
| 84 | 6667 | برنامج تسيير المحلات التجارية والمخازن | n/a | Guidjel/Sétif |  | [d25860750][ok25860750] |
| 85 | 6482 | برنامج تسيير المبيعات والمخازن | 18,000 | Tizi nbechar/Sétif |  | [d25233066][ok25233066] |
| 86 | 6377 | Logiciel de Gestion Commercial & Stock | 18,000 | Béjaïa |  | [d25879075][ok25879075] |
| 87 | 6368 | Logiciel commercial | 18,000 | Chlef |  | [d26087778][ok26087778] |
| 88 | 6173 | Logiciel gestion de stock | n/a | Birkhadem |  | [d25041060][ok25041060] |
| 89 | 5883 | LOGICIEL DE GESTION DU STOCK | 15,000 | Tébessa |  | [d25787532][ok25787532] |
| 90 | 5870 | برنامج تسيير المحلات التجارية | n/a | Bordj Bou Arreridj |  | [d25238323][ok25238323] |
| 91 | 5869 | LOGICIEL DE CAISSE ET DE VENTE | n/a | Béjaïa |  | [d26104056][ok26104056] |
| 92 | 5842 | logiciel des vêtements | n/a | Alger centre |  | [d24133059][ok24133059] |
| 93 | 5825 | Smart Gestion V 2.0 | 35,000 | Béjaïa |  | [d24550112][ok24550112] |
| 94 | 5776 | LOGICIEL RESTAURANT CAFETERIA | 18,000 | Bordj Bou Arreridj |  | [d21621288][ok21621288] |
| 95 | 5747 | برنامج مبيعات / Logiciel Gestion Stock | n/a | Ghardaïa |  | [d26327909][ok26327909] |
| 96 | 5663 | Logiciel de gestion point de vente | 25,000 | El Oued |  | [d25676170][ok25676170] |
| 97 | 5654 | Logiciel Restaurant "RestoLine" | 18,000 | Chlef |  | [d25918343][ok25918343] |
| 98 | 5588 | Logiciel de gestion de point de vente | 30,000 | Sidi maarouf/Jijel |  | [d25633863][ok25633863] |
| 99 | 5519 | Logiciel de gestion commercial | n/a | Bir el djir/Oran |  | [d25553128][ok25553128] |
| 100 | 5412 | Logiciel de facturation Algérien | n/a | El mouradia |  | [d25839906][ok25839906] |

---

## Link definitions (Ouedkniss listings)

[d22499274]: https://www.ouedkniss.com/chlef-bechar-saida-sidi-bel-abbes-mostaganem-mascara-oran-el-bayadh-naama-algerie-applications-logiciels-logiciel-gestion-commercial-smart-com-d22499274
[ok22499274]: https://www.ouedkniss.com/chlef-bechar-saida-sidi-bel-abbes-mostaganem-mascara-oran-el-bayadh-naama-algerie-applications-logiciels-logiciel-gestion-commercial-smart-com-d22499274
[d22047875]: https://www.ouedkniss.com/applications-logiciels-planete-points-de-vente-alger-centre-algerie-d22047875
[d10912645]: https://www.ouedkniss.com/applications-logiciels-bbs-gestion-commerciale-oran-algerie-d10912645
[ok10912645]: https://www.ouedkniss.com/applications-logiciels-bbs-gestion-commerciale-oran-algerie-d10912645
[d51171823]: https://www.ouedkniss.com/logiciels-abonnements-sama-vision-erp-logiciel-complet-pour-la-gestion-des-entreprises-et-points-de-vente-khraissia-alger-algerie-d51171823
[d12505797]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestions-commercial-pme-pro-dar-el-beida-alger-algerie-d12505797
[ok12505797]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestions-commercial-pme-pro-dar-el-beida-alger-algerie-d12505797
[d30370559]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-supermarche-dar-el-beida-alger-algerie-d30370559
[d30369909]: https://www.ouedkniss.com/applications-logiciels-logiciel-pos-light-superette-cosmetique-vente-vetement-avec-option-flexy-dar-el-beida-alger-algerie-d30369909
[d11162776]: https://www.ouedkniss.com/applications-logiciels-logiciel-pme-pmi-stock-facturation-bab-ezzouar-alger-algerie-d11162776
[ok11162776]: https://www.ouedkniss.com/applications-logiciels-logiciel-pme-pmi-stock-facturation-bab-ezzouar-alger-algerie-d11162776
[d23967633]: https://www.ouedkniss.com/applications-logiciels-logiciel-pme-pro-gestion-commerciale-et-stock-application-de-distribution-bab-ezzouar-alger-algerie-d23967633
[ok23967633]: https://www.ouedkniss.com/applications-logiciels-logiciel-pme-pro-gestion-commerciale-et-stock-application-de-distribution-bab-ezzouar-alger-algerie-d23967633
[d50572699]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-pme-pro-mohammadia-alger-algerie-d50572699
[d16488297]: https://www.ouedkniss.com/applications-logiciels-quick-fact-gestion-commerciale-stock-facturation-serveur-monoposte-alger-centre-algerie-d16488297
[ok16488297]: https://www.ouedkniss.com/applications-logiciels-quick-fact-gestion-commerciale-stock-facturation-serveur-monoposte-alger-centre-algerie-d16488297
[d17834397]: https://www.ouedkniss.com/bejaia-akbou-algerie-applications-logiciels-logiciel-gestion-commerciale-itscom-d17834397
[ok17834397]: https://www.ouedkniss.com/bejaia-akbou-algerie-applications-logiciels-logiciel-gestion-commerciale-itscom-d17834397
[d6894011]: https://www.ouedkniss.com/bejaia-akbou-algerie-applications-logiciels-logiciel-point-de-vente-et-gestion-com-d6894011
[ok6894011]: https://www.ouedkniss.com/bejaia-akbou-algerie-applications-logiciels-logiciel-point-de-vente-et-gestion-com-d6894011
[d23906352]: https://www.ouedkniss.com/biskra-algerie-applications-logiciels-logiciel-stockcom-gestion-commercial-d23906352
[ok23906352]: https://www.ouedkniss.com/biskra-algerie-applications-logiciels-logiciel-stockcom-gestion-commercial-d23906352
[d33391286]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-stock-gfs-hydra-alger-algerie-d33391286
[d30034727]: https://www.ouedkniss.com/logiciels-abonnements-gfs-gestion-commerciale-caisse%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D9%88-%D8%A7%D9%84%D8%A8%D9%8A%D8%B9-chlef-algerie-d30034727
[d24550112]: https://www.ouedkniss.com/logiciels-abonnements-smart-gestion-v-20-bejaia-algerie-d24550112
[ok24550112]: https://www.ouedkniss.com/logiciels-abonnements-smart-gestion-v-20-bejaia-algerie-d24550112
[d21173889]: https://www.ouedkniss.com/setif-algerie-applications-logiciels-winsto-gestion-de-stock-d21173889
[ok21173889]: https://www.ouedkniss.com/setif-algerie-applications-logiciels-winsto-gestion-de-stock-d21173889
[d15298462]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-du-stock-et-caisse-prix-jusqua-120000-chlef-aflou-ain-babouche-batna-bejaia-algerie-d15298462
[ok15298462]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-du-stock-et-caisse-prix-jusqua-120000-chlef-aflou-ain-babouche-batna-bejaia-algerie-d15298462
[d40249015]: https://www.ouedkniss.com/applications-logiciels-logiciel-pos-zsoft-gestion-de-ventestockcommerciale-alger-centre-algerie-d40249015
[d24881513]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-safe-pos-bouzareah-alger-algerie-d24881513
[ok24881513]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-safe-pos-bouzareah-alger-algerie-d24881513
[d39454663]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-la-distribution-mobile-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A5%D8%AF%D8%A7%D8%B1%D8%A9-%D8%A7%D9%84%D8%AA%D9%88%D8%B2%D9%8A%D8%B9-%D8%A8%D8%A7%D8%B3%D8%AA%D8%AE%D8%AF%D8%A7%D9%85-%D8%AA%D8%B7%D8%A8%D9%8A%D9%82-%D9%87%D8%A7%D8%AA%D9%81%D9%8A-bouzareah-alger-algerie-d39454663
[d26730980]: https://www.ouedkniss.com/alger-ain-benian-naadja-taya-bab-el-oued-ezzouar-baraki-birkhadem-birtouta-bourouba-dar-beida-gue-de-constantine-les-eucalyptus-smar-rahmania-rouiba-staoueli-tessala-merdja-algerie-autre-logiciel-point-vent-sup%C3%A9rette-promooo-d26730980
[d12993513]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-easykes-logiciel-pour-superette-d12993513
[ok12993513]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-easykes-logiciel-pour-superette-d12993513
[d56142529]: https://www.ouedkniss.com/logiciels-abonnements-ampro-le-logiciel-de-gestion-commerciale-caisse-parfait-application-mobile-alger-centre-algerie-d56142529
[d56573309]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-caisse-pos-multisphere-com-silver-setif-algerie-d56573309
[d56562698]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-stock-et-caisse-pos-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-mekastore-starter-bab-ezzouar-alger-algerie-d56562698
[d57221382]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-caisse-et-gestion-stock-point-vente-pc-alimentation-boucherie-telephonie-ghriss-mascara-algerie-d57221382
[d57221406]: https://www.ouedkniss.com/logiciels-abonnements-ds-rapide-logiciel-de-caisse-100-clavier-vente-gros-et-demi-ghriss-mascara-algerie-d57221406
[d57957894]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-caisse-pos-et-gestion-stock-pour-tout-commerce-tlemcen-algerie-d57957894
[d55281649]: https://www.ouedkniss.com/logiciels-abonnements-smartstock-logiciel-de-gestion-stock-professionnel-pour-windows-alger-centre-algerie-d55281649
[d27434885]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-commerciale-et-stock-complet-fa-gstock-bougara-blida-algerie-d27434885
[d21919164]: https://www.ouedkniss.com/blida-tizi-ouzou-alger-boumerdes-ain-romana-benian-algerie-applications-logiciels-stock-standard-d21919164
[d54257126]: https://www.ouedkniss.com/logiciels-abonnements-klick-stock-standard-logiciel-de-gestion-facturation-oran-algerie-d54257126
[d55740700]: https://www.ouedkniss.com/logiciels-abonnements-kio-pos-logiciel-de-caisse-et-gestion-stock-alger-centre-algerie-d55740700
[d56045476]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-kiopos-alger-centre-algerie-d56045476
[d55706998]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-omni-caisse-pro-alger-centre-algerie-d55706998
[d56301202]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-commercial-pos-dahabia-gestion-supermarche-ou-grossiste-a-vie-sans-abonnement-laghouat-algerie-d56301202
[d57517611]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-pro-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D8%AD%D8%AA%D8%B1%D8%A7%D9%81%D9%8A-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D9%83-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-bab-ezzouar-alger-algerie-d57517611
[d57312381]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D9%88%D8%A7%D9%84%D9%85%D8%A8%D9%8A%D8%B9%D8%A7%D8%AA-flampex-logiciel-de-gestion-hussein-dey-alger-algerie-d57312381
[d55784365]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D9%84%D8%A8%D9%8A%D8%B9-%D8%A8%D8%A7%D9%84%D8%AA%D8%AC%D8%B2%D8%A6%D8%A9-logiciel-du-ventesuperette-flamepos-alger-centre-algerie-d55784365
[d54074597]: https://www.ouedkniss.com/logiciels-abonnements-kers-pos-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-%D9%81%D9%84%D9%83%D8%B3%D9%8A-%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D9%86%D9%82%D8%A7%D8%B7-%D8%A8%D9%8A%D8%B9-%D8%A7%D8%AD%D8%AA%D8%B1%D8%A7%D9%81%D9%8A-souk-ahras-algerie-d54074597
[d56463136]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-flexy-reparation-vent-kers-souk-ahras-algerie-d56463136
[d34059266]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-wr-manager-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-ain-oussara-djelfa-algerie-d34059266
[ok34059266]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-wr-manager-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-ain-oussara-djelfa-algerie-d34059266
[d22296334]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-tajir-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-d22296334
[ok22296334]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-tajir-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-d22296334
[d18778898]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-logiciel-superette-grossiste-d18778898
[ok18778898]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-logiciel-superette-grossiste-d18778898
[d56826623]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-dipposoft-%D8%A7%D9%84%D8%B4%D8%A7%D9%85%D9%84-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D9%85%D8%A4%D8%B3%D8%B3%D8%A7%D8%AA-gestion-de-stock-facturation-djelfa-algerie-d56826623
[d57939003]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-inventaire-supperette-ats-pos-320-base-de-donne-configuration-annaba-algerie-d57939003
[d56299357]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-caisse-pro-kolea-tipaza-algerie-d56299357
[d56453599]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-de-stock-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D9%83%D8%B1%D9%8A%D8%AF%D9%8A-%D8%A8%D8%A7%D9%84%D8%A8%D8%A7%D8%B1%D9%83%D9%88%D8%AF-sidi-amar-annaba-algerie-d56453599
[d56207773]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-magasin-stock-facile-complet-version-2026-douera-alger-algerie-d56207773
[d55290118]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-caisse-et-gestion-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D9%83%D8%A7%D8%B4%D9%8A%D8%B1-%D9%88%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-bou-saada-msila-algerie-d55290118
[d52265413]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-hirosystem-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A3%D9%8A-%D9%86%D8%B4%D8%A7%D8%B7-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-birtouta-alger-algerie-d52265413
[d55107943]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A3%D8%B1%D9%83%D8%A7%D9%86-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D8%B3%D9%88%D8%A8%D8%B1%D9%85%D8%A7%D8%B1%D9%83%D8%AA-%D9%86%D8%B8%D8%A7%D9%85-%D9%83%D8%A7%D8%B4%D9%8A%D8%B1-%D9%88%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D8%A7%D8%AD%D8%AA%D8%B1%D8%A7%D9%81%D9%8A-sidi-amar-annaba-algerie-d55107943
[d26735464]: https://www.ouedkniss.com/alger-rouiba-algerie-applications-logiciels-logiciel-de-sup%C3%A9rette-idipos-d26735464
[d14435454]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-pour-mini-superette-oran-algerie-d14435454
[ok14435454]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-caissekom-pour-mini-superette-oran-algerie-d14435454
[d25951270]: https://www.ouedkniss.com/blida-tlemcen-alger-setif-sidi-bel-abbes-mostaganem-oran-ain-temouchent-centre-algerie-applications-logiciels-logiciel-restaurant-coffeline-d25951270
[d56061973]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-csi-stock-gestion-point-de-vente-achats-et-ventes-grossiste-detail-multi-magasin-birkhadem-alger-algerie-d56061973
[d51024226]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-shoperra-gestion-de-stock-et-clients-oran-algerie-d51024226
[d57732597]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-stock-gestock-khemis-el-khechna-boumerdes-algerie-d57732597
[d56087176]: https://www.ouedkniss.com/logiciels-abonnements-orbis-logiciel-de-gestion-pour-distributeurs-grossistes-commandes-stock-livraison-facture-cheraga-alger-algerie-d56087176
[d55690765]: https://www.ouedkniss.com/logiciels-abonnements-istock-pro-2026-kouba-alger-algerie-d55690765
[d51405412]: https://www.ouedkniss.com/ecrans-logiciel-de-vente-bzpos-original-bordj-bou-arreridj-algerie-d51405412
[d18252240]: https://www.ouedkniss.com/alger-bouzareah-algerie-applications-logiciels-gestacaisse-d18252240
[ok18252240]: https://www.ouedkniss.com/alger-bouzareah-algerie-applications-logiciels-gestacaisse-d18252240
[d11018311]: https://www.ouedkniss.com/alger-kouba-algerie-applications-logiciels-logiciel-fastshop-d11018311
[ok11018311]: https://www.ouedkniss.com/alger-kouba-algerie-applications-logiciels-logiciel-fastshop-d11018311
[d28927989]: https://www.ouedkniss.com/applications-logiciels-promotion-logiciel-gestion-commercial-blida-algerie-d28927989
[d12373295]: https://www.ouedkniss.com/autre-logiciel-de-gestion-stock-blida-algerie-d12373295
[ok12373295]: https://www.ouedkniss.com/autre-logiciel-de-gestion-stock-blida-algerie-d12373295
[d40287712]: https://www.ouedkniss.com/autre-pack-logiciel-pc-digital-imprimante-ticket-caisse-lecteur-code-a-barre-draria-alger-algerie-d40287712
[d44928828]: https://www.ouedkniss.com/all-in-one-logiciel-de-gestion-commercial-dar-el-beida-alger-algerie-d44928828
[d30782671]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commercial-pack-superette-alimentation-generale-epicerie-alger-centre-bab-ezzouar-cheraga-dely-brahim-hydra-algerie-d30782671
[d35537273]: https://www.ouedkniss.com/logiciels-abonnements-promo-logiciel-de-gestion-commercial-bab-ezzouar-ben-aknoun-birtouta-dely-brahim-hydra-alger-algerie-d35537273
[d52503089]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-simple-fiable-et-efficace-mohammadia-alger-algerie-d52503089
[ok52503089]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-commerciale-simple-fiable-et-efficace-mohammadia-alger-algerie-d52503089
[d57214320]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-caisse-gestion-premium-echo-pour-salons-barbershops-et-spas-alger-centre-algerie-d57214320
[d57584481]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%B5%D9%8A%D8%A7%D9%86%D8%A9-%D8%A7%D9%84%D9%87%D9%88%D8%A7%D8%AA%D9%81-dukan-pro-alger-centre-algerie-d57584481
[d52615477]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D8%A3%D9%82%D8%B3%D8%A7%D8%B7-%D9%84%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-logiciel-de-gestion-facilite-dar-chioukh-djelfa-algerie-d52615477
[d29886941]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-commerciale-el-eulma-setif-algerie-d29886941
[d57434988]: https://www.ouedkniss.com/bureautique-internet-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D9%88%D8%B8%D9%81%D9%8A%D9%8A%D9%86-etat-204-el-attaf-ain-defla-algerie-d57434988
[d57659580]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-restaurant-cafeteria-pizzeria-caisse-tactile-el-eulma-setif-algerie-d57659580
[d24009309]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-foodmax-logiciel-restaurant-pizzera-d24009309
[ok24009309]: https://www.ouedkniss.com/oran-algerie-applications-logiciels-foodmax-logiciel-restaurant-pizzera-d24009309
[d30642759]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-fast-food-pizzeria-restaurant-pecherie-cafeteria-cremerie-patisserie-tlemcen-algerie-d30642759
[d39031109]: https://www.ouedkniss.com/hotellerie-restauration-salles-logiciel-programme-de-gestion-pour-magasins-restaurants-et-dalimentation-setif-algerie-d39031109
[d39658646]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-restaurant-pizzeria-cafeteria-cremerie-oran-algerie-d39658646
[d56978620]: https://www.ouedkniss.com/maintenance-informatique-logiciel-de-vente-pos-restaurant-draria-alger-algerie-d56978620
[d48086740]: https://www.ouedkniss.com/logiciels-abonnements-menu-max-logiciel-de-digitalisation-et-gestion-des-restaurants-cafes-alger-centre-algerie-d48086740
[d57473655]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-specialise-pour-restaurantcafefast-foodfranchise-bir-el-djir-oran-algerie-d57473655
[d55642837]: https://www.ouedkniss.com/logiciels-abonnements-kiober-food-pos-logiciel-de-caisse-pour-les-cafes-restaurants-pizzeria-et-snacks-oran-algerie-d55642837
[d6203275]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-et-caisse-tactile-kouba-alger-algerie-d6203275
[ok6203275]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-et-caisse-tactile-kouba-alger-algerie-d6203275
[d55066585]: https://www.ouedkniss.com/autre-caisse-logiciel-clyo-alger-centre-algerie-d55066585
[d26432450]: https://www.ouedkniss.com/blida-alger-constantine-oran-boumerdes-tipaza-mila-bab-ezzouar-chelghoum-laid-algerie-applications-logiciels-logiwell-resto-d26432450
[d18723112]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-resto-pro-bab-ezzouar-alger-algerie-d18723112
[ok18723112]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-resto-pro-bab-ezzouar-alger-algerie-d18723112
[d50571559]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-restauration-itri-resto-pro-mohammadia-alger-algerie-d50571559
[d19431931]: https://www.ouedkniss.com/biskra-algerie-applications-logiciels-logiciel-de-flexy-v-5-4-d19431931
[d39656135]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-flexy-fastransfer-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D9%81%D9%84%D9%8A%D9%83%D8%B3%D9%8A-oran-algerie-d39656135
[d9607035]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-flexy-hammam-bouhadjar-ain-temouchent-algerie-d9607035
[d31231999]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-ouled-fayet-alger-algerie-d31231999
[d44602069]: https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A8%D9%8A%D8%B9-%D8%A7%D9%84%D8%B0%D9%87%D8%A8-logiciel-de-gestion-des-bijouteries-batna-algerie-d44602069
[d56860799]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-complet-pour-entreprises-et-pme-batna-bejaia-biskra-blida-tlemcen-algerie-d56860799
[d56993778]: https://www.ouedkniss.com/logiciels-abonnements-alternative-algerienne-a-sage-et-ebp-alger-centre-bir-el-djir-oran-algerie-d56993778
[d56709041]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-facturation-digifact-tlemcen-algerie-d56709041
[d55015843]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-facturation-en-ligne-innova-invoice-bir-el-djir-oran-algerie-d55015843
[d28343846]: https://www.ouedkniss.com/alger-centre-bordj-el-kiffan-algerie-ecoles-formations-formation-erp-sage-d28343846
[d30720309]: https://www.ouedkniss.com/applications-logiciels-dolibarr-erp-crm-baba-hassen-alger-algerie-d30720309
[d27522066]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-gestion-achat-vent-stock-caisse-khraissia-alger-algerie-d27522066
[d45540038]: https://www.ouedkniss.com/autre-systeme-de-caisse-pour-restaurant-logiciel-ecran-imprimante-tique-el-mouradia-alger-algerie-d45540038
[d8059698]: https://www.ouedkniss.com/alger-hydra-algerie-applications-logiciels-gestion-de-facturation-sur-mesure-d8059698
[d51577181]: https://www.ouedkniss.com/logiciels-abonnements-windev-25-cracke-kolea-tipaza-algerie-d51577181
[d29117684]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-stock-et-commerciale-outils-dactivation-bouira-algerie-d29117684
[ok11162910]: https://www.ouedkniss.com/applications-logiciels-logiciel-superette-jouet-vetement-bab-ezzouar-alger-algerie-d11162910
[ok6347367]: https://www.ouedkniss.com/setif-el-eulma-algerie-applications-logiciels-%C2%A0%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%81%D9%88%D8%A7%D8%AA%D9%8A%D8%B1-%D8%A8%D8%A7%D9%84%D9%84%D8%BA%D8%A9-%D8%A7%D9%84%D8%B9%D8%B1%D8%A8%D9%8A%D8%A9-d6347367
[ok20833261]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-point-de-vente-gestion-stock-alger-centre-douera-draria-zeralda-tipaza-algerie-d20833261
[ok5058901]: https://www.ouedkniss.com/sidi-bel-abbes-algerie-applications-logiciels-logiciel-sup%C3%89rette-et-boutique-d5058901
[ok9010110]: https://www.ouedkniss.com/guelma-algerie-applications-logiciels-logiciel-superette-d9010110
[ok13827110]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-tizi-nbechar-setif-algerie-d13827110
[ok20319224]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-dentreprise-odoo-cheraga-alger-algerie-d20319224
[ok9093894]: https://www.ouedkniss.com/setif-el-eulma-algerie-applications-logiciels-logiciel-superette-camera-redeau-d9093894
[ok9350793]: https://www.ouedkniss.com/souk-ahras-algerie-applications-logiciels-logiciel-caisse-pour-supperette-d9350793
[ok5967052]: https://www.ouedkniss.com/sidi-bel-abbes-algerie-applications-logiciels-logiciel-de-caisse-d5967052
[ok8364510]: https://www.ouedkniss.com/logiciels-abonnements-gestion-commerciale-bordj-bou-arreridj-algerie-d8364510
[ok13258442]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-dinventaire-pro-immo-dar-el-beida-alger-algerie-d13258442
[ok9470491]: https://www.ouedkniss.com/annaba-algerie-applications-logiciels-logiciel-superette-d9470491
[ok21540163]: https://www.ouedkniss.com/logiciels-abonnements-de-gestion-superette-supermarche-cosmetiques-kouba-alger-algerie-d21540163
[ok19564294]: https://www.ouedkniss.com/applications-logiciels-logiciel-pour-superette-setif-algerie-d19564294
[ok18994540]: https://www.ouedkniss.com/pcs-de-bureau-unites-centrales-gestion-stock-et-commerciale-chlef-laghouat-oum-el-bouaghi-batna-bejaia-algerie-d18994540
[ok16407861]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-%D8%A8%D8%A7%D9%84%D8%B9%D8%B1%D8%A8%D9%8A%D8%A9-tizi-nbechar-setif-algerie-d16407861
[ok21755610]: https://www.ouedkniss.com/applications-logiciels-logiciel-gest-superette-avec-des-pack-dar-el-beida-alger-algerie-d21755610
[ok20764280]: https://www.ouedkniss.com/blida-tizi-ouzou-alger-boumerdes-tipaza-ain-el-hammam-zaouia-draa-mizan-centre-benian-naadja-taya-bab-ezzouar-bordj-bahri-kiffan-rouiba-staoueli-algerie-applications-logiciels-logiciel-et-caisse-tactile-pour-magasi-d20764280
[ok12051137]: https://www.ouedkniss.com/relizane-el-matmar-algerie-applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D8%B3%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AE%D8%B2%D9%88%D9%86-%D8%A8%D8%A7%D9%84%D8%B9%D8%B1%D8%A8%D9%8A%D8%A9-d12051137
[ok20764626]: https://www.ouedkniss.com/alger-centre-ain-benian-naadja-taya-bab-el-oued-ezzouar-baba-hassen-bachdjerrah-baraki-belouizdad-ben-aknoun-beni-messous-bir-mourad-rais-birkhadem-birtouta-bologhine-bordj-bahri-kiffan-bourouba-bouzareah-casbah-chevalley-cheraga-dar-beida-dely-brahim-douera-draria-achour-biar-harrach-madania-marsa-magharia-mouradia-gue-de-constantine-rouiba-algerie-applications-logiciels-logiciel-superette-restaurant-d20764626
[ok13765224]: https://www.ouedkniss.com/alger-centre-algerie-applications-logiciels-logiciel-superette-%D8%AA%D8%B5%D9%85%D9%8A%D9%85-%D9%83%D9%84-%D8%A7%D9%84%D8%A8%D8%B1%D8%A7%D9%85%D8%AC-d13765224
[ok13826797]: https://www.ouedkniss.com/applications-logiciels-gestion-commerciale-point-de-vente-tizi-nbechar-setif-algerie-d13826797
[ok9473957]: https://www.ouedkniss.com/skikda-algerie-applications-logiciels-logiciel-superette-d9473957
[ok4732486]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-restaurant-fast-food-alger-centre-algerie-d4732486
[ok16407886]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-point-vente-tizi-nbechar-setif-algerie-d16407886
[ok19762488]: https://www.ouedkniss.com/blida-tizi-ouzou-alger-saida-boumerdes-bab-ezzouar-bordj-el-kiffan-mohammadia-algerie-applications-logiciels-promotionlogiciel-superette-restaurant-d19762488
[ok22429721]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-de-stock-%D9%84%D9%88%D8%AC%D9%8A%D8%B3%D9%8A%D8%A7%D9%84-%D8%A7%D9%84%D8%B3%D9%88%D8%A8%D9%8A%D8%B1%D8%A7%D8%AA-%D9%88%D8%AC%D9%85%D9%8A%D8%B9-%D8%A7%D9%84%D8%A3%D9%86%D8%B4%D8%B7%D8%A9-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-bouira-algerie-d22429721
[ok19254138]: https://www.ouedkniss.com/applications-logiciels-logiciel-restauration-et-cafeteria-dar-el-beida-alger-algerie-d19254138
[ok8853250]: https://www.ouedkniss.com/applications-logiciels-logiciel-point-de-vent-et-materiel-bab-ezzouar-alger-algerie-d8853250
[ok22267608]: https://www.ouedkniss.com/alger-dar-el-beida-algerie-applications-logiciels-logiciel-de-gestion-stock-caisse-d22267608
[ok21591248]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-commercial-ain-charchar-skikda-algerie-d21591248
[ok21429035]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-de-superette-boutique-mostaganem-algerie-d21429035
[ok23748056]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-dentreprise-alger-centre-algerie-d23748056
[ok7624187]: https://www.ouedkniss.com/logiciels-abonnements-logiciel-gestion-commerciale-superette-resto-et-pizzeria-ain-naadja-alger-algerie-d7624187
[ok19564652]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%A7%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-setif-algerie-d19564652
[ok3110145]: https://www.ouedkniss.com/applications-logiciels-superette-alger-centre-algerie-d3110145
[ok19752913]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-stock-et-commerciale-version-pro-bouira-algerie-d19752913
[ok19338248]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-commerciale-pvente-sig-oran-mascara-algerie-d19338248
[ok25191915]: https://www.ouedkniss.com/logiciels-abonnements-meilleur-logiciel-gestion-commerciale-adrar-chlef-laghouat-oum-el-bouaghi-batna-algerie-d25191915
[ok22519331]: https://www.ouedkniss.com/setif-el-ouricia-algerie-applications-logiciels-gestion-commerciale-d22519331
[ok16827647]: https://www.ouedkniss.com/applications-logiciels-pack-et-materiel-de-commerce-batna-bejaia-biskra-tlemcen-alger-centre-algerie-d16827647
[ok20770512]: https://www.ouedkniss.com/oum-el-bouaghi-algerie-pc-fixe-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D9%85%D9%84%D8%A7%D8%A8%D8%B3-d20770512
[ok20374297]: https://www.ouedkniss.com/applications-logiciels-logiciel-restaurant-alger-centre-algerie-d20374297
[ok22011635]: https://www.ouedkniss.com/bejaia-algerie-applications-logiciels-logiciel-gestion-restaurant-d22011635
[ok13541790]: https://www.ouedkniss.com/constantine-algerie-applications-logiciels-logiciel-de-gestion-stock-caisse-d13541790
[ok22881363]: https://www.ouedkniss.com/setif-el-ouricia-algerie-applications-logiciels-gestion-commerciale-d22881363
[ok20127285]: https://www.ouedkniss.com/setif-amoucha-algerie-applications-logiciels-logiciel-de-gestion-stock-ventes-achat-d20127285
[ok21183383]: https://www.ouedkniss.com/setif-algerie-applications-logiciels-logiciel-de-restaurant-d21183383
[ok23846124]: https://www.ouedkniss.com/alger-centre-algerie-applications-logiciels-logiciel-de-gestion-caisse-ultra-si-d23846124
[ok8897087]: https://www.ouedkniss.com/applications-logiciels-logiciel-et-materiels-de-la-caisse-bouira-algerie-d8897087
[ok22047224]: https://www.ouedkniss.com/applications-logiciels-planete-de-gestion-alger-centre-algerie-d22047224
[ok11536954]: https://www.ouedkniss.com/applications-logiciels-e-shop-gestion-commerciale-pour-magas-setif-algerie-d11536954
[ok17762051]: https://www.ouedkniss.com/batna-merouana-algerie-applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D9%85%D8%AD%D9%84-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-d17762051
[ok24116972]: https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-annaba-algerie-d24116972
[ok23568445]: https://www.ouedkniss.com/tlemcen-algerie-applications-logiciels-logiciel-sup%C3%A9rette-restaurant-d23568445
[ok24322509]: https://www.ouedkniss.com/alger-douera-algerie-applications-logiciels-logiciel-caisse-sup%C3%A9rette-magasin-gros-d24322509
[ok24097980]: https://www.ouedkniss.com/chlef-laghouat-bechar-blida-tlemcen-tiaret-alger-saida-sidi-bel-abbes-mostaganem-mascara-oran-el-bayadh-naama-ain-temouchent-relizane-centre-sefra-mecheria-algerie-applications-logiciels-logiciel-de-caisse-pour-restaurant-d24097980
[ok25214517]: https://www.ouedkniss.com/alger-el-mouradia-algerie-applications-logiciels-logiciel-de-facturation-alg%C3%A9rie-d25214517
[ok25860750]: https://www.ouedkniss.com/setif-guidjel-algerie-applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-%D9%88%D8%A7%D9%84%D9%85%D8%AE%D8%A7%D8%B2%D9%86-d25860750
[ok25233066]: https://www.ouedkniss.com/setif-tizi-nbechar-algerie-applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%A8%D9%8A%D8%B9%D8%A7%D8%AA-%D9%88%D8%A7%D9%84%D9%85%D8%AE%D8%A7%D8%B2%D9%86-d25233066
[ok25879075]: https://www.ouedkniss.com/bejaia-blida-bouira-tlemcen-tizi-ouzou-alger-jijel-setif-sidi-bel-abbes-oran-centre-algerie-applications-logiciels-logiciel-de-gestion-commercial-stock-d25879075
[ok26087778]: https://www.ouedkniss.com/chlef-algerie-applications-logiciels-logiciel-commercial-d26087778
[ok25041060]: https://www.ouedkniss.com/applications-logiciels-logiciel-gestion-de-stock-birkhadem-alger-algerie-d25041060
[ok25787532]: https://www.ouedkniss.com/applications-logiciels-logiciel-de-gestion-du-stock-tebessa-algerie-d25787532
[ok25238323]: https://www.ouedkniss.com/bordj-bou-arreridj-algerie-applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-d25238323
[ok26104056]: https://www.ouedkniss.com/bejaia-algerie-applications-logiciels-logiciel-de-caisse-et-vente-d26104056
[ok24133059]: https://www.ouedkniss.com/alger-centre-algerie-applications-logiciels-logiciel-des-v%C3%AAtements-d24133059
[ok21621288]: https://www.ouedkniss.com/applications-logiciels-logiciel-restaurant-cafeteria-bordj-bou-arreridj-algerie-d21621288
[ok26327909]: https://www.ouedkniss.com/ghardaia-algerie-applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D9%85%D8%A8%D9%8A%D8%B9%D8%A7%D8%AA-logiciel-gestion-stock-d26327909
[ok25676170]: https://www.ouedkniss.com/el-oued-algerie-applications-logiciels-logiciel-de-gestion-point-vente-d25676170
[ok25918343]: https://www.ouedkniss.com/chlef-bejaia-bechar-blida-bouira-tlemcen-tizi-ouzou-alger-setif-saida-sidi-bel-abbes-mostaganem-oran-boumerdes-tipaza-ain-temouchent-relizane-centre-algerie-applications-logiciels-logiciel-restaurant-restoline-d25918343
[ok25633863]: https://www.ouedkniss.com/jijel-sidi-maarouf-algerie-applications-logiciels-logiciel-de-gestion-point-vente-d25633863
[ok25553128]: https://www.ouedkniss.com/oran-bir-el-djir-algerie-applications-logiciels-logiciel-de-gestion-commercial-d25553128
[ok25839906]: https://www.ouedkniss.com/alger-el-mouradia-algerie-applications-logiciels-logiciel-de-facturation-alg%C3%A9rien-d25839906
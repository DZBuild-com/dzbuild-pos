# 03 — Global POS Feature Taxonomy (benchmark of the best POS systems worldwide)

> **Project:** DZBuild POS — a free, offline-first, installable Windows desktop POS + "gestion commerciale / gestion de stock" for the Algerian market, optionally linked to a DZBuild (dzbuild.dev) account via OAuth.
> **Document purpose:** a complete, hierarchical catalog of every feature that leading POS products ship, so the DZBuild POS scope misses nothing. Each feature is tagged with the products that have it, whether it is **table-stakes** or a **differentiator**, and a suggested release slot for DZBuild POS.
> **Research date:** 2026-09-28. **Author:** DZBuild POS team (global benchmark track).
> **Companion docs:** 01/02 (Algerian market and local competitors), 04+ (stack and architecture). Algeria-specific legal points in this file are marked "confirm in DZ docs". This file does not verify them.

---

## 0. How to read this document

### 0.1 Product codes

| Code | Product (vendor) | Type / deployment | License / price model | Why it is in the benchmark |
|---|---|---|---|---|
| **OD** | Odoo 18/19 Point of Sale + Inventory + Purchase (Odoo S.A.) | Browser-based. Keeps working through short network outages. Self-hostable. | Community edition is LGPL (POS is in Community). Enterprise is paid. | The most complete open ERP-grade POS. It is the reference for pricelists, loyalty, and the restaurant and inventory depth. |
| **SQ** | Square POS / Square for Retail / Square for Restaurants (Block) | Cloud + native apps, offline card mode | Free plan + Plus/Premium tiers | The best-known "free POS". It sets the UX bar. |
| **LS-R** | Lightspeed Retail R-Series | Cloud | Subscription | Deep retail: serials, special orders, work orders, FIFO cost |
| **LS-X** | Lightspeed Retail X-Series (formerly **Vend**) | Cloud, browser/iPad | Subscription | Vend lineage: composites, promotions, layby, on-account, register closures |
| **LS-K** | Lightspeed Restaurant K-Series | Cloud, iPad | Subscription | Hospitality: floor plans, courses, seat split, bar tabs, happy-hour price lists |
| **LV** | Loyverse POS | Cloud + mobile apps, offline sales | Free core + paid add-ons | The best-known free POS in emerging markets. The closest "free" competitor model. |
| **SH** | Shopify POS (Lite / Pro) | Cloud + mobile | Plan + POS Pro add-on | The omnichannel reference (BOPIS, ship-from-store, exchanges) |
| **TO** | Toast POS | Cloud + Android hardware, offline mode | Subscription + processing | Restaurant reference: 86ing, KDS, course firing |
| **ZO** | Zoho POS + Zoho Inventory | Cloud + apps, offline billing | Freemium/subscription | Inventory depth: batches, bins, backorders, dropship, FIFO lots |
| **CL** | Clover (Fiserv) | Cloud + proprietary hardware | Hardware + subscription | App-market model, gift cards, employee scheduling |
| **EN** | ERPNext / Frappe POS | Browser, self-hostable, offline apps exist | GPL-3 (open source) | Open ERP POS: POS Profile, opening/closing entries, FIFO/Moving-average |
| **DT** | Dolibarr TakePOS | Browser (PHP), self-hosted | GPL-3 | French-speaking open ERP. Widely used in the Maghreb. Good "gestion commerciale" reference. |
| **UC** | uniCenta oPOS | **Desktop Java app** (Win/Linux/Mac), local DB | GPL-3 | Offline desktop POS, the closest architecture to DZBuild POS |
| **CH** | Chromis POS | **Desktop Java** (Openbravo POS fork) | GPL | Offline desktop POS with GS1 weight/price barcodes and a kitchen screen |
| **FL** | Floreant POS | **Desktop Java**, embedded DB | Open source | Offline restaurant POS (30,000 restaurants claimed) |
| **SB** | SambaPOS v5 | **Windows desktop (.NET)** + SQL Server Express | Freemium/commercial | Windows restaurant POS with a rules and automation engine |
| **OB** | Openbravo POS (Orisha Commerce) | HTML5 offline-capable web POS | Commercial (legacy open-source POS exists) | Enterprise omnichannel retail with fiscal compliance |
| **OR** | Ordoro | Cloud | Subscription | E-commerce ops reference: kitting, dropship, multi-warehouse |
| **KO** | KORONA POS (COMBASE) | Cloud + Windows registers, offline mode | Subscription | Loss prevention, blind balance, retail KPIs (GMROI, sell-through) |
| **HK** | Hike POS | Cloud, works offline | Subscription | Tender breadth: layby, on-account, store credit, cheque |
| **OS** | Open Source Point of Sale (OSPOS) | Web (PHP/CodeIgniter 4/MySQL) | MIT + mandatory footer | Open-source retail with quotations, expenses, cash-up |
| **EP** | Epos Now | Cloud | Subscription | Training mode, AI pricing, delivery-app integrations |

### 0.2 Attribution rules (honesty markers)

- A product code **without** a mark was verified from that product's own site, documentation, help center, or a cited third-party review. See §1 for the source list.
- A product code with **†** comes from general product knowledge. It was **not re-verified**, so treat it as **unverified**. It is probably right, but confirm it before quoting it externally.
- **"—"** in *Seen in* means a best-practice or engineering feature that vendors do not usually advertise. It is still recommended.
- "e.g." lists are **not exhaustive**. A missing code does **not** mean the product lacks the feature. It means the feature was not evidenced during the research.

### 0.3 Tier and release tags

| Tag | Meaning |
|---|---|
| **TS** | *Table-stakes.* Users expect it. Its absence blocks adoption. |
| **DF** | *Differentiator.* Present only in the better or premium products. Wins users. |
| **AD** | *Advanced / niche.* Useful for specific verticals or bigger merchants. |
| **M** | Recommended for DZBuild POS **MVP** (first public release) |
| **V1** | Recommended for the first major update after MVP |
| **V2** | Later roadmap |
| **opt** | Optional / plugin / only if demand appears |
| **n/a** | Not applicable to an offline Windows POS in Algeria (e.g., US-only card networks). Listed for completeness. |

---

## 1. Sources consulted (per product)

**Odoo (OD)**
- S1 Odoo 18 POS docs index: https://www.odoo.com/documentation/18.0/applications/sales/point_of_sale.html
- S2 Pricing features (discounts, discount tags, loyalty, pricelists, fiscal positions, cash rounding, ESL): https://www.odoo.com/documentation/18.0/applications/sales/point_of_sale/pricing.html
- S3 Restaurant features: https://www.odoo.com/documentation/18.0/applications/sales/point_of_sale/restaurant.html
- S4 Shop features (sales orders, barcodes, serial/lots, ship later): https://www.odoo.com/documentation/18.0/applications/sales/point_of_sale/shop.html
- S5 Payment methods (cash/bank journals, identify customer, terminals, QR): https://www.odoo.com/documentation/18.0/applications/sales/point_of_sale/payment_methods.html
- S6 Odoo 19 POS docs TOC (customer display, scale, cash machines, presets, UrbanPiper): https://www.odoo.com/documentation/19.0/applications/sales/point_of_sale.html
- S7 Customer account (credit limit, deposit, settle orders/invoices): https://www.odoo.com/documentation/19.0/applications/sales/point_of_sale/payment_methods/customer_credit.html
- S8 Odoo 19 release notes (courses, allergens, minimal-rights profile, ZPL labels, prep-time report, forecast purchasing): https://www.odoo.com/odoo-19-release-notes
- S9 Inventory valuation (Standard / AVCO / FIFO, landed costs): https://www.odoo.com/documentation/18.0/applications/inventory_and_mrp/inventory/product_management/inventory_valuation/inventory_valuation_config.html and https://www.odoo.com/documentation/19.0/applications/inventory_and_mrp/inventory/inventory_valuation/cheat_sheet.html
- S10 POS configuration (categories, restrict categories, multi-employee login): https://www.odoo.com/documentation/18.0/applications/sales/point_of_sale/configuration.html

**Square (SQ)**
- S11 Retail pricing and plan comparison: https://squareup.com/us/en/point-of-sale/retail/pricing
- S12 Inventory management features: https://squareup.com/us/en/point-of-sale/features/inventory-management
- S13 Cash drawer sessions: https://squareup.com/help/us/en/article/8344-start-and-end-a-cash-drawer-session and reports: https://squareup.com/help/us/en/article/8358-view-cash-drawer-reports
- S14 Offline payments: https://squareup.com/help/us/en/article/7777-process-card-payments-with-offline-mode
- S15 Bulk item import/export: https://squareup.com/help/us/en/article/5153-import-items-online
- S16 Purchase orders / vendors: https://squareup.com/help/us/en/article/8258-create-purchase-orders-with-square-for-retail and https://squareup.com/help/us/en/article/5958-vendor-management
- S17 Square for Restaurants (coursing, seats, conversational modifiers, auto-gratuity, KDS): https://www.merchantmaverick.com/reviews/square-for-restaurants-pos-review/ and https://fitsmallbusiness.com/square-for-restaurants/
- S18 Reviews (payments incl. QR/BNPL, time clock, plans): https://www.nerdwallet.com/business/software/reviews/square-for-retail and https://www.techrepublic.com/article/square-pos-review/ ; AI tools: https://www.businessnewsdaily.com/2955-best-pos-systems.html

**Shopify POS (SH)**
- S19 Features page: https://www.shopify.com/pos/features
- S20 "93 POS features" article: https://www.shopify.com/blog/pos-features
- S21 Lite vs Pro: https://help.shopify.com/en/manual/sell-in-person/getting-started/identify-your-pos-needs

**Lightspeed (LS-R / LS-X / LS-K)**
- S22 Retail inventory page: https://www.lightspeedhq.com/pos/retail/inventory-management-software/
- S23 R-Series special orders / layaways / work orders: https://retail-support.lightspeedhq.com/hc/en-us/articles/229130588-Creating-a-special-order , https://retail-support.lightspeedhq.com/hc/en-us/sections/8332210562843-Special-orders-and-layaways , https://retail-support.lightspeedhq.com/hc/en-us/articles/229131428-Creating-and-completing-work-orders
- S24 R-Series average vs FIFO cost: https://retail-support.lightspeedhq.com/hc/en-us/articles/40045780747035-Understanding-average-cost-and-FIFO-cost-methods ; pricing concepts: https://retail-support.lightspeedhq.com/hc/en-us/articles/229130248-About-pricing-concepts
- S25 X-Series composites / variants: https://x-series-support.lightspeedhq.com/hc/en-us/articles/50084912660635-Adding-composite-products ; promotions and tiered promotions: https://x-series-support.lightspeedhq.com/hc/en-us/articles/25534138271643-Setting-up-tiered-promotions-in-Retail-POS-X-Series
- S26 X-Series store credit / on-account / layaway / returns: https://x-series-support.lightspeedhq.com/hc/en-us/articles/25534209899675-Issuing-store-credit-in-Retail-POS-X-Series , https://x-series-support.lightspeedhq.com/hc/en-us/articles/25534089724827-Managing-On-account-balances , https://x-series-support.lightspeedhq.com/hc/en-us/articles/25533693114011-Creating-and-managing-layaway-sales , https://x-series-support.lightspeedhq.com/hc/en-us/articles/25534237536411-Returning-or-exchanging-a-sale-in-Retail-POS-X-Series
- S27 X-Series register closure / blind close / cash movements: https://x-series-support.lightspeedhq.com/hc/en-us/articles/25534185400347-Opening-and-closing-a-register , https://x-series-support.lightspeedhq.com/hc/en-us/articles/25534307257243-How-to-hide-expected-cash-when-closing-the-register , https://x-series-support.lightspeedhq.com/hc/en-us/articles/42681623234331-Using-the-cash-movements-report
- S28 K-Series floor plans / check splitting / bar tabs / price lists: https://k-series-support.lightspeedhq.com/hc/en-us/articles/1260804656689-About-floor-plans-and-tables , https://k-series-support.lightspeedhq.com/hc/en-us/articles/360051089493-Check-splitting , https://k-series-support.lightspeedhq.com/hc/en-us/articles/4408089985179-Creating-and-managing-bar-tabs , https://k-series-support.lightspeedhq.com/hc/en-us/articles/4402274951323-About-price-lists
- S29 K-Series overview: https://www.lightspeedhq.com/uk/blog/lightspeed-restaurant-k-series/

**Loyverse (LV)**
- S30 Features: https://loyverse.com/features
- S31 Advanced inventory: https://loyverse.com/advanced-inventory
- S32 Pricing (free vs add-ons): https://loyverse.com/pricing
- S33 Offline: https://help.loyverse.com/help/offline-work-of-pos
- S34 Shifts / pay in / pay out: https://help.loyverse.com/help/shift-management-loyverse-pos , https://help.loyverse.com/help/shift-reports
- S35 Import/export: https://help.loyverse.com/help/importing-and-exporting , https://help.loyverse.com/help/importing-and-exporting-customers , https://help.loyverse.com/help/exporting-data-from-loyverse-account

**Toast (TO)**
- S36 KDS: https://pos.toasttab.com/hardware/kitchen-display-system ; reviews: https://www.posusa.com/toast-pos-review/ , https://www.therestauranthq.com/restaurants/toast-pos-review/

**Zoho (ZO)**
- S37 Zoho Inventory features: https://www.zoho.com/inventory/features/
- S38 Zoho POS: https://www.zoho.com/en-us/pos/ ; pricing (registers per plan): https://www.zoho.com/en-us/pos/pricing.html

**Clover (CL)**
- S39 https://www.businessnewsdaily.com/16068-clover.html ; https://www.forbes.com/advisor/business/software/clover-pos-review/

**ERPNext (EN)**
- S40 POS docs: https://docs.frappe.io/erpnext/user/manual/en/point-of-sale ; POS Profile: https://docs.frappe.io/erpnext/user/manual/en/pos-profile
- S41 Valuation FIFO / Moving average: https://docs.frappe.io/erpnext/fifo-and-moving-average ; Stock settings: https://docs.erpnext.com/docs/user/manual/en/stock-settings
- S42 Community offline POS apps for ERPNext (shows the offline gap): https://github.com/RAYANaouf/POS-ERPNext , https://github.com/ERPGulf/GPos

**Dolibarr (DT)**
- S43 POS presentation: https://www.dolibarr.org/presentation-point-of-sale-pos.php ; TakePOS wiki: https://wiki.dolibarr.org/index.php/Module_Point_of_sale_(TakePOS)
- S44 Stock management (lots, sell-by / eat-by, PMP, virtual stock): https://www.dolibarr.org/presentation-stock-management.php
- S45 Advanced Barcodes for TakePOS (3rd-party module, weight/price barcodes): https://www.dolistore.com/product.php?id=433&l=en

**Desktop open source (UC, CH, FL, SB)**
- S46 uniCenta: https://unicenta.com/ , https://www.linuxlinks.com/unicenta-open-source-point-sale/
- S47 Chromis: https://www.linuxlinks.com/chromis-pos-commercial-grade-free-open-source-point-sale-software/ , https://chromis.co.uk/kitchen-display-screen/
- S48 Floreant: https://floreant.org/ , https://floreantpos.org/
- S49 SambaPOS: https://www.sambapos.com/ ; automation commands: https://kb.sambapos.com/en/2-3-16-a-how-to-edit-automation-commands/ ; action/rule structure: https://kb.sambapos.com/en/4-3-how-does-it-work-action-rule-structure/ ; split ticket: https://kb.sambapos.com/en/4-64-how-to-use-split-ticket-feature/ ; backups: https://kb.sambapos.com/en/5-5-3-how-to-take-backup/ ; delivery: https://kb.sambapos.com/en/4-12-advanced-delivery-service-implementation-v5-2-3/

**Others**
- S50 Openbravo POS (Orisha): https://www.orisha.com/en/commerce/software/openbravo-pos
- S51 Ordoro inventory: https://www.ordoro.com/features/inventory-management
- S52 KORONA POS features: https://koronapos.com/pos-features/ ; loss prevention: https://koronapos.com/pos-features/retail-loss-prevention/ ; 15 essentials: https://koronapos.com/blog/pos-system-features/
- S53 Hike POS payments and features: https://hikeup.com/us/payments/ , https://www.merchantmaverick.com/reviews/hike-pos-review/
- S54 OSPOS README: https://github.com/opensourcepos/opensourcepos
- S55 Epos Now: https://tech.co/pos-system/epos-now-pos-review , https://www.capterra.com/p/152638/Epos-Now/
- S56 2026 AI-in-POS trend pieces: https://www.connectpos.com/ai-pos-software/ , https://lithospos.com/blog/pos-system-trends-2026-ai-voice-ordering-cloud-technology-reshaping-retail-and-restaurants/

> **Method note:** The research used web search and page fetches (EN). Vendor feature pages are marketing pages, so wherever possible the help-center article was preferred (e.g., Lightspeed support, Square help, Odoo docs). Not every product could be checked row by row; rows marked † rely on the vendor's marketing page or general product knowledge.

---

## 2. Executive synthesis (what the benchmark says)

1. **Nobody combines every capability in one free, offline, installable product.**
   - Free products (Loyverse, Square Free) are cloud-first. They lock advanced inventory, employee management, and history behind paid add-ons (S32, S11).
   - Deep open-source products (Odoo, ERPNext, Dolibarr) are browser/server apps. They are heavy to install and weak or conditional offline. ERPNext needs community apps for real offline use (S42).
   - Desktop offline POS (uniCenta, Chromis, Floreant, SambaPOS) have dated UX and shallow "gestion commerciale": weak purchasing, supplier debt, and landed cost.
   - **An offline Windows app with Odoo-level depth, Loyverse-level simplicity, and Shopify-level polish would be unique.**
2. **Offline is where cloud leaders are weakest**, and that is the DZBuild opportunity:
   - Loyverse does not show stock levels or negative-stock alerts offline (S33).
   - Square offline cannot take gift cards, run loyalty, or do close-of-day reporting, and it caps offline card amounts (S14).
   - Toast's offline mode "has significant limitations" (S36).
   - An offline-first design (local DB is the source of truth, cloud is an optional sync target) removes this whole class of problems.
3. **The recurring "premium" differentiators**, which are worth building in since the product is free:
   - blind close and cash-variance tolerance (LS-X, KO)
   - layaway + on-account + store credit + deposit (LS-X, HK, OD)
   - serial/lot/expiry with FIFO costing (LS-R, ZO, OD, EN, DT)
   - composite/kit products with derived stock (LS-X, LV, ZO, OR)
   - pricelists by customer group, quantity, and date (OD)
   - special orders, work orders, and repairs (LS-R)
   - reorder suggestions and demand forecasting (LS-R, SH, OD 19)
   - granular role permissions with manager override (SH, KO)
   - loss-prevention reports (KO)
   - KDS with coursing (TO, SQ, LS-K, OD 19)
   - happy-hour scheduled price lists (LS-K)
   - GS1 weight/price barcodes (CH, LV)
   - CSV/XLSX import with undo (SQ)
4. **Customer credit ("crédit client") is first-class in ERP-style POS** (Odoo customer account with credit limit, deposits, settle orders/invoices, S7; Lightspeed on-account, S26; Hike credit limits, S53). This matches the paper "carnet de crédit" habit of small shops (confirm in DZ docs). It should be an MVP feature.
5. **Import/export is universal but uneven.**
   - Square supports XLSX/CSV, 30,000 items per file, modify vs replace, and **undo import** (S15).
   - Loyverse exports items, customers, and receipts (S35).
   - uniCenta and Chromis import categories and products from CSV (S46, S47).
   - DZBuild POS should support import and export for every master-data and transaction entity, in XLSX and CSV, with templates, a validation preview, dry-run, and undo.
6. **2025–2026 trend features:**
   - AI assistants and voice commands (Square AI tools, S18; Lightspeed AI, S29; Epos Now AI pricing, S55)
   - demand forecasting (SH, LS-R, OD 19 forecast-based purchasing)
   - self-ordering kiosks and QR ordering (OD, DT)
   - electronic shelf labels (OD)
   - delivery-aggregator integrations (OD UrbanPiper/GrabFood, EP)
   - WhatsApp notifications (OD 19, ZO)

   For DZBuild POS, the realistic trend features are offline AI-lite (reorder suggestions, anomaly flags), WhatsApp share of receipts and debt statements, and QR ordering on the LAN.

---
## 3. The feature catalog

Column legend: **ID** | **Feature** | **Seen in** (product codes; † means unverified, — means best practice) | **Tier** (TS/DF/AD) | **DZ** (recommended DZBuild POS release: M / V1 / V2 / opt / n/a) | **Notes**

---

### A. Checkout, cart and register screen

#### A.1 Session and register

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CHK-001 | Open a register session or shift before selling, with the opening float counted | OD, SQ, LS-X, LV, EN (POS Opening Entry), DT (cash control "at opening"), KO† | TS | M | SQ, LV and LS-X track opening cash per drawer or shift (S13, S34, S27) |
| CHK-002 | Several registers or terminals per store | OD, ZO (registers per plan), DT, UC, CH, LS-X†, SQ† | TS | M | LAN multi-poste is common in Algerian supermarkets (confirm in DZ docs) |
| CHK-003 | Several cashiers on one session, fast user switch by PIN or badge | OD (multi-employee login), SH (staff PINs), LV†, SQ† | TS | M | |
| CHK-004 | Parallel or parked sales (several open carts at once) | DT ("parallel sales"), LV (open tickets), SH (save/retrieve carts), OD† | TS | M | Needed when a customer "forgets something" |
| CHK-005 | Work period or business day (day open/close separate from shifts) | SB (Work Period Started event), EN† | DF | M | Z-report per business day |
| CHK-006 | Register lock screen (auto-lock after idle, unlock with PIN) | SB (Unlock Ticket)†, SQ† | TS | M | |
| CHK-007 | Training / demo mode (sales not posted) | EP (training mode) | DF | V1 | Safe way to onboard new cashiers |

#### A.2 Adding items to the cart

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CHK-010 | Product grid with categories, colours and images; configurable tile layout | SH (customizable smart grid), SQ (visual grid), OD (categories; OD19 parent/child grouping), UC (customizable sales screen), LV† | TS | M | |
| CHK-011 | Universal search (name, SKU, barcode, reference, Arabic/French names) | SH (universal search bar), OD† | TS | M | Needs accent- and diacritic-insensitive search in AR and FR |
| CHK-012 | Barcode scan to add (USB/HID or serial scanner) | OD, LV, SH, ZO, DT, UC, CH, EP, KO | TS | M | |
| CHK-013 | Camera barcode scanning | LV (rear camera), SH, ZO | DF | V2 | On desktop this would use a webcam. Low priority. |
| CHK-014 | Weight- or price-embedded barcodes (EAN-13 prefixes 2x, GS1 variable measure) | LV (weight barcodes), CH (GS1 variable price or weight), DT (3rd-party Advanced Barcodes), OD† | DF (TS for groceries/butchers) | M | Configurable prefix, item-code length, weight or price decimals |
| CHK-015 | Live scale integration (read the weight from a serial or USB scale) | OD (Scale), UC (weighing scales) | DF | V1 | |
| CHK-016 | Quantity entry: numpad, "qty ×" multiplier, decimal quantities | OD†, LV†, UC† | TS | M | |
| CHK-017 | Price override on a line (permission-gated, logged) | KO ("simple and locked price changes"), EN† (allow rate change), OD† | TS | M | |
| CHK-018 | Custom or open item (typed name and price, no catalog entry) | SH (custom sale), SQ† | TS | M | |
| CHK-019 | Line notes and order notes | SH (order notes), OD (customer notes) | TS | M | |
| CHK-020 | Assign a salesperson per line or per sale ("add staff to sale") | SH (Pro), LS-R† | DF | V1 | Needed for commissions |
| CHK-021 | Product quick-info (stock in other locations, price, description) on long press | OD 19 (long-press product info), SH (product descriptions) | DF | V1 | |
| CHK-022 | Variant picker (size/colour matrix) popup | LV, SH, LS-X, ZO | TS | M | |
| CHK-023 | Modifier popup (forced and optional groups) | LV (modifiers), SQ (forced modifiers), TO† | TS (hospitality) | M | |
| CHK-024 | Serial or lot prompt when selling a tracked item | OD (serial numbers and lots in POS), LS-R (serialized) | DF | V1 | IMEI capture for phone shops |
| CHK-025 | Combo or menu builder at sale time | OD (product combos) | DF | V1 | |
| CHK-026 | Upsell or suggested items | SH (sale item suggestions) | AD | V2 | |
| CHK-027 | Quick keys, favourites pages, function keys | UC (customizable screens)†, SB (screen customization) | TS | M | |
| CHK-028 | Price-check mode (look up without selling) | — (KO†) | TS | M | |
| CHK-029 | Repeat last line or last sale | — | DF | M | |

#### A.3 Cart operations

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CHK-030 | Change quantity, remove a line (a line void is logged) | KO (line item voids), SB (Void, Cancel Void) | TS | M | |
| CHK-031 | Cancel the whole sale (permission-gated, logged) | KO (receipt cancellation), SH (cancel and void orders) | TS | M | |
| CHK-032 | Save or park a cart and retrieve it later | SH (save and retrieve carts), LV (open tickets) | TS | M | |
| CHK-033 | Attach a customer (search, scan loyalty card, create inline) | OD (set customers), LV (loyalty card barcode), SH | TS | M | |
| CHK-034 | Checkout validations (e.g., require a customer for credit sales) | SH (checkout validations / required checkout info), OD (Identify Customer) | DF | M | |
| CHK-035 | Sale type or dining option (in-store / takeaway / delivery) | LV (dining options), OD 19 (presets) | TS (resto) | M | |
| CHK-036 | Gift line (zero-price with reason) | SB (Gift / Cancel Gift commands) | AD | V1 | |
| CHK-037 | Send cart to an online checkout | SH (send email carts) | AD | n/a | |
| CHK-038 | Convert cart to quotation, sales order, proforma or invoice | OD (quotations, sales orders), OS (quotation and invoicing) | DF | M | Core of "gestion commerciale" |
| CHK-039 | Customer-facing display mirroring the cart (second monitor or pole display) | OD (customer display), LV, SH (Customer View), KO, UC (customer displays) | TS | M | A second HDMI monitor is a cheap way to do this on Windows |
| CHK-040 | Live totals: subtotal, discounts, taxes, total, item count | all | TS | M | |
| CHK-041 | Standard sale finished in 3 taps or fewer | KO (15 essentials) | TS | M | UX KPI |

#### A.4 Finishing the sale

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CHK-050 | Payment screen with quick-cash buttons (exact, common banknotes) and change due | OD†, LV† | TS | M | DZD banknotes 200/500/1000/2000 (confirm in DZ docs) |
| CHK-051 | One-click payment validation | OD 19 | DF | M | |
| CHK-052 | Receipt choice: print / none / email / SMS / WhatsApp | SH (customer receipt selection), ZO (email, SMS, WhatsApp), LV (printed or electronic) | TS | M | |
| CHK-053 | Kick the cash drawer on cash payment | LV, FL, UC (cash drawer support) | TS | M | |
| CHK-054 | Issue an invoice at payment (B2B customer with tax IDs) | OD (invoice at payment), OS | TS (DZ) | M | |
| CHK-055 | Ship later / deliver later | OD (ship later), SH (ship to home) | DF | V1 | |
| CHK-056 | Reprint the last receipt; searchable receipt history | LV (receipts history), UC† | TS | M | |

---

### B. Payments and tenders

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PAY-001 | Cash | all | TS | M | |
| PAY-002 | Integrated card terminal (amount pushed to the terminal) | OD (Adyen, Ingenico, Stripe, Worldline, SIX, Viva…), SQ, SH, CL, TO, HK | TS (global) | opt | CIB/Edahabia terminals in Algeria are mostly standalone (confirm in DZ docs) |
| PAY-003 | Non-integrated card (record-only tender) | HK (non-integrated card payments), OD (bank-journal method) | TS | M | |
| PAY-004 | Split payment across several tenders | OD, SH, ZO, KO, HK, DT (multi-payment) | TS | M | |
| PAY-005 | Partial payment (pay part now, the rest later) | SH (partial payments), FL (partial payments), HK | TS | M | Connects to layaway and on-account |
| PAY-006 | Custom payment types (cheque, bank transfer, CCP, mobile transfer…) | SH (custom payment types), HK (cheque, custom), ZO (bank transfer) | TS | M | |
| PAY-007 | QR-code payment (show a QR code for the customer to pay) | OD (QR code payments), SQ (QR) | DF | V1 | e.g., a static QR for a transfer app (confirm in DZ docs) |
| PAY-008 | Mobile wallets / NFC | SQ, CL, KO | TS (global) | n/a | |
| PAY-009 | BNPL (Afterpay/Clearpay, Shop Pay Installments) | SQ, SH, HK | DF (global) | n/a | Replaced locally by layaway and instalments |
| PAY-010 | Gift card as a tender | SH, OD, HK, KO, CL, OS | DF | V1 | |
| PAY-011 | Store credit as a tender | LS-X, HK | DF | M | |
| PAY-012 | Loyalty points as a tender | HK, OD (rewards) | DF | V1 | |
| PAY-013 | On-account / customer credit tender with a credit limit | OD (customer account + credit limit), LS-X (on account), HK (credit limits) | TS (DZ) | M | The digital "carnet de crédit" |
| PAY-014 | eWallet / prepaid balance top-up ("deposit money") | OD (eWallet; Deposit money) | DF | V1 | |
| PAY-015 | Tips / gratuity presets, tip after payment, auto-gratuity for large parties | OD (15/20/25%, tip after payment), SH, CL (tip screens), SQ (party-size auto-gratuity) | TS (resto, global) | V2 | Low cultural relevance in DZ |
| PAY-016 | Cash rounding (e.g., to the nearest 5 or 10 DA) | OD (cash rounding) | DF | M | |
| PAY-017 | Change-handling rules (no change on card tenders, max overpay) | OD (change handling) | TS | M | |
| PAY-018 | Foreign-currency cash with an exchange rate | OB (currencies), DT†, EN† | AD | opt | |
| PAY-019 | Surcharge or fee per payment method | — | AD | opt | |
| PAY-020 | Offline card store-and-forward | SQ (24 h Reader / 72 h Register), CL, TO | TS (global) | n/a | |
| PAY-021 | Automated cash machines (recyclers) | OD (Cashdro, Cashmatic, Glory) | AD | n/a | |
| PAY-022 | Payment method restricted to identified customers | OD (Identify Customer) | DF | M | |
| PAY-023 | Write-off of small differences (write-off limit) | EN (write-off account, cost centre, limit) | AD | V1 | |
| PAY-024 | Refund to the original tender, store credit or gift card | SH (refund to gift card), LS-X (store credit), KO | TS | M | |
| PAY-025 | Instalment or payment plans | KO (payment plans), HK (layby) | DF | V1 | "Paiement par tranches" (confirm in DZ docs) |
| PAY-026 | Auto-reconciliation of tenders with accounting | SH (auto-reconciliation), OD (journals) | AD | V2 | |
| PAY-027 | Settle a customer's old debts from the POS (settle orders or invoices, partial) | OD (Settle orders / Settle invoices) | DF | M | |
| PAY-028 | Payment reference capture (cheque number, bank, transfer ID) | — (OD†) | TS | M | |

---

### C. Returns, exchanges and voids

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| RET-001 | Refund selected items or the whole receipt | LV (refund selected/all items), OD (return and refund), SH | TS | M | |
| RET-002 | Find the original sale by scanning the receipt barcode, or by number, date or customer | LV†, OD† | TS | M | |
| RET-003 | Return without a receipt (permission-gated) | KO† | TS | M | |
| RET-004 | Exchange in one transaction (return + new items, pay or refund the difference) | SH (exchanges, Pro), LS-X, KO | TS | M | |
| RET-005 | Mandatory return reason codes | LS-R (custom adjustment reasons)† | DF | M | |
| RET-006 | Restock or write off (damaged) per returned line | KO (restocks instantly) | DF | M | |
| RET-007 | Refund method: cash / original / store credit / gift card | LS-X, SH, KO | TS | M | |
| RET-008 | Cross-location returns | SQ (Premium), SH (buy online, return in store) | AD | V2 | |
| RET-009 | Return-window policy (days) and a warning when it is exceeded | — | DF | V1 | |
| RET-010 | Block refunds above the sold quantity; track quantity already returned | — | TS | M | Anti-fraud |
| RET-011 | Void a completed sale (same session) vs refund (later), with authentication | SH (cancel/void orders), KO (receipt voids require authentication) | TS | M | |
| RET-012 | Credit note (avoir) document | ZO (returns → credits), OD†, DT† | TS (DZ) | M | |
| RET-013 | Return to supplier generated from a customer return (RMA) | OR (returns management), ZO† | AD | V2 | |
| RET-014 | Gift receipt (no prices) | SH (gift receipts) | DF | V1 | |

---

### D. Discounts, promotions, coupons, bundles, happy hour

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| DSC-001 | Manual line discount (% or amount) | LV, OD, SH | TS | M | |
| DSC-002 | Manual whole-order discount | LV, OD (product or order level) | TS | M | |
| DSC-003 | Predefined named discounts (e.g., "Staff 10%") | LV†, SQ† | TS | M | |
| DSC-004 | Maximum discount per role; manager approval above the limit | SH (manager approvals), KO (locked price changes, discount monitoring), EN† | DF | M | |
| DSC-005 | Automatic, rule-based discounts (no code needed) | SH (automatic discounts), KO (scheduled, auto-applied, stacking rules) | DF | V1 | |
| DSC-006 | Coupons and discount codes (single- or multi-use, expiry, generated in batch) | OD (coupons, discount codes), SH, OB (coupons) | DF | V1 | |
| DSC-007 | Buy X get Y | OD | DF | V1 | |
| DSC-008 | Tiered or volume promotions (qty breaks, spend thresholds) | LS-X (tiered promotions), OD (pricelist min qty) | DF | V1 | |
| DSC-009 | Mix & match / bundle price ("any 3 for 1000 DA") | LS-X (advanced promotions)†, OB (complex baskets) | DF | V1 | |
| DSC-010 | Next-order coupon printed on the receipt | OD | DF | V2 | |
| DSC-011 | Scheduled promotions (date range, days of week, hours) | KO, OD (date ranges), LS-K | DF | V1 | Ramadan and seasonal promos (confirm in DZ docs) |
| DSC-012 | Happy hour (time-based price list) | LS-K (order profile + scheduled price list), FL (shift-wise pricing) | DF | V1 | |
| DSC-013 | Customer-group discounts | OD (pricelists), LS-R† | TS | M | |
| DSC-014 | Promotion stacking, priority and exclusivity rules | KO, OB (flexible rules) | DF | V1 | |
| DSC-015 | Discount tags via barcode (scan a "-20%" sticker) | OD | DF | V1 | Clearance items |
| DSC-016 | Multi-location promotions | KO | AD | V2 | |
| DSC-017 | Discount reports (by reason and by cashier) | SH (discount reports), KO (discount monitoring by cashier) | DF | M | |
| DSC-018 | Receipt deals (threshold discount on the total) | KO (receipt deals) | DF | V1 | |
| DSC-019 | "You saved X DA" on receipt and customer display | — | DF | M | |
| DSC-020 | Free item / gift with purchase | OD (rewards)†, SB (gift) | DF | V1 | |

---

### E. Taxes

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| TAX-001 | Several tax rates; tax per product or category | OD, LV (tax report), OS (VAT/GST), SH (taxes) | TS | M | DZ TVA rates (19% / 9% / exempt): confirm in DZ docs |
| TAX-002 | Tax-inclusive (TTC) or tax-exclusive (HT) pricing | OD†, SQ†, LS-X† | TS | M | Show HT/TVA/TTC on invoices |
| TAX-003 | Compound or multiple taxes on one line | OS (multi-tier taxation), OD† | AD | V1 | |
| TAX-004 | Fiscal positions (tax mapping by customer type, eat-in vs takeaway) | OD | DF | V1 | |
| TAX-005 | Tax-exempt customers | OS (customer taxation), OD (fiscal position) | DF | M | |
| TAX-006 | Tax report by rate and period | LV (tax report), SH (finance reports) | TS | M | For G50 declaration prep (confirm in DZ docs) |
| TAX-007 | Stamp duty (droit de timbre) on cash-paid invoices | none of the global products | TS (DZ) | M | DZ-specific. Confirm rules in DZ docs. |
| TAX-008 | Company and customer legal IDs on documents (RC, NIF, NIS, AI, RIB) | — (custom fields in OD/DT†) | TS (DZ) | M | |
| TAX-009 | Tax rounding per line or per total | OD† | TS | M | |
| TAX-010 | E-invoicing / fiscal-device certification / multi-country compliance | OB (e-invoicing, certifications, finance law) | AD | watch | Watch for any DZ e-invoicing obligation |
| TAX-011 | Businesses not subject to VAT (flat-tax regimes); hide TVA lines | — | TS (DZ) | M | Confirm IFU/auto-entrepreneur rules in DZ docs |

---

### F. Receipts, invoices and document templates

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| DOC-001 | Receipt header/footer, logo, contact info, social handles | UC (customizable ticket and receipt layouts), SH (custom printed receipts, Pro), LV† | TS | M | |
| DOC-002 | Visual receipt and template designer (58/80 mm thermal, A4, A5) | UC (highly customizable layouts), DT (templates requested in issue #12989) | DF | V1 | A drag-and-drop designer would be a strong DF |
| DOC-003 | Customer address and notes printed on the receipt | LV | DF | M | |
| DOC-004 | Digital receipts: email / SMS / WhatsApp link or PDF | SH, ZO (WhatsApp), LV, UC (digital receipts) | DF | V1 | |
| DOC-005 | Full commercial document set: devis (quote), facture proforma, bon de commande, bon de livraison, facture, avoir, bon de réception, bon de retour, bon de transfert | OD, OS (receipt/invoice/quotation print and email), DT, ZO | TS (gestion commerciale) | M | |
| DOC-006 | Bilingual Arabic/French documents with RTL layout | — (OD supports RTL languages†) | TS (DZ) | M | |
| DOC-007 | Amount in words (French/Arabic) on invoices | — | TS (DZ) | M | Confirm in DZ docs |
| DOC-008 | Barcode or QR code of the receipt number printed on the receipt (for returns) | —† | DF | M | |
| DOC-009 | Kitchen/bar tickets routed by product category to specific printers | OD (category-based routing), LV (kitchen printers), UC, CH, FL | TS (resto) | M | |
| DOC-010 | Pre-bill (addition) before payment | OD (early receipt printing), SB (Print Bill) | TS (resto) | M | |
| DOC-011 | Label templates: shelf labels, barcode labels, ZPL/EPL for thermal label printers | OD 19 (four default ZPL formats), LV (label printing), SQ (label sizes), KO (price and shelf labels), OS (barcode printing) | TS | M | |
| DOC-012 | Numbering sequences per document type, year and register | OD†, DT† | TS | M | Gap-free numbering for invoices |
| DOC-013 | PDF export and print preview of every document | OD†, ZO† | TS | M | |
| DOC-014 | "DUPLICATA" / copy marker on reprints | — | DF | M | |
| DOC-015 | Custom fields on documents (e.g., vehicle plate, warranty months) | SH (customer metafields) | AD | V1 | |
| DOC-016 | Terms, return policy or warranty text on receipts | — | TS | M | |
| DOC-017 | Batch print or batch email of documents (monthly statements) | — | DF | V1 | |
| DOC-018 | Customer statement of account (relevé) | OD (customer statement) | TS (DZ) | M | |

---

### G. Product catalog: products, variants, modifiers, units, composites, kits, serials, lots, expiry

#### G.1 Basics

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-001 | Unlimited products and SKUs | SH (unlimited), LV† | TS | M | Target 100k+ SKUs with fast search |
| PRD-002 | Hierarchical categories (parent/child) | OD (parent-child PoS categories), SH (collections) | TS | M | |
| PRD-003 | Several barcodes per product | OD†, LS-R† | TS | M | |
| PRD-004 | Automatic SKU / internal EAN-13 generation | SQ (auto-create items and SKUs), ZO (SKU generator), OS (barcode generation) | TS | M | |
| PRD-005 | Product images | SQ (image upload, auto-polish), SH | TS | M | |
| PRD-006 | Brand, supplier reference, tags, extensible custom attributes | OS (extensible attributes), ZO (attributes) | TS | M | |
| PRD-007 | Product types: stockable / consumable / service / non-inventory | OD†, ZO† | TS | M | |
| PRD-008 | Active/archived; "available in POS" toggle | OD | TS | M | |
| PRD-009 | Categories restricted per register | OD (Restrict Categories) | DF | V1 | |
| PRD-010 | Long description, allergens, nutritional info | OD 19 (allergens), SH (product descriptions) | DF | V1 | |
| PRD-011 | Multilingual product names (AR/FR/EN) | ZO (multi-language billing)† | DF (TS in DZ) | M | |

#### G.2 Variants and modifiers

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-020 | Variants (matrix of attributes; own SKU, barcode, price, cost and stock per variant) | LV, SH, LS-R, LS-X, ZO (item groups), HK (variant-level inventory) | TS | M | Clothing and shoe shops |
| PRD-021 | Modifiers / add-ons (priced, grouped, min/max, forced) | LV, SQ (forced modifiers) | TS (resto) | M | |
| PRD-022 | Conversational modifiers (no / extra / light / on the side) | SQ | DF | V1 | |
| PRD-023 | Modifiers that deduct ingredient stock | LV (production)† | AD | V1 | |

#### G.3 Units of measure

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-030 | Sell by weight, length or volume (decimal quantities) | LV (weight barcodes), UC (scales) | TS | M | |
| PRD-031 | Several units with conversion (buy a carton of 24, sell by piece) | OD 19 (packaging + UoM merged), ZO†, EN† | TS (DZ wholesale) | M | Carton / fardeau / pièce |
| PRD-032 | Packaging barcodes (scanning the carton adds 24 units) | OD (packagings) | DF | M | |
| PRD-033 | Different price per unit vs per pack | OD† | DF | M | |

#### G.4 Composites, kits, recipes, production

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-040 | Composite or bundle items with stock derived from components | LS-X (composite), LV (composite items), ZO (composite), OR (kitting), LS-R (bundles) | DF | M | |
| PRD-041 | Recipes / BOM (ingredients deducted on sale) | LV (production), OR (BOMs), OD† (MRP) | DF | V1 | Pizzerias, bakeries |
| PRD-042 | Production or assembly orders | LV (production), OR (manufacturing), ZO (assemblies) | AD | V1 | |
| PRD-043 | Combos / menus | OD (product combos) | DF | V1 | |

#### G.5 Serials, lots, expiry

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-050 | Serial number tracking (IMEI), captured on receipt and on sale | OD, LS-R, ZO, DT | DF | M | Phone and electronics shops |
| PRD-051 | Lot / batch tracking | OD, ZO, DT, EN† | DF | V1 | |
| PRD-052 | Expiry dates (DLC/DLUO), sell-by and eat-by | DT (sell-by / eat-by), ZO (batch expiration), OD 19 (expired in forecast) | DF | V1 | Groceries, parapharmacy, cosmetics |
| PRD-053 | FEFO suggestion and near-expiry alerts | ZO†, OD† | DF | V1 | |
| PRD-054 | Warranty per serial; find a serial across all locations | LS-R (serial search across locations) | DF | V1 | |

#### G.6 Price and cost fields

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-060 | Cost, sale price, margin and markup auto-calculation | LS-R (markup vs margin; default vs unit cost), SQ (unit costs) | TS | M | |
| PRD-061 | Several sale prices per product (retail / semi-wholesale "demi-gros" / wholesale) | OD (pricelists), LS-R (price books) | TS (DZ) | M | |
| PRD-062 | Minimum sale price (floor), with a warning below cost | — | DF | M | |
| PRD-063 | Bulk price update (± % by category, supplier or brand) | LS-R (bulk price updates) | DF | M | Inflation / price-change days |
| PRD-064 | Price and cost history per product | — | DF | V1 | |

#### G.7 Catalog tools

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRD-070 | Bulk edit (spreadsheet-like grid) | SQ (bulk edit), KO (mass editing) | DF | M | |
| PRD-071 | Duplicate a product | —† | TS | M | |
| PRD-072 | Product QR codes | SH | AD | V2 | |
| PRD-073 | Electronic shelf labels sync | OD | AD | opt | |
| PRD-074 | Shared catalog across stores with per-store price and stock overrides | LS-R, SH | DF | V2 | |
| PRD-075 | Merge duplicate products | — | DF | V1 | Useful after an import |

---

### H. Pricing tiers and price lists

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PRC-001 | Named price lists assigned to customers or customer groups | OD, ZO (price lists), LS-K (price lists) | DF | M | |
| PRC-002 | Quantity-break pricing (min qty) | OD (min quantity) | DF | M | |
| PRC-003 | Date-bounded prices | OD (date ranges) | DF | V1 | |
| PRC-004 | Formula price lists (cost + x %, or derived from another list − y %) | OD† | DF | V1 | |
| PRC-005 | Supplier (purchase) price lists | ZO (vendor price lists), OD† | DF | V1 | |
| PRC-006 | Time-of-day price list (happy hour) | LS-K | DF | V1 | |
| PRC-007 | Switch menu or price list per service (lunch / dinner / terrace) | SB (swap menus and price lists), FL (shift-wise pricing) | DF | V1 | |
| PRC-008 | Batch-specific pricing | ZO (batch pricing) | AD | V2 | |
| PRC-009 | Hide cost and margin from cashiers | KO (permissions) | TS | M | |
| PRC-010 | Switch price list on the fly at the POS | OD† | DF | M | |

---

### I. Inventory

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| INV-001 | Stock decremented in real time on sale and incremented on return | DT (automatic stock decrease), EN (real-time), LV, SQ, OD | TS | M | |
| INV-002 | Stock on hand per location / warehouse | OD, ZO, OR, LS-R, SH, DT | TS | M | |
| INV-003 | Several warehouses (shop floor + depot + van) | ZO (multi-warehouse), OR, DT, OD, KO | TS (DZ) | M | Shop + "dépôt" is very common (confirm in DZ docs) |
| INV-004 | Bin / shelf locations | ZO (bin locations), DT (emplacements), OR (locations on pick list) | AD | V1 | |
| INV-005 | Transfers between locations (send, in-transit, receive; transfer document) | LV (transfer orders), SQ (Premium), SH (send/receive), ZO, KO, OB | TS | M | |
| INV-006 | Stock adjustments with reason codes (damage, loss, theft, expiry, own use, correction) | LV (tracked reasons), LS-R (custom adjustment reasons) | TS | M | |
| INV-007 | Full and partial (cycle) stock counts | LV (full or partial counts), SH (inventory counts, Pro), SQ (camera counting, Plus) | TS | M | |
| INV-008 | Counting by barcode scanner, printable count sheets, blind count (expected qty hidden) | LV (scanner or manual) | DF | M | Blind count is anti-fraud |
| INV-009 | Count reconciliation (expected vs counted, variance qty and value, approve and post) | LV (reconcile expected vs actual) | TS | M | |
| INV-010 | Keep selling during a count (snapshot at count start, sales delta applied) | — | DF | V1 | Hard to do well. Big DF. |
| INV-011 | Collaborative count from several devices, merged | LS-R (mobile-friendly counts)† | DF | V2 | Android scanner app over LAN |
| INV-012 | Low-stock alerts and daily notifications | LV (daily), SQ (daily stock alerts), SH, ZO, KO, OR | TS | M | |
| INV-013 | Reorder point / min-max per item and location | LS-R (reorder points, min/max), SH, ZO, OD (reordering rules) | TS | M | |
| INV-014 | Reorder suggestions / auto-generated draft POs | LS-R (smart reorder recommendations), SQ (automated restocks), KO (auto ordering) | DF | V1 | |
| INV-015 | Demand forecasting | SH, LS-R, OD 19 (forecast-based purchasing) | DF | V2 | Seasonality (Ramadan, Aïd, back-to-school) |
| INV-016 | Inventory rebalancing across stores | SH | AD | V2 | |
| INV-017 | Movement ledger per item (every change: who, when, why, document link) | LV (inventory history), DT (movement logs), SH (historical adjustments), SQ (inventory history) | TS | M | |
| INV-018 | Inventory valuation report (cost value, retail value, potential profit) | LV, OD (valuation dashboard) | TS | M | |
| INV-019 | Costing methods: weighted average (CMUP / PMP / AVCO), FIFO, standard; LIFO in ERPNext | OD (Standard/AVCO/FIFO), LS-R (Average/FIFO), EN (FIFO/Moving average/LIFO), DT (PMP/standard), ZO (FIFO cost lot tracking) | DF | M (CMUP) / V1 (FIFO) | CMUP is the usual method in French-style accounting (confirm in DZ docs) |
| INV-020 | Landed costs (freight, customs, fees) allocated to received items | OD (FIFO/AVCO only), LS-R (landed costs, distributed costs) | DF | V1 | Key for importers |
| INV-021 | Negative-stock policy: allow / warn / block, per product or global | LV (negative stock alerts), EN† | TS | M | |
| INV-022 | Committed / reserved stock (orders, layaways, special orders) | ZO (committed stock), LS-R (reservations by layaway/special order), HK (layby updates available inventory) | DF | V1 | |
| INV-023 | Virtual / forecast stock (on hand + incoming − reserved) | DT (virtual stock), OD 19 (forecasted report) | DF | V1 | |
| INV-024 | Inventory ageing / dead-stock report | ZO (inventory aging summary), SQ (aging inventory) | DF | V1 | |
| INV-025 | ABC analysis | KO (ABC inventory grading) | DF | V1 | |
| INV-026 | Sell-through report | SQ, KO | DF | V1 | |
| INV-027 | GMROI and turnover | KO | AD | V2 | |
| INV-028 | Back-orders (sell out-of-stock, fulfil later) | ZO (back orders), LS-R (special orders) | DF | V1 | |
| INV-029 | Drop-shipping | ZO, OR (CSV/EDI/API routing) | AD | n/a | |
| INV-030 | Pick lists, packing slips, shipping labels | ZO (picklists, packaging and shipping), OR | AD | V2 | |
| INV-031 | Internal consumption / own use | — (reason codes in LV/LS-R) | TS (DZ) | M | |
| INV-032 | Waste and breakage tracking | — | TS (resto) | V1 | |
| INV-033 | Multi-line transfer or adjustment documents | DT (several transfers in one transaction), SQ (bulk transfers, Plus) | TS | M | |
| INV-034 | Opening-stock import | LV (import items)† | TS | M | |
| INV-035 | Stock per variant and per lot or serial | HK (variant-level), DT (lot), OD, ZO | TS | M | |
| INV-036 | Inventory states (available, reserved, damaged, in transit) | SH (inventory states / status) | DF | V1 | |
| INV-037 | Channel reserve quantities (hold stock back from online) | OR (reserve quantities by channel) | AD | V2 | |
| INV-038 | Consignment stock (dépôt-vente) | — | AD | V2 | Unverified in the benchmark |
| INV-039 | Inventory period closing with backdating | OD 19 (simplified closing, backdating) | AD | V2 | |
| INV-040 | Supplier lead times | OR | AD | V1 | |
| INV-041 | Stock value at any past date (historical stock) | OD† | DF | V1 | |

---

### J. Purchasing and suppliers

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| PUR-001 | Supplier directory (contacts, tax IDs, payment terms, RIB) | SQ (vendor profiles), OS (supplier DB), ZO, LS-R | TS | M | |
| PUR-002 | Purchase orders (create, PDF/email, statuses draft, sent, partial, received) | LV (Advanced Inventory), SQ (Plus), LS-R, ZO, OD, KO, OR | TS | M | Free in DZBuild (paid add-on in LV/SQ) |
| PUR-003 | PO autofill from low stock / reorder suggestions | LV (smart autofill), LS-R (smart reorder), SQ (automated restocks) | DF | V1 | |
| PUR-004 | Add PO lines by scan or CSV import | LS-R (search, scan, import via CSV) | DF | V1 | |
| PUR-005 | Receive (full or partial) against a PO, updating costs | LS-R (costs update on receive), SH (receive inventory, Pro), KO (ease of receiving), OR (goods receipts) | TS | M | |
| PUR-006 | Direct goods receipt without a PO (bon de réception) | OS (receivings) | TS (DZ) | M | |
| PUR-007 | Supplier invoices (bills) and accounts payable | ZO (bills, unpaid bills), OD 19 (upload vendor bill from PO) | TS (DZ) | M | |
| PUR-008 | Supplier payments (partial) and supplier balance ("crédit fournisseur") | ZO (vendor payments) | TS (DZ) | M | |
| PUR-009 | Returns to supplier (bon de retour) | ZO†, OD† | TS | M | |
| PUR-010 | Landed costs on receipts | OD, LS-R | DF | V1 | |
| PUR-011 | Supplier price lists; last purchase price per supplier | ZO (vendor price lists) | DF | V1 | |
| PUR-012 | Several suppliers per product; preferred supplier | OD†, LS-R† | DF | V1 | |
| PUR-013 | Vendor performance reports | SQ | AD | V2 | |
| PUR-014 | Supplier catalog integration (NuORDER) | LS-R | AD | n/a | |
| PUR-015 | RFQ / alternative RFQs | OD 19 | AD | V2 | |
| PUR-016 | Foreign-currency purchases with exchange rate | OD 19 (vendor currency) | AD | V1 | Importers |
| PUR-017 | Print barcode labels straight from received lines | LV (labels for POs/counts) | DF | M | |
| PUR-018 | Expense logging (rent, power, transport) | OS (expenses logging) | DF | M | Simple P&L for small shops |
| PUR-019 | PO shipment tracking | KO (track shipping) | AD | V2 | |
| PUR-020 | EDI with suppliers | ZO (EDI), OR (EDI) | AD | n/a | |

---

### K. Customers, CRM, credit, loyalty, gift cards, store credit, layaway

#### K.1 CRM

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CRM-001 | Customer database (name, phone, email, address, legal IDs) | all | TS | M | Phone number is the main key in DZ |
| CRM-002 | Purchase history per customer | LV, SH (order history), ZO | TS | M | |
| CRM-003 | Customer notes and preferences | LV (notes), OD (customer notes) | TS | M | |
| CRM-004 | Customer groups, tags, segments | SH (tag and categorize, segmentation), OD† | DF | M | |
| CRM-005 | Customer custom fields | SH (customer metafields) | AD | V1 | |
| CRM-006 | Marketing consent / GDPR-style data handling | SH (marketing opt-ins), OS (GDPR) | AD | V1 | DZ Law 18-07 on personal data (confirm in DZ docs) |
| CRM-007 | Import / export customers (CSV) | LV | TS | M | |
| CRM-008 | Balance, points and debt visible at the POS | OD (customer profile from the register) | DF | M | |
| CRM-009 | Caller ID (open the customer on an incoming call) | SB (Caller ID) | AD | V2 | Phone-order restaurants |
| CRM-010 | Customer feedback | CL | AD | V2 | |
| CRM-011 | Email / SMS campaigns | CL, OS (SMS, MailChimp), SH (email marketing) | AD | V2 | Via DZBuild cloud |
| CRM-012 | Customer analytics (top customers, frequency, RFM) | SH (customer reports), LV (customers base) | DF | V1 | |
| CRM-013 | Birthday rewards and reminders | —† | AD | V2 | |
| CRM-014 | Duplicate detection and merge | — | DF | V1 | |
| CRM-015 | Clienteling (profile + recommendations at the counter) | OB | AD | V2 | |

#### K.2 Customer credit / on-account ("crédit client")

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CRD-001 | Sell on account with a credit limit | OD, LS-X, HK | TS (DZ) | M | |
| CRD-002 | Credit-limit behaviour: warning or hard block (configurable) | OD (warning only) | DF | M | |
| CRD-003 | Settle debt (pick sales or invoices, partial payment) | OD (settle orders / invoices) | TS (DZ) | M | |
| CRD-004 | Customer statement and receivables ageing | ZO (receivables), OD (customer statement) | TS (DZ) | M | |
| CRD-005 | Deposits and advances (acompte) | OD (deposit money; down payments), HK | TS | M | |
| CRD-006 | Debt reminders by WhatsApp or SMS with a statement link | — | DF | V1 | DZBuild differentiator |
| CRD-007 | Payment terms and due dates | ZO†, OD† | DF | V1 | |
| CRD-008 | On-account sales counted in the sales report at creation (vs layaway, counted when paid) | LS-X (documented difference) | TS | M | Accounting semantics matter |

#### K.3 Loyalty

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| LOY-001 | Points programme (earn per DA, redeem) | LV (free), OD, KO, HK, CL, OS | DF | V1 | |
| LOY-002 | Loyalty cards with barcode | LV, SB (SambaCard) | DF | V1 | |
| LOY-003 | Per-product point values or multipliers | HK | AD | V2 | |
| LOY-004 | Tiers (silver / gold) | —† | AD | V2 | |
| LOY-005 | Rewards (free product, discount) | OD (rewards) | DF | V1 | |
| LOY-006 | Loyalty works offline | gap: SQ loyalty is unavailable offline (S14) | DF | M (design rule) | |
| LOY-007 | Loyalty unified across stores and online | SH (unified loyalty) | AD | V2 | Via DZBuild sync |

#### K.4 Gift cards and store credit

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| GFT-001 | Sell and redeem physical gift cards (barcode) | SH, OD, HK, KO, CL (70+ templates), OS | DF | V1 | |
| GFT-002 | Digital eGift cards | HK, CL, SH (Apple Wallet) | AD | V2 | |
| GFT-003 | Balance check, reload, expiry | —† | DF | V1 | |
| GFT-004 | Issue store credit (from returns) and redeem it | LS-X, HK, KO | TS | M | |
| GFT-005 | Gift card / store-credit liability report | —† | AD | V1 | |

#### K.5 Layaway, special orders, work orders, quotes

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| LAY-001 | Layaway (deposit, stock reserved, instalments; counted as a sale only when fully paid) | LS-X, LS-R, HK | DF | M | Common for furniture, electronics and wedding goods (confirm in DZ docs) |
| LAY-002 | Special orders (sell out-of-stock, order it, notify the customer; statuses Not Ordered → Ready for Pickup) | LS-R | DF | V1 | |
| LAY-003 | Work orders / repairs (device intake, parts, labour, assigned tech, status, customer notification) | LS-R (work orders) | DF | V1 | Phone-repair shops are numerous (confirm in DZ docs) |
| LAY-004 | Quotations converted into sales or invoices | OD, OS | TS | M | |
| LAY-005 | Draft orders | SH (Pro) | DF | M | |
| LAY-006 | Reservations / holds | LS-R (reservations), OB (reservation) | DF | V1 | |
| LAY-007 | Appointment booking | SH (appointment booking), OD (table booking) | AD | V2 | |

---

### L. Employees, roles and permissions

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| EMP-001 | User accounts with PIN or password | SH (staff PINs), OD | TS | M | |
| EMP-002 | Roles with granular permissions (per action: refund, discount > x %, price override, open drawer, delete line, see cost, run reports, export, settings) | SH (roles and permissions, Pro), KO (cashier roles), UC (Admin/Manager/Clerk/Guest), LV (access rights), OS (permission controls), CL | TS | M | |
| EMP-003 | Manager override (one-time approval by manager PIN) | SH (manager approvals) | DF | M | |
| EMP-004 | Minimal-rights cashier profile | OD 19 | DF | M | |
| EMP-005 | Time clock (clock in/out, hours) | LV, SQ, KO, CL, EP | DF | V1 | |
| EMP-006 | Scheduling and time-off | CL | AD | V2 | |
| EMP-007 | Sales by employee | LV, SH (staff reports), KO | TS | M | |
| EMP-008 | Commissions | SH (Pro), KO (commission tracking) | DF | V1 | |
| EMP-009 | Payroll | SH (payroll sync), CL, SQ | AD | n/a | |
| EMP-010 | Hourly workload analysis | LV (smart management) | AD | V2 | |
| EMP-011 | Restrict users to specific registers or stores | OD†, EN† | DF | V1 | |
| EMP-012 | Badge / RFID / iButton login | UC (iButton/Dallas keys), OD† | DF | V1 | |
| EMP-013 | Staff tasks | LS-K (Tasks) | AD | V2 | |
| EMP-014 | Employee cash advance from the till | — | DF (DZ) | V1 | |
| EMP-015 | Unlimited staff accounts (free) | SH (unlimited, Pro); LV charges per employee for employee management (S32) | DF | M | DZBuild: unlimited, free |

---

### M. Cash management

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| CSH-001 | Opening float count | SQ, LV, LS-X, EN, OD | TS | M | |
| CSH-002 | Cash in / cash out (pay in / pay out) with a reason and note | SQ, LV, LS-X (notes appear in the closure report), OD† | TS | M | |
| CSH-003 | Expected vs counted cash, with the variance shown | LV, SQ, LS-X, OD | TS | M | |
| CSH-004 | Blind close (expected cash hidden from staff) | LS-X (hide expected cash), KO (blind balance) | DF | M | |
| CSH-005 | Acceptable-difference threshold / forced recount / manager sign-off | KO (customizable acceptable differences) | DF | M | |
| CSH-006 | Denomination counting helper (qty × banknote/coin) | —† | DF | M | |
| CSH-007 | X report (mid-shift, no reset) and Z report (end of day, sequential, not editable) | LV (shift reports), SQ (drawer report), KO (drawer closing), UC† | TS | M | |
| CSH-008 | Reconciliation per payment method (cash, card, cheque, transfer) | EN (closing entry with expected vs closing per mode), LS-X (cash and credit totals) | TS | M | |
| CSH-009 | Blind cash drops / safe drops during a shift | KO (blind cash drops) | DF | V1 | |
| CSH-010 | Force drawer closed before the next action; no-sale opens limited and logged | KO (force drawer closing, no-sale tender prevention) | DF | M | |
| CSH-011 | Shift report pushed to the owner (email / WhatsApp / DZBuild) | LV (email notifications of new shift reports) | DF | V1 | |
| CSH-012 | Cash movements report (filter, export) | LS-X | DF | M | |
| CSH-013 | End a drawer remotely from the back office | SQ | AD | V2 | |
| CSH-014 | Cash control per day, month and terminal | DT | TS | M | |
| CSH-015 | Expense payouts from the till by category | OS (expenses), LV (pay out for small expenses) | TS (DZ) | M | |
| CSH-016 | Bank deposit tracking | — | AD | V1 | |
| CSH-017 | Several drawers per register / drawer per cashier | —† | AD | V2 | |
| CSH-018 | Tip pooling | —† | AD | n/a | |

---

### N. Reporting and analytics

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| RPT-001 | Home dashboard (today, trend, top items) | SH (dashboard overview), LV (sales trend) | TS | M | |
| RPT-002 | Sales summary by period | all | TS | M | |
| RPT-003 | Sales by item, category and variant | ZO (sales by item), LV (popular items), SH (product reports) | TS | M | |
| RPT-004 | Sales by customer | ZO | TS | M | |
| RPT-005 | Sales by employee | LV, SH, KO | TS | M | |
| RPT-006 | Sales by payment method | LV†, SQ† | TS | M | |
| RPT-007 | Sales by hour and weekday (heatmap) | LV (hourly workload), KO (time-based comparisons) | DF | V1 | |
| RPT-008 | Profit, margin and COGS | SQ (COGS, Plus), LS-R (margins and COGS), LV (valuation) | TS (DZ) | M | Free in DZBuild (paid tier in SQ) |
| RPT-009 | Discounts report | SH, KO | DF | M | |
| RPT-010 | Tax report | LV, SH | TS | M | |
| RPT-011 | Returns and refund-rate report | KO (return rates) | DF | M | |
| RPT-012 | Voids, cancellations and no-sale report | KO | DF | M | |
| RPT-013 | Inventory reports (on hand, valuation, low stock, ageing, sell-through, ABC, turnover) | SQ, KO, ZO, LV | TS/DF | M/V1 | |
| RPT-014 | Purchase reports | ZO (purchase analytics) | TS | M | |
| RPT-015 | Receivables and payables (debts) | ZO (receivables) | TS (DZ) | M | |
| RPT-016 | Shift / Z-report archive | LV (shifts), LS-X (register closure report) | TS | M | |
| RPT-017 | Export every report to XLSX / CSV / PDF | LV (reports export), LS-X (cash movement export) | TS | M | |
| RPT-018 | Custom report builder | SB (custom reports), SH (custom inventory reports) | DF | V2 | |
| RPT-019 | Period comparison (vs previous period or year) | LV (sales trend), KO | DF | M | |
| RPT-020 | Average basket, items per basket, conversion | KO (average transactions, conversion rates) | DF | V1 | |
| RPT-021 | Preparation-time report | OD 19 | AD | V2 | |
| RPT-022 | Consolidated multi-store reports | OD (consolidated across shops), SH, CL (multi-location dashboard) | DF | V2 | Via DZBuild cloud |
| RPT-023 | Owner mobile live sales | LS-K (Pulse) | DF | V2 | DZBuild web/mobile dashboard |
| RPT-024 | Breadth benchmark: 50+ built-in reports | UC (50+ reports) | — | M target | |
| RPT-025 | Scheduled / emailed reports | LV (shift report emails) | DF | V2 | |
| RPT-026 | Peer benchmarks | LS-K (Benchmarks and Trends) | AD | V2 | |
| RPT-027 | Simple P&L (sales − COGS − expenses) | OS (expenses) | DF (DZ) | V1 | |
| RPT-028 | Franchise royalties | KO (manage royalties) | AD | n/a | |
| RPT-029 | Unlimited sales history (no paywall) | LV limits free analytics to 31 days; history is a paid add-on (S32) | DF | M | DZBuild: unlimited, local |

---

### O. Restaurant and hospitality

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| RST-001 | Floor plans (rooms, drag-and-drop tables, shapes, seats) | OD, LS-K, DT (floors/rooms/tables drag-and-drop), FL (floor plan plugin), OS (restaurant tables) | TS (resto) | M (resto mode) | |
| RST-002 | Live table status (free, occupied, bill printed, time since order) | OD | TS | M | |
| RST-003 | Open tickets per table; predefined tickets | LV (predefined tickets), SB (entities) | TS | M | |
| RST-004 | Transfer an order between tables; merge tables | OD (order transfer, merge) | TS | M | |
| RST-005 | Split bill by item, by seat, or evenly (covers) | OD (bill splitting), LS-K (seat / covers), SB (split ticket), FL (split tickets) | TS | M | |
| RST-006 | Merge split checks back | LS-K | DF | V1 | |
| RST-007 | Seat numbers and guest counts | LS-K, SQ (seat management) | DF | V1 | |
| RST-008 | Courses and course firing (hold / fire, automatic or server-led) | TO, SQ (course management), LS-K, OD 19 | DF | V1 | |
| RST-009 | Kitchen and bar printers by category, with cancellation tickets | OD (automatic cancellation printing), LV, UC, CH, FL | TS | M | |
| RST-010 | Kitchen display (timers, colour by station, routing, bump, recall) | TO, SQ (routing and timers), LV (KDS, free), OD (preparation display), CH (Kitchen Screen, up to 9 locations, recall last order), UC (display monitors) | DF | V1 | A second PC or Android tablet on the LAN |
| RST-011 | 86 items (out-of-stock toggle synced everywhere; auto-86) | TO | DF | V1 | |
| RST-012 | Bar tabs (pre-authorised card) | LS-K | AD | V2 | Tabs yes; card pre-auth is n/a |
| RST-013 | Dine-in / takeaway / delivery order types | LV (dining options), FL (takeout), OD | TS | M | |
| RST-014 | Delivery management (address, driver, fee, status) | SB (delivery service), FL (delivery plugin) | DF | V1 | |
| RST-015 | Delivery-aggregator integration (UrbanPiper, Uber Eats, DoorDash, GrabFood/GoFood) | OD, EP | AD | watch | Local aggregators: confirm in DZ docs |
| RST-016 | QR self-ordering at the table / kiosk | OD (self-ordering kiosk/mobile), DT (auto order by QR), LS-K (Order Anywhere), EP (Order and Pay) | DF | V2 | Can be served from the LAN |
| RST-017 | Table reservations | OD (table booking), LS-K (Reservations) | AD | V2 | |
| RST-018 | Allergens | OD 19 | AD | V2 | |
| RST-019 | Happy-hour pricing | LS-K | DF | V1 | |
| RST-020 | Void / comp / gift with reason | SB (void, gift) | TS | M | |
| RST-021 | Waiter handheld ordering (tableside) | LS-K (Tableside), TO† | DF | V2 | Android waiter app over LAN |
| RST-022 | Pizza builder / half-and-half | FL† | AD | V2 | Unverified |
| RST-023 | Recipe and ingredient deduction | LV (production), SB† | DF | V1 | |
| RST-024 | Different tax for eat-in vs takeaway | OD (fiscal positions) | AD | V1 | |
| RST-025 | Auto-gratuity for large parties | SQ | AD | n/a | |
| RST-026 | Event → action automation engine | SB (actions, rules, automation commands) | AD | V2 | |
| RST-027 | Order-ready screen / pager / order number call | —† | DF | V2 | |
| RST-028 | Separate kitchen and bar preparation displays | OD (preparation display), TO (by station) | DF | V1 | |

---

### P. Omnichannel and orders beyond the counter

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| OMN-001 | Sales orders from the POS, fulfilled later | OD | DF | V1 | |
| OMN-002 | Click and collect / pickup in store | SH (Pro), OB, UC (click and collect routing), EP | AD | V2 | Via a DZBuild storefront |
| OMN-003 | Ship from store / buy in store, ship home | SH, OB | AD | V2 | |
| OMN-004 | Local delivery | SH (Pro) | AD | V1 | Delivery notes |
| OMN-005 | Online store sync (catalog, stock, orders) | SH, ZO (online store), LS-R (eCom), SQ, KO | DF | V2 | **Key DZBuild differentiator** (dzbuild.dev link) |
| OMN-006 | Marketplace integrations | SH, ZO (Etsy, Shopify, WooCommerce), LS-R | AD | V2 | |
| OMN-007 | Shipping-carrier integration and labels | ZO (UPS, FedEx, Easyship…), OR | AD | V2 | Local couriers: confirm in DZ docs |
| OMN-008 | Unified returns across channels | OB, SH | AD | V2 | |
| OMN-009 | Virtual shopping / social selling | SH | AD | n/a | |
| OMN-010 | Invoices with payment follow-up | CL (invoicing), ZO | AD | V1 | |

---

### Q. Offline mode and sync

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| OFF-001 | Keep selling with no internet | LV, SQ (offline payments), ZO (offline billing), CL, TO, KO, HK, OB, OD ("even if temporarily offline"), EN (offline setting / community apps); UC, CH, FL, SB are fully local | TS | M | DZBuild design: 100 % local-first |
| OFF-002 | **Everything** works offline: stock levels, reports, loyalty, gift cards, closing, printing | UC, CH, FL, SB (local DB). **Gaps:** LV shows no stock levels or negative-stock alerts offline (S33); SQ has no gift cards, loyalty or close-of-day offline (S14); TO offline "has significant limitations" (S36) | DF | M | Core positioning |
| OFF-003 | Automatic background sync when online | LV, KO (automatic sync), SQ (automatic upload) | TS | V1 | Sync to the DZBuild cloud |
| OFF-004 | Manual sync button and clear sync status | ZO (manual sync) | TS | V1 | |
| OFF-005 | Conflict resolution (stock as deltas, per-field merge, event log) | — | DF | V1 | Engineering: an append-only event log makes this tractable |
| OFF-006 | Offline card limits and expiry windows | SQ (per-transaction limits, 24/72 h) | — | n/a | |
| OFF-007 | Several terminals on a LAN with no internet (local server or peer) | UC, CH (shared MySQL/PostgreSQL), SB (SQL Server), FL† | TS (DZ) | M | Supermarket with 3–6 tills |
| OFF-008 | HQ pushes master data to stores; stores push transactions to HQ | OB (centralised store management), KO (transfer and sync data) | DF | V2 | |
| OFF-009 | Embedded local DB, crash-safe (WAL), survives power cuts | UC, CH (Derby embedded), FL (embedded DB) | TS | M | Power cuts happen (confirm in DZ docs); needs transactional writes |
| OFF-010 | Wired-network fallback | TO (hardwired) | — | n/a | |

---

### R. Multi-store and multi-terminal

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| MST-001 | Several stores under one account | LV (free), SQ, SH, LS-R, LS-X, ZO, CL, KO | TS | V2 | Via DZBuild OAuth link |
| MST-002 | Per-store settings (receipt, taxes, prices) | —† | TS | V2 | |
| MST-003 | Central catalog push | OB, KO | DF | V2 | |
| MST-004 | Inter-store transfers | see INV-005 | TS | V2 | |
| MST-005 | Consolidated reporting | OD, CL, SH | DF | V2 | |
| MST-006 | Franchise mode (royalties, franchise inventory) | KO | AD | n/a | |
| MST-007 | Per-location performance insights | ZO | DF | V2 | |
| MST-008 | Several legal entities (multi-company) in one install | OD†, DT† | AD | V2 | |
| MST-009 | Unlimited registers and stores, free | LV (multi-store free); ZO limits registers by plan; SQ puts transfers in Premium | DF | M | |

---

### S. Hardware integrations

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| HW-001 | ESC/POS thermal receipt printers (USB, serial, LAN, Bluetooth; 58/80 mm) | UC (Epson, Star, compatibles), OD (ePOS printers), DT (ESC/POS over TCP/IP, LPT, CUPS), CH (USB printers), LV | TS | M | Xprinter-type clones are common (confirm in DZ docs) |
| HW-002 | Windows-driver printing fallback (any printer) | DT (browser print) | TS | M | |
| HW-003 | A4/A5 printers for invoices and quotes | — | TS | M | |
| HW-004 | Network kitchen printers | OD, LV, CH, FL | TS | M | |
| HW-005 | Barcode scanners (HID keyboard wedge, serial/COM) | all | TS | M | |
| HW-006 | Cash drawer (printer RJ11 kick or USB/serial) | LV, FL, UC, CL | TS | M | |
| HW-007 | Customer display (VFD pole 2×20 or second monitor) | OD, UC, LV, KO | TS | M | |
| HW-008 | Scales (serial protocols, e.g., CAS, Dibal, Toledo) | OD (scale), UC | DF | V1 | |
| HW-009 | Label printers (ZPL / EPL / TSPL: Zebra, TSC, Xprinter label) | OD 19 (ZPL), LV (label printing), SQ | TS | M | |
| HW-010 | Label-printing scales (EAN-13 "2x" barcodes decoded at the POS) | CH (GS1), LV (weight barcodes) | DF | M | Parsing only |
| HW-011 | Integrated card terminals | OD, SQ, SH, CL, TO | TS (global) | opt | |
| HW-012 | Kitchen display screens | LV, TO, CH, OD | DF | V1 | |
| HW-013 | Handheld Android devices / data collectors for inventory | KO (mobile inventory), LS-R (mobile counts)† | DF | V2 | |
| HW-014 | Magnetic card / iButton / RFID login | UC | AD | V1 | |
| HW-015 | All-in-one touch terminals (min 1024×768) | UC, CH (from 1024×768), FL (touch UI) | TS | M | |
| HW-016 | Electronic shelf labels | OD | AD | opt | |
| HW-017 | Cash recyclers | OD | AD | n/a | |
| HW-018 | Local hardware bridge / IoT proxy | OD (IoT system), DT (TakePOS connector) | — | M | Built into the desktop app; no separate box |
| HW-019 | Caller-ID modem | SB | AD | V2 | |
| HW-020 | Hardware test page and diagnostics | — | DF | M | |
| HW-021 | Price-checker kiosk | —† | AD | V2 | |
| HW-022 | Runs on low-end hardware (old Windows PCs, Raspberry Pi) | FL (Raspberry Pi) | DF | M | Target Win10/11, 4 GB RAM (confirm in DZ docs) |

---

### T. Import and export

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| IMP-001 | Import products from CSV | LV, SQ, UC (category + product import), CH (CSV import), KO (data import), OR (bulk uploads) | TS | M | |
| IMP-002 | Import from Excel (.xlsx) | SQ (xlsx recommended) | TS (DZ) | M | Excel is the lingua franca of small businesses |
| IMP-003 | Import modes: add/update (upsert by SKU or barcode) vs replace all | SQ (Modify vs Replace) | DF | M | |
| IMP-004 | Undo the last import | SQ (Undo Catalogue) | DF | M | |
| IMP-005 | Downloadable import templates | SQ†, LV† | TS | M | |
| IMP-006 | Column-mapping UI, preview, per-row validation errors, dry run | SQ (import troubleshooting)† | DF | M | |
| IMP-007 | Large imports (30,000+ rows) | SQ (30,000 items per file) | TS | M | Target 100k |
| IMP-008 | Import customers | LV | TS | M | |
| IMP-009 | Import suppliers | —† | TS | M | |
| IMP-010 | Import opening stock and costs (per warehouse, lots, serials) | — | TS | M | |
| IMP-011 | Import opening customer and supplier balances (debts) | — | TS (DZ) | M | |
| IMP-012 | Import PO lines | LS-R | DF | V1 | |
| IMP-013 | Migration importers from other POS exports | — | DF | V1 | Local competitor formats: see DZ docs |
| IMP-014 | Export items | LV, SQ | TS | M | |
| IMP-015 | Export customers | LV | TS | M | |
| IMP-016 | Export receipts and sales lines | LV (receipts CSV) | TS | M | |
| IMP-017 | Export any list or report | LV (reports export), LS-X (cash movements export) | TS | M | |
| IMP-018 | Export for the accountant (sales/purchase journals, VAT ledger) | OD (accounting), SH (finance reports)† | DF (DZ) | V1 | |
| IMP-019 | Encoding and locale safety (UTF-8 with BOM for Excel, `;` separator for FR locale, Arabic text, leading zeros in barcodes) | SQ (warns that Excel mangles SKUs in CSV) | TS (DZ) | M | |
| IMP-020 | Bulk image import (folder/zip matched by SKU) | — | DF | V1 | |
| IMP-021 | Full data export in open formats (portability, "your data is yours") | — | DF | M | Trust builder for a free product |
| IMP-022 | Scheduled auto-export to a folder | — | AD | V2 | |

---

### U. Backup, restore and data safety

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| BCK-001 | Manual backup to a file | SB (Manage > Settings > Database Backups) | TS | M | |
| BCK-002 | Automatic scheduled local backups with rotation | SB (backup folder)† | TS | M | |
| BCK-003 | Backup to USB, external drive or network folder | — | TS | M | |
| BCK-004 | Encrypted cloud backup to the DZBuild account | — | DF | V1 | |
| BCK-005 | One-click restore with preview and integrity check | SB restore needs SQL Management Studio (shows the pain) | DF | M | |
| BCK-006 | Upgrade single-PC DB to a LAN multi-user DB | CH (Derby → MySQL/PostgreSQL) | AD | M | |
| BCK-007 | Point-in-time recovery / journal | — | AD | V2 | |
| BCK-008 | Automatic backup before every update or migration | — | TS | M | |
| BCK-009 | Integrity check and repair tools | — | DF | V1 | |
| BCK-010 | "Move to a new PC" wizard | SB (reinstall pain) | DF | M | |

---

### V. Audit log and loss prevention

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| AUD-001 | Tamper-resistant audit log of sensitive actions (void, line delete, discount, price override, refund, drawer open, settings change, login/logout) | KO (transaction auditing, view cashier actions), OS (transaction logging) | DF | M | |
| AUD-002 | Change history per record | DT (movement logs), SH (historical stock adjustments) | DF | M | |
| AUD-003 | Sequential receipts that cannot be deleted; cancellations are reversal documents | KO†, OB (fiscal certifications) | TS | M | |
| AUD-004 | Loss-prevention reports (voids, no-sales, refunds and discounts by cashier) | KO | DF | M | |
| AUD-005 | Hash-chained journal (tamper-evident, NF525-style) | OB (certifications)† | DF | V1 | |
| AUD-006 | CCTV overlay of transactions | —† | AD | n/a | |
| AUD-007 | Back-office access control | KO (Studio access control) | TS | M | |
| AUD-008 | Receipt voids require authentication | KO | DF | M | |

---

### W. Security

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| SEC-001 | Role-based access control | see EMP-002 | TS | M | |
| SEC-002 | Password hashing; PIN lockout after failed attempts | — | TS | M | |
| SEC-003 | Local DB encryption at rest | — | DF | V1 | |
| SEC-004 | Idle timeout / auto-lock | — | TS | M | |
| SEC-005 | TLS for all network traffic (LAN and cloud) | OD (HTTPS secure connection) | TS | M | |
| SEC-006 | Login captcha | OS (reCAPTCHA) | — | n/a | |
| SEC-007 | PCI-DSS (card data) | SQ, SH, UC (EMV/PCI) | — | n/a | Never store card data |
| SEC-008 | Personal-data compliance | OS (GDPR) | TS | M | DZ Law 18-07: confirm in DZ docs |
| SEC-009 | Code-signed installer and updates | — | TS | M | |
| SEC-010 | Free forever, no licence lock or kill switch | FL ("no locking, no hidden cost") | DF | M | |
| SEC-011 | 2FA on the DZBuild account | — | DF | V1 | |
| SEC-012 | Export permission (data exfiltration control) | — | DF | M | |

---

### X. APIs, webhooks, integrations

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| API-001 | REST API | LV, UC (REST API), OB (open APIs), OR (open API), ZO | DF | V2 | A local API plus the DZBuild cloud API |
| API-002 | Webhooks | LV (webhooks), ZO (webhooks and custom functions) | DF | V2 | |
| API-003 | Scripting / automation engine | UC (scripting), CH (scripting), SB (rules), ZO (Deluge) | AD | V2 | |
| API-004 | App marketplace | CL (App Market) | AD | n/a | |
| API-005 | Accounting integrations | EP (Xero), ZO (Books, Xero, QuickBooks), KO | AD | V2 | Local accounting software: see DZ docs |
| API-006 | E-commerce sync | see OMN-005 | DF | V2 | |
| API-007 | Plugin / module system | FL (plugins), DT (modules), OD (apps) | DF | V2 | |
| API-008 | OAuth account linking (DZBuild) | — | DF | V1 | |
| API-009 | Messaging (WhatsApp, SMS gateway) | ZO (WhatsApp invoices), OS (SMS), OD 19 (WhatsApp notifications) | DF | V1 | |

---

### Y. Accessibility and localization

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| L10N-001 | Multilingual UI | UC (17 languages), ZO (10+ billing languages), OS (multilingual), FL (localization) | TS | M | Arabic, French, English (Tamazight optional) |
| L10N-002 | Full RTL layout for Arabic | — | TS (DZ) | M | |
| L10N-003 | Document language chosen per document (receipt in AR, invoice in FR) | ZO (multi-language billing) | DF | M | |
| L10N-004 | Currency formatting (DA / DZD, decimal rules) | FL (currency configuration) | TS | M | |
| L10N-005 | Optional Hijri dates | — | AD | V2 | |
| L10N-006 | Dark theme | LV (dark theme) | DF | M | |
| L10N-007 | Large touch targets, font scaling | — | TS | M | |
| L10N-008 | Full keyboard-only operation | — | DF | M | |
| L10N-009 | High-contrast, colour-blind-safe status colours | — | DF | V1 | |
| L10N-010 | Screen-reader labels (WCAG 2.2 AA) | — | AD | V2 | |
| L10N-011 | Themes | OS (Bootswatch themes) | AD | V2 | |
| L10N-012 | Western Arabic numerals (0-9) in the Arabic UI by default | — | TS (DZ) | M | Confirm in DZ docs |

---

### Z. UI/UX patterns

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| UX-001 | Touch-first register with large tiles | UC, CH, FL, DT (touch and responsive) | TS | M | |
| UX-002 | Barcode-first flow (scanner input always captured, no clicks needed) | — | TS | M | |
| UX-003 | Keyboard shortcuts (F1–F12: pay, search, qty, discount, customer, park) | — | TS (DZ desktop) | M | |
| UX-004 | Layout customisable per user or register | UC, SB (screen customization), SH (smart grid) | DF | V1 | |
| UX-005 | Retail, restaurant and mixed sale screens (switchable) | UC (switchable/mixable Retail, Restaurant, Store), CH (multi sale modes) | DF | M | Café + épicerie hybrids |
| UX-006 | Scales from 1024×768 upward | UC, CH | TS | M | |
| UX-007 | Instant search (<100 ms on 100k SKUs) | — | TS | M | |
| UX-008 | Inline undo instead of modal confirmations | — | DF | M | |
| UX-009 | Status bar: online/offline, sync, printer, drawer, user | — | DF | M | |
| UX-010 | Contextual help tooltips | — | DF | V1 | |
| UX-011 | Command palette (Ctrl+K) in the back office | — | DF | V1 | |
| UX-012 | Notification centre (low stock, expiring lots, debts due, backup status) | LV (low-stock notifications) | DF | V1 | |
| UX-013 | Till and back office as separate apps vs one app with modes | CH (separate till and back office) | — | M | Recommend one app with role-based modes |
| UX-014 | Mobile-friendly back office | — | AD | V2 | Via the DZBuild web app |

---

### AA. Onboarding, demo data, help, training

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| ONB-001 | Setup wizard (business type → preset categories, taxes, receipt, hardware) | SB (V5 quick setup), SQ ("link your catalog and set up fast") | DF | M | |
| ONB-002 | Demo / sample data mode (switchable, erasable) | OD (demo data)† | DF | M | |
| ONB-003 | Training mode | EP | DF | V1 | |
| ONB-004 | Help centre and in-app help | LV (100+ articles), KO (online manual) | TS | M | |
| ONB-005 | Community forum | LV, SB (SambaClub), DT | DF | V1 | |
| ONB-006 | 24/7 live chat | LV | — | opt | |
| ONB-007 | Migration assistance from other POS | KO (installation assistance) | DF | V1 | |
| ONB-008 | Business-type templates (grocery, clothing, phone shop, parapharmacy, restaurant, café, bakery, hardware store, cosmetics) | — | DF | M | |
| ONB-009 | Hardware wizard with test print and drawer kick | KO (hardware training)† | DF | M | |
| ONB-010 | Video tutorials in Darija / Arabic / French | — | DF (DZ) | V1 | |

---

### AB. Administration, settings, updates

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| ADM-001 | Auto-updates with channels (stable/beta), delta packages | — | TS | M | |
| ADM-002 | Offline update package (USB) | — | DF (DZ) | V1 | |
| ADM-003 | Roll back to the previous version | — | DF | V1 | |
| ADM-004 | Export/import settings (clone configuration to a new PC) | — | DF | V1 | |
| ADM-005 | Company profile and legal information | — | TS | M | |
| ADM-006 | Document numbering sequences and prefixes | — | TS | M | |
| ADM-007 | Module toggles (hide unused modules: restaurant, serials, lots…) | OD (apps), DT (modules) | DF | M | Keeps the UI simple |
| ADM-008 | Opt-in telemetry and crash reporting | — | DF | M | |
| ADM-009 | No feature paywalls | contrast: LV paid add-ons (S32), SQ Plus/Premium (S11), SH POS Pro (S21) | DF | M | |
| ADM-010 | Accounting export / integration | KO (accounting integration), EP (Xero) | DF | V1 | |

---

### AC. Emerging 2025–2026 features

| ID | Feature | Seen in | Tier | DZ | Notes |
|---|---|---|---|---|---|
| AI-001 | AI assistant ("what sold best last Ramadan?") | SQ (AI tools), LS-K (Lightspeed AI) | DF | V2 | Can run via the DZBuild cloud when online |
| AI-002 | Voice commands | SQ (voice commands, per S18) | AD | V2 | |
| AI-003 | Generated content (descriptions, photos, menus) | SQ (menu/photo generator; auto-polish images) | AD | V2 | |
| AI-004 | AI pricing suggestions | EP (AI pricing tool) | AD | V2 | |
| AI-005 | Predictive inventory / demand forecasting | SH, LS-R, OD 19 | DF | V2 | |
| AI-006 | Computer-vision shelf counts | trend (S56) | AD | n/a | |
| AI-007 | Anomaly and fraud detection (unusual voids or discounts) | trend (S56) | DF | V2 | Rule-based version possible offline in V1 |
| AI-008 | Self-checkout / kiosk | OB (self-checkout), OD (kiosk) | AD | V2 | |
| AI-009 | Supplier-invoice capture (upload a bill into the PO; OCR) | OD 19 (bill upload from PO)† | DF | V2 | |
| AI-010 | Presets / order modes | OD 19 (PoS presets) | DF | V1 | |

---

## 4. Market gaps and pain points found (what to beat)

| # | Gap observed | Evidence | DZBuild POS answer |
|---|---|---|---|
| G1 | Free tiers paywall inventory depth (POs, transfers, counts, production, labels), employee management and sales history | LV: Advanced Inventory $25/store/mo, Employee $5/employee/mo, unlimited history $5/store/mo (S32). SQ: POs, vendors, COGS in Plus; transfers and cross-location returns in Premium (S11). SH: exchanges, staff roles, counts in POS Pro (S21) | Everything free, with no tiers |
| G2 | Offline is partial in cloud POS | LV: no stock or negative-stock alerts offline (S33). SQ: no gift cards, loyalty or close-of-day offline, plus expiry windows (S14). TO: limited offline (S36) | A local DB as the single source of truth; the cloud is optional |
| G3 | Open-source ERP POS are heavy servers and not truly offline | EN offline relies on community apps (S42). OD is a browser client needing a server. DT is a PHP web app | Single installer, embedded DB, zero IT |
| G4 | Desktop open-source POS have dated UX and thin back office | UC/CH are Java Swing descendants of Openbravo POS (S46, S47); FL needs plugins for inventory, floor plan and delivery (S48) | Modern UI with full gestion commerciale |
| G5 | Backup/restore is technical | SB restore via SQL Management Studio (S49) | One-click backup, restore, and move-PC wizard |
| G6 | Printing is fragile in browser POS | DT lists four printer methods, two deprecated (S43) | Native ESC/POS, Windows spooler and ZPL drivers in-app |
| G7 | Customer credit is rare and basic in retail-first POS | Only OD/LS-X/HK have it (S7, S26, S53); OD's credit limit only warns | First-class "crédit client" with limits, reminders, statements |
| G8 | Import is often CSV-only and error-prone | SQ warns that Excel breaks SKUs in CSV (S15) | XLSX-first import with mapping, validation, dry-run, undo |
| G9 | No product combines retail + restaurant + wholesale well | UC offers mixable screens (S46) but a shallow back office | Modes (retail / restaurant / wholesale) in one app, toggled by module |

---

## 5. Shortlist: the 40 features that make a POS "the most complete" (DZBuild must-win list)

1. Offline-first, everything local (OFF-001/002/009) + LAN multi-till (OFF-007)
2. Barcode-first register + keyboard shortcuts + touch (UX-002/003, UX-001)
3. Weight/price barcodes + scale support (CHK-014/015)
4. Parked sales and several open carts (CHK-004/032)
5. Split tender, partial payment, custom tenders (PAY-004/005/006)
6. Customer credit with limits, settlement and statements (PAY-013, CRD-001..005)
7. Layaway / deposits (LAY-001, CRD-005)
8. Returns, exchanges and credit notes with reason codes and anti-fraud limits (RET-001..012)
9. Discount permissions + manager override (DSC-004, EMP-003)
10. Promotions engine: BOGO, tiered, mix & match, scheduled, happy hour, coupons (DSC-005..015)
11. Price lists: retail / demi-gros / gros, quantity breaks (PRC-001/002, PRD-061)
12. Multi-unit (carton ↔ piece) with packaging barcodes (PRD-031/032)
13. Variants matrix (PRD-020)
14. Composite / kits + recipes (PRD-040/041)
15. Serial/IMEI + lots + expiry with FEFO alerts (PRD-050..053)
16. Multi-warehouse + transfers (INV-003/005)
17. Counts with blind mode and reconciliation (INV-007..009)
18. Reorder points + suggestions (INV-013/014)
19. CMUP and FIFO costing + landed costs (INV-019/020)
20. Full purchasing cycle incl. supplier debt (PUR-001..009)
21. Full French/Arabic document set (DOC-005/006/007) and legal fields (TAX-007/008)
22. Label designer and printing (DOC-011, HW-009)
23. Receipt template designer (DOC-002)
24. Cash management with blind close, variance threshold, X/Z (CSH-001..010)
25. Expenses from the till + simple P&L (CSH-015, RPT-027)
26. Roles and granular permissions, unlimited users (EMP-002, EMP-015)
27. Audit log + loss-prevention reports (AUD-001..004)
28. 40–60 built-in reports, all exportable to XLSX/PDF (RPT-*, RPT-017)
29. XLSX/CSV import-export of every entity with undo (IMP-*)
30. One-click backups (local + USB + DZBuild cloud) and a move-PC wizard (BCK-*)
31. Customer display on a second monitor (CHK-039)
32. Restaurant mode: floor plan, split, transfer, kitchen printers, KDS (RST-001..010)
33. Loyalty points + gift cards + store credit, all working offline (LOY, GFT)
34. Work orders / repairs (LAY-003)
35. Special orders / back-orders (LAY-002, INV-028)
36. WhatsApp sharing of receipts, statements and debt reminders (DOC-004, CRD-006)
37. Setup wizard with business-type templates + demo data (ONB-001/002/008)
38. Auto-update with rollback + offline update package (ADM-001..003)
39. DZBuild OAuth link: cloud backup, multi-store dashboard, storefront sync (API-008, BCK-004, OMN-005, RPT-022)
40. Module toggles to keep the UI simple for small shops (ADM-007)

---

## 6. Catalog statistics

Counted from the catalog tables in §3 (one row = one feature).

| Metric | Count |
|---|---|
| **Total catalogued features** | **547** |
| Table-stakes (TS, incl. "TS (DZ)", "TS (resto)") | 211 |
| Differentiators (DF) | 231 |
| Advanced / niche (AD) | 97 |
| Untiered (design notes, n/a) | 8 |
| Recommended for **MVP** (M) | 287 |
| Recommended for **V1** | 141 |
| Recommended for **V2** | 88 |
| Optional / watch | 9 |
| Not applicable in DZ (n/a) | 22 |

Features per module: CHK 46 · INV 41 · PRD 39 · RPT 29 · PAY 28 · RST 28 · HW 22 · IMP 22 · DSC 20 · PUR 20 · CSH 18 · DOC 18 · CRM 15 · EMP 15 · RET 14 · UX 14 · L10N 12 · SEC 12 · TAX 11 · ADM/AI/BCK/OFF/OMN/ONB/PRC 10 each · API 9 · MST 9 · AUD 8 · CRD 8 · LAY 7 · LOY 7 · GFT 5.

> An MVP of about 287 features is large, and deliberately so: the user asked for a POS that "has everything". Most MVP rows are small CRUD and report items that share one data model. The engineering-heavy MVP items are:
> - offline-first DB with a LAN multi-till mode
> - hardware layer (printers, drawer, scanner, customer display, label printer)
> - documents engine (AR/FR, RTL)
> - costing (CMUP)
> - promotions and pricing engine
> - import/export engine
> - backup/restore

## 7. Verification notes and open questions for the other research tracks

- **Unverified (†) items** have not been re-checked against a primary source. The most important ones to confirm before any marketing comparison:
  - Square: named discounts, tax-inclusive pricing
  - Lightspeed R-Series: price books / customer-type pricing details
  - Odoo: formula pricelists, RTL support, multiple barcodes
  - ERPNext: "allow rate/discount change" options (the POS Profile page did not render its full field list)
  - Floreant: pizza builder
- **Toast** marketing pages returned HTTP 403. Toast facts come from Toast's KDS hardware page (S36) and third-party reviews.
- **Clover** feature facts come from a third-party review (S39), not from Clover's own pages.
- **Algeria-specific items** (TVA rates, droit de timbre, NIF/NIS/RC/AI fields, Law 18-07, e-invoicing, local couriers and aggregators, payment methods like CIB/Edahabia/BaridiMob, competitor import formats) are flagged "confirm in DZ docs". They must be validated by the Algerian market / legal research tracks (docs 01/02).
- **Suggested next step for the stack track:** treat §5 as the acceptance list for the architecture. Each item implies design constraints:
  - an event-sourced local DB for OFF-005
  - a hardware abstraction layer for HW-*
  - an RTL-capable document renderer for DOC-006
  - a background sync service for DZBuild OAuth

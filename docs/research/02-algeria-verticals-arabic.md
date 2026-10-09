# 02 — Algerian POS Market by Vertical + Arabic-language & Social-Media Sources

> Research date: 2026-09-28 · Scope: complements the French-web vendor sweep (01-*). Focus: what Arabic-speaking / social-media-sold Algerian POS products offer, what each **vertical** specifically needs, and local commercial habits (الكريدي, versements, prix multiples, fermeture de caisse, BaridiMob, etc.).
> Product: DZBuild POS — free, offline-first, installable Windows desktop POS + gestion commerciale, optionally linked to a dzbuild.dev account (OAuth).

---

## 0. Method, confidence levels and limitations

- Searches were run in Arabic (برنامج تسيير المحلات الجزائر, برنامج كاشير الجزائر, برنامج نقاط البيع الجزائر, برنامج تسيير المخزون والفوترة, برنامج سوبيرات, برنامج صيدلية, برنامج مطعم, IMEI, ملابس, كريدي, جملة/نصف جملة, تقسيط, فليكسي, مواد البناء, قصابة, كوسميتيك, مكتبة, مصبنة…), French and English. Vendor pages were read directly where reachable.
- **Confidence tags used below**
  - **[V] Verified**: the vendor page was fetched and the claim read on it.
  - **[S] Search-snippet only**: seen in search-result text/title (vendor site unreachable, JS-only, or social media). Treat as likely but not independently verified.
  - **[U] Unverified / inferred**: domain knowledge or requirement inferred from general POS practice; no Algerian source found.
- **Limitations**:
  - Ouedkniss, Facebook, Instagram, TikTok and YouTube pages return no usable content when read directly (JS-rendered); only titles/snippets were usable. Google Play pages also returned truncated content.
  - Several Algerian vendor domains were unreachable at fetch time: `kers-pos.shop`, `bistro.icu`, `arabusinesse.com` (Smart Resto), `natrix-soft.com` (GesRep, 404), `ar.newsoft-dz.com` (404). Their claims are marked [S].
  - Not every gap could be closed; the remaining ones (pressing, librairie specifics, some Android apps) are flagged.

---

## 1. Executive summary (key takeaways)

1. **The Algerian "برنامج تسيير المحلات" market is dominated by small local editors selling lifetime licences via Facebook/TikTok/YouTube/Ouedkniss**, not by SaaS. Typical pitch: "برنامج جزائري 100%", Arabic + French, **بدون اشتراكات** (no subscription), **تحديثات مجانية مدى الحياة**, works **بدون إنترنت**, trial version on request, phone number to call. (BNsoft, KERS POS, Factor Facile, YASSNA-POS, Dolisoft, AlgoStock, PharmaPro.)
2. **Price anchors** (one-time): Codeshade 7,000–9,000 DA; AlgoStock 19,500 DA (+5,000–12,000 DA/yr for mobile/AI); PharmaPro 45,000 DA lifetime; Shabakett Officine 80,000–150,000 DA; SaaS: Almawarid 10,000–18,000 DA/yr; Symloop 35,000–120,000 DA/yr; Tassyir 1,900–9,900 DA/month; Pharmacium 80,000–200,000 DA/month (+TVA). A **free** product that matches the lifetime-licence feature set is a very strong proposition.
3. **Must-have "Algerian" features that recur in Arabic marketing** (beyond generic POS):
   - **الكريدي / ديون الزبائن** (customer credit ledger) + **versements** (partial repayments) + supplier debts (مستحقات الموردين), initial balances, black/favourite lists.
   - **Multiple price levels** — "إمكانية البيع ب 4 أسعار" (BNsoft), détail / gros / demi-gros / professionnel (Dolisoft, GestiumPRO), price per customer category.
   - **Sale on hold / waiting** ("خاصية الإنتظار"), returns & damaged goods, expenses (rent, electricity, wages, transport), **profit per period** and **net profit after expenses**.
   - **Zakat calculation** ("حساب الزكاة بنقرة زر واحدة") — a culturally specific differentiator seen in at least two products (BNsoft, "المتجر").
   - **Flexy** (mobile top-up Mobilis/Djezzy/Ooredoo via USSD or API) bundled into the POS for multi-service shops (KERS POS).
   - **Repair module** (phones/PC): intake slip, status, parts, labour — offered as a feature even in general POS (BNsoft, AlgoStock).
   - **Barcode label printing**, expiry / "منتهية الصلاحية" tracking, FIFO/lots.
   - **Bilingual AR (RTL)/FR**, keyboard-first fast checkout, 80 mm / 58 mm ESC/POS tickets, A4/A5 invoices, BL→facture conversion, Excel import.
4. **Regulatory items a POS must handle (2024–2026)**: mandatory mentions RC/NIF/NIS/AI on invoices; sequential numbering; TVA 19 % / 9 %; **droit de timbre** on cash payments (LF 2025: progressive 1 / 1.5 / 2 DA per 100 DA tranche, min 5 DA, electronic payments exempt); ticket Z / daily closure; pharmacy-specific CNAS/CASNOS Chifa. Push toward **TPE & QR payments** (BaridPay QR via BaridiMob launched late 2025; TPE obligation for 11 sectors in 2026).
5. **Vertical specialists exist for pharmacy (many), restaurant, auto parts, phone repair, installment sales, opticians**; but **no Algerian product verified for pressing, librairie-specific (school lists), or butcher-specific scale integration** — generic tools are used. A modular "vertical packs" design (enable per shop type) would cover all of them in one free app.

---

## 2. Arabic-language & social-media vendor landscape (Algeria)

### 2.1 Local "general store" POS products (sold mainly via social media / Ouedkniss / own site)

| Product | Channel(s) seen | Positioning / key features | Pricing / licence | Platform | Conf. | Source |
|---|---|---|---|---|---|---|
| **KERS POS** | Own site, YouTube (KERS FLEXY) | "POS + Stock + Facturation + Flexy"; barcode & labels; invoices; **FIFO / lots / expiry**; debts; reports; **Flexy via API or USSD**, auto-detects operator (Djezzy/Mobilis/Ooredoo), handles special offers; works offline; targets supérettes, cosmetics, clothing, pharmacies, restaurants | **Lifetime licence, no subscription** | Windows (offline) | [S] (site DNS failed at fetch) | https://kers-pos.shop/ · https://kers-pos.shop/blog-best-pos-algeria.php · https://www.youtube.com/watch?v=aG9tJC29Wpw |
| **Factor Facile** (Devion) | Own site | Invoicing of all commercial documents; POS; stock; **كريدي وديون الزبائن**; expenses, revenues, staff; **AI assistant**; **"فاتورة من صورة"** (invoice from photo via AI); multi-device in same shop with per-user permissions; encrypted data; offline, data stored locally | **One-time purchase, no monthly subscription** | Windows desktop (v1.6.70) + Android | [V] | https://devion.cloud/factorfacile |
| **Codeshade POS** | Own site | Fast checkout, barcode, invoices; stock alerts; **debts, invoices, daily movements**; real-time profit; multi-location overview | Starter **7,000 DA** (1 POS, 500 products), Growth **9,000 DA** (5,000 products), Multi-store custom | Offline mode | [V] | https://codeshade.app/ |
| **BNsoft** | TikTok (@bnsoft_), YouTube | "برنامج جزائري 100%"; AR+FR; **4 price levels**; barcode label printing; **customer waiting/hold** ("خاصية الإنتظار"); customer & supplier debts; **services: phone repair**; profit per period; **Zakat in one click**; totals of sales/purchases; out-of-stock & **expired** products list. Verticals listed: supérette, librairie/papeterie, clothing/shoes/bags, pastry supplies & packaging, para-pharmacie, pièces détachées, électroménager, cosmetics, quincaillerie (كنكاري), household utensils, computer shops, paint shops, phones & accessories | No subscription, lifetime free updates; trial on request; phone 06 72 95 70 16 | Desktop (assumed Windows) | [S] | https://www.tiktok.com/@bnsoft_/video/7500743518688742662 |
| **YASSNA-POS** | YouTube | "أقوى برنامج تسيير محلات التجزئة في الجزائر", gestion de stock; versions 2022–2023 | Unknown | Unknown | [S] (titles only) | https://www.youtube.com/watch?v=0Bdx2hPSNYs · https://www.youtube.com/watch?v=bb_mlKs-g9E |
| **POWER PRODUCTION** | YouTube (Dec 2023) | Management of **production workshops**, shops and warehouses | Unknown | Unknown | [S] | https://www.youtube.com/watch?v=lotrFNyo6Qo |
| **DIGISOFT GOLD** | YouTube (Aug 2025) | "برنامج تسيير المحلات التجارية – النسخة الذهبية" | Unknown | Unknown | [S] | https://www.youtube.com/watch?v=sRxZ9EgcsrM |
| **Infinity (إنفينيتي) with AI** | YouTube (Feb 2026) | "أسرع برنامج تسيير محلات في الجزائر" — supermarket POS with AI | Unknown | Unknown | [S] | https://www.youtube.com/watch?v=wvBpdljTh9Q |
| **"أفضل و أقوى باك برنامج تسيير المحلات 2023"** | YouTube | Bundle of store-management programs | Unknown | Unknown | [S] | https://www.youtube.com/watch?v=U7rvBiu_TSo |
| **HiroSystem** | Ouedkniss (Birtouta, Alger) | Sale by barcode and by name; stock; daily & monthly reports; "any commercial activity" | Unknown | Unknown | [S] | https://www.ouedkniss.com/logiciels-abonnements-...-d52265413 |
| **CPOSdz** | Facebook | Software + hardware for supérettes & "مواد غذائية عامة"; commercial management, POS, e-commerce | Unknown | Unknown | [S] | https://www.facebook.com/cposdz/ |
| **"المتجر" (Al-Metjar)** | Forums / Ouedkniss | 100 % Arabic; min-stock alerts; barcode; inventory; **returns & damaged goods**; purchases on credit / partial payment; **customer types retail vs wholesale → automatic sale price**; customer debts & supplier dues over time; **opening balances**; **favourite / blacklist** customers; **expenses** (transport, rent, electricity, wages); margin per product type or per item; profit instant/hourly/daily; **net profit after expenses**; **Zakat**; cash register statistics; **automatic periodic DB archiving/backup**; "table sales" (بيع الطاولات) | Unknown | Windows (assumed) | [S] | https://www.ouedkniss.com/applications-logiciels-...-d13827110 · https://djelfa.info/vb/showthread.php?t=2065687 (403) |
| **LinkSoft** | Facebook | "تسيير المحلات التجارية Logiciel de Gestion Commerciale" | Unknown | Unknown | [S] | https://www.facebook.com/LinkSoftLogiciel/ |
| **GDS Pro** (Yellow Store DZ) | Facebook video | "برنامج تسيير المحلات LOGICIEL GESTION DE STOCK GDS PRO" | Unknown | Unknown | [S] | https://www.facebook.com/dz.yellowstore/videos/3956026711338527/ |
| **Logiwal (LSCDEV)** | Facebook video | "أذكى برنامج لتسيير المحلات التجارية خاص بجميع المجالات" | Unknown | Unknown | [S] | https://www.facebook.com/LSCDEV/videos/811969174081991/ |
| **Dimastore (ديماستور)** | Ouedkniss (via fibladi) | POS + shop/SME management, **3 languages** | Unknown | Unknown | [S] | https://shop.fibladi.com/ar/algerie/detail/www.ouedkniss.com-15722662.htm |
| **Stock X** (Newsoft DZ) | Own site (404 now) | "برنامج تسيير المخزون و الفوترة الإصدار 10" — all documents & slips between company and partners | Unknown | Windows | [S] | https://ar.newsoft-dz.com/index.php/2017-11-07-20-11-13 |
| **AlgoStock** (Algosynapse) | Own site | Fast barcode POS; **payment modes Cash, CIB/Edahabia, BaridiMob, customer credit**; thermal tickets, A4 invoices, BL, devis; multi-warehouse, transfers; **expiry / perishables**; NIF/NIS/RC; partial invoicing & payment deferrals; **repair module** (work orders, parts, labour); analytics, stock-out prediction; 5 roles (admin, manager, cashier, technician, employee); **AR (RTL)/FR/EN switch without restart**; local PostgreSQL, **100 % offline**, backups to USB/network; mobile app syncing over **local Wi-Fi**; AI (OCR supplier invoice photo, NL questions) | Standard **19,500 DA lifetime** (1 PC); Pro Connect +5,000 DA/yr; Elite AI +12,000 DA/yr | Windows 10/11 x64 + iOS/Android companion | [V] | https://algosynapse.com/algostock/ |
| **Almawarid** (Issal SARL) | Own site (SEO pages per vertical & wilaya) | 41 professions; web + Android + Windows desktop (offline); ticket Z "conforme DGI"; TVA 19/9 %, CNAS, IRG, loi 18-07, D.E. 05-468; RH/paie; hosted in Algeria | Free plan; Standard **10,000 DA/yr** (≤5 employees), Enterprise **18,000 DA/yr** | Web/Android/Windows | [V] | https://almawarid.app/ · vertical pages cited in §3 |
| **GestiumPRO** (Cirtasoft) | Own site (AR/FR) | Devis, commandes, factures, balances, customer payments; multi-warehouse; POS with daily closure & multi-payment; CRM & loyalty; **delivery & routes mobile app**; échéances; **multi-criteria price tables, seasonal promos, per-customer discounts**; 50+ sectors incl. retail, **demi-gros, gros**, bakeries, pharmacies, fashion, electronics, fuel stations, materials | Not shown | Web/desktop | [V] | https://www.cirtait.com/logiciel-gestion-commerciale/ar/ |
| **DZ Compta** | Own site | Stock for Algerian companies: inventory, movements, valuation, stock-out alerts, multi-warehouse, product tracking | Not shown | Cloud | [S] | https://dzcompta.cloud/ar/stock/ |
| **Managely** | Own site (AR) | POS on same data as ERP; **barcode + keyboard-only, no mouse**; offline retail POS; WhatsApp automation, e-shop, delivery, booking; **SCF-compliant** accounting; DZD | 7-day trial; no per-user fees | Cloud + offline POS | [V] | https://managely.cloud/ar/best-pos-software-in-algeria |
| **Tassyir (تسيير)** | Own site | Algerian e-commerce store builder: orders from purchase → confirmation → parcel delivery; stock alerts; finances (sales, expenses, wages); staff permissions; multi-store; delivery-company integration; pixel tracking; abandoned/duplicate order recovery | 1,900 / 4,900 / 9,900 DA per month | Web | [V] | https://tassyir.io/ |

### 2.2 Foreign Arabic-language POS vendors that dominate Arabic search results
These appear first for Arabic queries (Egypt/KSA vendors), are **not Algeria-specific** (ZATCA e-invoicing, EGP/SAR), but shape Arabic-speaking users' expectations: **DEXEF** (desktop + cloud cashier, clothing, building materials, restaurants — https://dexef.com/software/dexef-cashier/), **Daftra** (https://www.daftra.com/), **Alcashier** (offline, multi-branch, multi-cashier — https://alcashier.com/), **Al Badr** (supermarket, clothes/shoes, building-materials packs — https://albadrsales.com/en/pos-supermarkets-grocery-stores/), **Pioneers Easy Store** (https://pioneers-solutions.com/blog-download-cashier-program), **Kashier/Cashierk** (ZATCA — https://kashierapp.com/), **Fatoorah** (shifts/sessions — https://fatoorah.ai/). [V/S] Takeaway: Arabic users expect vertical landing pages (ملابس، مواد بناء، سوبر ماركت، مطاعم) with the same core engine.

### 2.3 Android POS / merchant apps relevant to Algeria
| App | What it does | Notes | Conf. | Source |
|---|---|---|---|---|
| **Mahaal Caisse enregistreuse** (Inyad) | Mobile POS; "6000+ merchants"; site now redirects to inyad.com | Inyad claims 350,000 businesses/month across 40+ countries | [V] (redirect page) / [S] | https://www.mahaal.app/ar/... · https://play.google.com/store/apps/details?id=com.inyad.store · https://inyad.com/ |
| **inyad Konnash — كناش الديون** | Digital credit notebook: customers & suppliers, credits/debits, **payment reminders**; used by épiceries, wholesalers, distributors | Free | [S] | https://play.google.com/store/apps/details?id=com.invyad.konnash |
| **DZ POS** (NGC Technologies) | Order-taking for sales teams, multiple open bills, categories, pictures, prices, inventory | Algerian-named | [S] | https://play.google.com/store/apps/details?id=com.ngctechnologies.dzposAPP |
| **Cashier – ادارة حسابات المحلات** | Shop accounts, barcode reader | Egyptian dev, Arabic | [S] | https://play.google.com/store/apps/details?id=com.ahmedelshazly2020d.sales_managers |
| **Alcashier – الكاشير** | Restaurant/store POS, offline | Arabic | [S] | https://play.google.com/store/apps/details?id=com.alcashier |
| **Daftra POS** | Cloud POS | Arabic | [S] | https://play.google.com/store/apps/details?id=com.izam.daftraPos |
| **OmniPOS Restaurant** | Restaurant/café accounts, product costing | Arabic | [S] | https://play.google.com/store/apps/details?id=omnipos.restaurant.pos |
| **مدير الديون / Debt manager**, **Teajar** (WhatsApp reminders, PDF statements) | Debt tracking | Generic | [S] | https://play.google.com/store/apps/details?id=com.centuryegg.pdm · https://teajar.io/ar/our-products/customer-debt-app |
| **Masroufi (مصروفي)** | Algerian (Djelfa) BNPL grocery app — "تسوق الآن وادفع بالكريدي", deferred to payday, deducted from bank/Edahabia ("golden") card | Launched 2024; shows the **kridi habit** moving to apps | [V] | https://www.echaab.dz/2024/09/19/... |
| **Flexy apps** (Flexy ici, Mobile ALG, **Flexy Pro** for reseller networks: wallet in DA, Mobilis/Djezzy/Ooredoo top-up, **Idoom** & digital cards) | Mobile top-up for shops | Flexy is a common add-on service in Algerian shops | [S] | https://flexy-pro.com/?lang=ar · https://play.google.com/store/apps/details?id=ougmos.elokaz.flexy |

**Implication for DZBuild POS**: Android is where credit-book and simple POS apps live; Windows is where "real" shop management happens. A Windows-first app with an optional companion (read-only dashboard / waiter / inventory counting over LAN) mirrors what AlgoStock and Smart Resto do.

---

## 3. Verticals — specific needs and who serves them

Legend for "Needs": ★ = explicitly stated by an Algerian vendor/source; ☆ = standard for vertical, not seen in an Algerian source [U].

### 3.1 Supérette / alimentation générale / épicerie (سوبيرات، مواد غذائية عامة)
**Needs**
- ★ Fast checkout at peak hours; hundreds–thousands of SKUs ("Inventaire de centaines de SKUs", "Caisse rapide aux heures de pointe") — Almawarid [V].
- ★ **Neighbourhood customer credit**: "Crédit clients du quartier — qui doit combien"; case: "Une supérette à Bab El Oued avec 1 200 produits et 30 clients à crédit" — Almawarid [V].
- ★ Barcode scanning + **label printing** for unlabelled goods; min-stock alerts; **expiry list** (BNsoft, KERS, AlgoStock).
- ★ Weighed goods (fruits, vegetables, cheese, olives) → **barcode scales** Rongta/Aclas widely sold in Algeria (see §5).
- ★ **Hold/park a ticket** while customer fetches item ("خاصية الإنتظار" — BNsoft).
- ★ Multiple prices (detail vs. small wholesale for neighbouring shops).
- ★ Ticket Z / closure; payment breakdown.
- ★ **Flexy** counter (top-up) often in same shop — KERS POS.
- ☆ Promotions (pack/lot pricing, "2+1"), quick keys for bread/milk/eggs (unbarcoded fast-movers), price-check mode, shelf labels, supplier returns of expired goods, sachets/pieces from carton (unit conversion).
**Local software**: KERS POS, BNsoft, CPOSdz, HiroSystem, Codeshade, AlgoStock, Almawarid (supermarché Alger page: "caisse rapide multi-rayons, stock en temps réel et ticket Z conforme DGI" [S] https://almawarid.app/logiciel-supermarche-alger/), Infinity AI [S].
**Sources**: https://almawarid.app/logiciel-epicerie-superette-algerie/ · https://www.facebook.com/cposdz/ · https://www.tiktok.com/@bnsoft_/video/7500743518688742662

### 3.2 Boulangerie / pâtisserie (مخبزة / حلويات)
**Needs**
- ★ **Daily production vs sales** to avoid waste ("Production journalière vs ventes — éviter le gaspi") — Almawarid [V].
- ★ **Special orders** (weddings, celebrations, custom cakes) replacing WhatsApp/Excel: deposit (arrhes), pickup date, customer — Almawarid [V].
- ★ Ticket Z "conforme DGI exigé en cas de contrôle fiscal" — Almawarid [V].
- ★ Touch caisse, cash drawer, ticket printer; margin calculation; remote sales monitoring — Clyo Systems [S].
- ☆ Recipes / BOM (flour, sugar, butter) with raw-material stock decrement; unsold (invendus) & losses; sale by piece and by kg (gâteaux traditionnels by weight → scale); pre-orders for Ramadan/Aïd peaks; quick keys with images; bread price is regulated (baguette subsidised) [U].
- ★ "لوازم الحلويات و التغليف" (pastry supplies & packaging shops) is itself a retail vertical — BNsoft [S].
**Local software**: Almawarid (Standard 10,000 DA/yr) [V], Clyo Systems Algérie (Kouba; restaurant, fast-food, cafétéria, boulangerie/pâtisserie, institut de beauté; tel 023 78 56 58) [V/S], POWER PRODUCTION (production workshops) [S], GestiumPRO (bakeries listed) [V].
**Sources**: https://almawarid.app/logiciel-boulangerie-patisserie-algerie/ · https://www.clyosystems.dz/ · https://www.facebook.com/ClyoSystemsAlgerie/

### 3.3 Boucherie / charcuterie / poissonnerie / fruits & légumes (قصابة) — weight & scale
**Needs**
- ★ **Barcode-printing scales** (balance étiqueteuse): weigh → print label with weight, price, barcode → scanned at POS. Used "en supermarché, épicerie, boucherie ou marché" — General IT [V/S].
- ☆ POS must decode **weight/price-embedded EAN-13** (prefix 20–29 variable-measure: PLU + weight or price + checksum) configurable per scale brand [U — standard GS1 practice].
- ☆ Direct serial (RS-232) / Ethernet scale reading on the POS (Rongta RLS1000 has RS232 + Ethernet) [S]; PLU upload from POS to scale (price list sync).
- ☆ Sale by kg with decimal quantities (3 decimals), tare, price per kg display; carcass cutting yield (achat carcasse → découpe) [U]; credit for regular customers (restaurants).
- ★ Almawarid lists "Boucherie/Charcuterie" as a vertical; pricing 10,000–18,000 DA/yr [S].
**Local software**: no Algerian butcher-specific POS verified; generic POS + scale. Searches in Arabic ("برنامج تسيير القصابة … ميزان") returned no local product. **Opportunity.**
**Sources**: https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-2/ · https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-ls6/ · https://diamond-tech-dz.com/product/balance-code-a-barre-rongta-rls-1000/ · https://www.ouedkniss.com/autre-balance-code-a-barre-rongta-rls1000-blida-algerie-d25104259

### 3.4 Fast-food / restaurant / café / pizzeria (مطعم، فاست فود، كافيتيريا)
**Needs**
- ★ **Tables / floor plan**, **waiter (serveur) app sends order directly to kitchen** ("السارفور يبعث الطلب للمطبخ" — Instagram ad; Smart Resto "تطبيق ذكي يستخدمه النادل") [S].
- ★ One server linking "كاشير - جرسون - مطبخ - طاولات - ديليفري" (cashier, waiter, kitchen, tables, delivery) [S].
- ★ **Kitchen tickets / KDS**, section printers (grill, pizza, bar), reservations — Protid, S2I Ultimate FERP ("liaison salle-cuisine"), Symloop [S/V].
- ★ Service modes: **sur place / à emporter / livraison** [S].
- ★ Raw-material stock + **recipes/costing** ("تحديد تكلفة المنتجات" — OmniPOS; Almawarid "stocks de matières premières, paie serveurs/cuisiniers, ticket Z") [S].
- ★ Bilingual AR/FR, offline with sync, multi-payment incl. credit, loyalty + SMS promos — Symloop Sétif [V].
- ☆ Menu items with **supplements/options** (sauce, fromage, taille), combos/menus, split bill, transfer table, merge tables, ticket number/call screen for fast-food, delivery driver assignment + cash collection, phone-order customer lookup by number & address; café: very fast "tap" sales (café, thé, eau), staff meals; Yassir/other delivery-platform orders entered as a channel [U].
**Local software**: **Bistro** (bistro.icu — "برنامج تسيير المطاعم في الجزائر") [S, site down], **Smart Resto** (arabusinesse.com — restaurants, cafés, tourist parks; waiter app) [S, site down], **Clyo Systems Algérie** [V], **Protid Pro Restaurant** (réservation, commandes, tickets cuisine, menus, stock, mobile terminal; +213 553 900 297) [S], **Ultimate FERP** by S2I, **RestPlus** by SII (cantines, restaurants, hotels) [S], **INNOVA SOFT** [S], **iThreeWeb** [S], **Almawarid** (restaurant Oran) [S], **Symloop** (Sétif; plans 35,000 / 65,000 / 120,000 DA per year incl. install & training) [V].
**Sources**: https://www.instagram.com/p/DXgZyo6jUJn/ · https://www.bistro.icu/ · https://arabusinesse.com/ · https://www.protid.com/logiciels-algerie/gestion-restaurants-en-algerie/ · https://www.solutionsinformatiques.dz/RestPlus-un-progiciel-de-gestion-des-cantines-et-des-restaurants-toutes-options?lang=fr · https://www.innova-dz.com/ · https://ithreeweb.com/entreprises/logiciel-de-gestion-de-restaurant-algerie/ · https://almawarid.app/logiciel-gestion-restaurant-oran/ · https://symloop.com/ar/blog/logiciel-caisse-pos-restaurant-commerce-setif-2026/

### 3.5 Pharmacie & parapharmacie (صيدلية / شبه صيدلي) — the most specialised vertical
**Algerian specifics**
- **CNAS/CASNOS tiers payant via Chifa card**: insured pays typically 20 %, CNAS reimburses 80 % [S — search summary of lamacta]; >12–13k contracted pharmacies; Chifa 2.0 / e-Chifa (digital card in "الهناء" app), reimbursement ceiling raised 3,000 → 5,000 DA [S] (https://cnas.dz/en/chifa-3/ · https://ar.awrasaljazair.com/carte-e-chifa-…).
- Historic pain: the official Chifa software "fails to track expiration dates, batch numbers, prices… no inventory, no barcode"; pharmacists "transformed into CNAS clerks" (vignette removal & gluing); 80 % dissatisfied (Algérie360, 2011) [V] → **commercial pharmacy software fills the gap around Chifa**.
- Regulated margins, **psychotropes/stupéfiants register**, national nomenclature of approved medicines, TVA exemptions for medicines (Art. 41 CTCA per PharmaPro) [V].
**Needs (all ★ from Algerian vendors)**
- Sales regimes: **Comptoir, Chifa, CASNOS, Crédit, Instance, Retours** (PharmaPro) [V].
- **Multi-lot, FEFO** deduction ("déduit du lot avec la péremption la plus proche en premier"), expiry alerts (30 days default) [V PharmaPro]; FIFO/FEFO, removal of expired lots [V Pharmacium].
- Search by name, barcode, **DCI**, laboratory, registration number [V PharmaPro].
- **Supplier e-invoice / BL import** (UPROMEDIC API — 6 branches; dedup; auto lot creation) [V PharmaPro]; direct invoice import (Shabakett) [V].
- **Chifa file generation** in CNAS format, catalogue sync, **rejection tracking** [V PharmaPro]; real-time **bordereaux CNAS & CASNOS** (closure, submission, verification) [V Shabakett]; "Saisie assistée des ventes CHIFA avec code à barres ou prix" [V XpertPharm].
- Ordonnances, patient history, interactions, renewal alerts, loyalty [V Pharmacium]; loyalty points [V Silwane/PharmaPro].
- Order assistant (predict shortages) [V Silwane, XpertPharm]; laboratory margins, discounts, **UG (unités gratuites)** [V Silwane].
- Cash security: counter tracking, seller audits, cash vs treasury comparison [V Silwane]; >40 user privileges incl. lots & psychotropes [V Shabakett].
- Mandatory receipt mentions RC/NIF/NIS/AI/capital, sequential numbering, NIF/NIS 15-digit validation, **e-invoice JSON "ready for DGI"**, fiscal block without RC/NIF (PharmaPro claims "Loi de Finances 2024" compliance) [V vendor claim].
- Offline: 14-day offline licence grace, local SQLite, daily backups, **Telegram notifications** of sales/alerts, AR/FR RTL, silent auto-update [V PharmaPro].
- Parapharmacie: cosmetics with expiry, promotions; "شبه صيدلي" served by general POS (BNsoft) [S].
**Local software & prices**
| Product | Price | Source |
|---|---|---|
| PharmaPro (tkawen) | **45,000 DA lifetime**, 15-day trial, Windows 10/11 | https://pharmapro.tkawen.com/ [V] |
| Shabakett Officine | 80,000 / 100,000 / 150,000 DA | https://www.shabakett.com/products/shabakett-officine [V] |
| Pharmacium (Cirtasoft) | 80,000–200,000 DA **per month** + 19 % TVA (as read on page) | https://www.cirtait.com/pharmacium/ [V] |
| XpertPharm (+ XpertAssist) — SQL Server, tabs, remote access | n/a | https://xpertsoft-dz.com/xpertpharm.php [V] |
| Silwane Signature (IntelliX) — "100 % barcode" | n/a | https://www.intellixgroup.com/ar/logiciel-pharmacie-silwane-signature [V] |
| CTI Pharm (CTIB) — user manual PDF | n/a | https://ctib.dz/adm/gfichiers/upload/files/MesDoc/Manuel_cti_pharm.pdf [S] |
| Almawarid pharmacie (expiry across thousands of refs, multi-seller caisse, recurring prescriptions) | 10–18k DA/yr | https://almawarid.app/logiciel-pharmacie-algerie/ [V] |
**Recommendation**: DZBuild POS should support **parapharmacie** fully (lots/expiry/FEFO) but treat **officine + Chifa** as a later, optional "pack" — Chifa/CNAS integration is regulated, and the market is already crowded with specialists at 45k–150k DA.

### 3.6 Quincaillerie / matériaux de construction (كنكاري / مواد البناء)
**Needs (★ Algerian sources)**
- ★ **Multi-units**: "articles vendus au kg, ml, m², unité, lot ou sac"; price per packaging (GestiumPRO) [V]; "achat au carton, stockage à la pièce, **câble vendu au mètre** [acheté en couronne], visserie au sachet ou parfois au poids"; conversions pièce/boîte/carton/mètre/kg (EP Digital, Odoo) [V].
- ★ Link internal refs, manufacturer refs, supplier refs and **alternative/old codes** to one article; counter search by technical designation (EP Digital) [V]; attributes: dimensions, material, brand, compatibility, packaging.
- ★ **Devis chantier** multi-articles → bon de commande → facture in one click (GestiumPRO) [V]; "Devis chantier (entrepreneur)" (Almawarid) [V].
- ★ **Contractor credit accounts** with configurable **credit ceilings**, real-time outstanding, alerts before exceeding: "Les entrepreneurs, maçons et artisans travaillent souvent à crédit" (GestiumPRO) [V].
- ★ Materials: sale **au m² / m³ / palette**, **truck delivery + pallet loading**, "30 chantiers/mois" (Almawarid) [V]; multi-register invoices / multi-barcode (Basmatec SILVER) [S].
- ☆ Paint: tint formulas; tiles: m² ↔ boxes rounding; cement: sacks & tonnes; returns of unused materials; delivery notes per chantier; deposit (consigne) for pallets [U].
**Local software**: GestiumPRO (Cirtasoft) [V], Almawarid [V], OnyxStock (SII — multi-unit, multi-depot) [S], General IT quincaillerie [S], Basmatec SILVER [S], POWER PRODUCTION [S], BNsoft (كنكاري, paint shops) [S], EP Digital (Odoo) [V].
**Sources**: https://www.cirtasoft.com/secteurs/quincaillerie/ · https://epdigitaldz.com/logiciel-gestion-stock/quincaillerie/ · https://almawarid.app/logiciel-materiaux-construction-algerie/ · https://almawarid.app/logiciel-quincaillerie-outillage-algerie/ · https://www.solutionsinformatiques.dz/Logiciel-de-Gestion-de-stock-Algerie?lang=fr · https://www.general-it.dz/logiciel-de-gestion-de-quincaillerie/ · https://www.basmatec.com/?page_id=145

### 3.7 Pièces détachées auto (قطع الغيار)
**Needs**
- ★ Search by **OEM reference or brand/model/year**; **equivalences & vehicle compatibility** (Almawarid) [V].
- ★ **Shelf/location management** ("stock par rayon"); auto barcode generation; smart search by name/reference/barcode (Dolisoft) [V].
- ★ **Multiple tariffs by customer: détail, gros, professionnel** (Dolisoft) [V]; **garage accounts vs walk-in customers** (Almawarid) [V].
- ★ BL → facture automatic conversion; **Excel mass import**; debt tracking; print on **Ticket, A5 or A4** (Dolisoft) [V].
- ★ **Landed cost of imported parts (douane, transport)** (Almawarid) [V].
- ★ Warranty & traceability of parts (ITRI / General IT) [V].
- ☆ Cross-reference catalogue import (TecDoc-like, but commercial licence costly) — support user-maintained equivalence groups instead [U]; used/second-hand parts; core/consigne returns [U].
**Local software**: Dolisoft (lifetime licence, "1st licence bought = 2nd free", AR/FR, offline) [V], Almawarid ("Tarif sur demande") [V], ITRI Pièces détachées (General IT) [V], MEGA-AUTO (Megasoft) [S], Sphinx Manager [S], BNsoft [S]. SAA (insurer) built its own parts software (Algerie360) [S].
**Sources**: https://dolisoft-dz.com/products/promo-dolisoft-logiciel-de-gestion-complet-pour-magasins-de-pieces-detachees-1 · https://almawarid.app/logiciel-pieces-auto-algerie/ · https://www.general-it.dz/logiciel-de-gestion-des-pieces-detachees/ · https://www.megasoft-office.com/logiciel-mega-auto/ · https://www.ouedkniss.com/gestion-stock-pieces-detachees_informatique-r?lang=fr

### 3.8 Vêtements / chaussures / sacs (ملابس، أحذية)
**Needs**
- ★ **Size + colour + season** matrix; example "40 modèles × 5 tailles × 4 couleurs"; **soldes/promos + remises clients**; **container reception with barcodes** (Almawarid) [V].
- ★ Add colours & sizes from one screen, barcode per variant (DEXEF, Arabic) [S].
- ★ BNsoft lists clothing/shoes/bags [S]; Ouedkniss listing "برنامج تسيير محل ملابس – وهران بئر الجير" [S].
- ☆ **Exchange** (استبدال) more common than refund; exchange voucher/credit note; stock by variant grid; label printing per variant; slow-mover ("راكد") report; seasonal markdown; consignment/dépôt-vente [U].
**Local software**: Almawarid [V], BNsoft [S], AlgoStock (clothing listed) [V], Inyad/Mahaal (clothing landing page) [S].
**Sources**: https://almawarid.app/logiciel-pret-a-porter-algerie/ · https://dexef.com/apps/clothing-store-software/ · https://www.ouedkniss.dz/…-d49106504

### 3.9 Téléphonie / électronique / informatique + réparation + flexy
**Needs**
- ★ **Repair workflow**: device intake slip (وصل دخول الأجهزة), device state, parts stock, **net profit per repair**; also computers (GesRep) [S]; multi-user with permissions, print repair receipt, follow from reception to delivery, auto parts stock (Arabic forum listing) [S].
- ★ **IMEI verification & repair tracking** + POS + customers + payments (DzPhoneHub, Alger) [S].
- ★ **IMEI & serial number tracking** in stock (Vyvex) [V]; repair work orders with parts & labour, "Technician" role (AlgoStock) [V].
- ★ "خاصية الخدمات: تصليح هواتف" inside general POS (BNsoft) [S].
- ★ **Flexy** top-up for Mobilis/Djezzy/Ooredoo via USSD offline or API; operator auto-detection; offers (KERS POS) [S]; Idoom & digital cards (Flexy Pro) [S].
- ☆ Warranty per serial (date sold, duration), SAV returns to supplier, accessories bundles, second-hand buyback (condition, IMEI, buyback price), SMS/WhatsApp "your phone is ready" [U — seen in foreign tools FixVault/Phonilab].
**Local software**: GesRep (Natrix-soft) [S], "Flash" repair software (dzgsm forum, Aug 2026) [S], DzPhoneHub [S], AlgoStock [V], BNsoft [S], KERS POS [S], Vyvex [V].
**Sources**: https://www.natrix-soft.com/2019/11/gesrep.html · https://dzgsm.com/vb/threads/96766/ · https://dzgsm.com/vb/threads/65279/ · https://dzphonehub.online/ · https://www.vyvex.app/ar · https://flexy-pro.com/?lang=ar

### 3.10 Cosmétiques (كوسميتيك)
**Needs**: ★ expiry dates + barcode + FIFO/lots (KERS POS positioning) [S]; ★ listed by BNsoft [S]; ☆ product variants (shade/teinte), testers write-off, loyalty, promos/bundles, sales by brand, many unbarcoded imports → label printing [U]. Cosmetics shops are a popular small-business project in Algeria (TikTok "مشروع محل كوسميتيك في الجزائر") [S].
**Sources**: https://kers-pos.shop/ · https://www.tiktok.com/discover/مشروع-محل-كوسميتيك-في-الجزائر

### 3.11 Librairie / papeterie (مكتبة، أدوات مدرسية)
**Needs**: ★ listed by BNsoft and Almawarid ("Bookstore/Stationery") [S/V]; ☆ **rentrée scolaire peak** (official "المدونة الرسمية للأدوات المدرسية 2025-2026" published by Ministry of Education → school supply **lists/packs** per level), thousands of low-price SKUs without barcodes, photocopy/printing services (per page), ISBN for books, returns to publishers, school/administration invoices on credit [U + context source].
**Local software**: none specific verified. Generic POS. **Opportunity: "school list" pack builder.**
**Sources**: https://www.education.gov.dz/2025/09/03/… (official school supplies list) · https://ultraalgeria.ultrasawt.com/… (rentrée 2026 import licences)

### 3.12 Grossiste / demi-gros / distribution (جملة، نصف جملة)
**Needs**
- ★ **Multi-depot stock, thousands of refs; tariffs per customer + encours & crédits; bons de livraison & tournées**; case: "grossiste alimentaire à Blida livrant 200 détaillants" (Almawarid) [V].
- ★ Retail/wholesale customer type → automatic price ("المتجر") [S]; "4 prices" (BNsoft) [S]; multi-criteria price tables & per-customer discounts, **delivery routes mobile app** (GestiumPRO) [V].
- ★ Sales-team order taking on phone, multiple open bills (DZ POS) [S]; **Cash-van / DSD** (Protid) [S].
- ☆ Salesperson (vendeur) assignment & **commissions**, route/tournée planning, truck stock (van as a depot), collection of versements on route, carton ↔ unit pricing, minimum quantities for gros price, UG (free goods) promotions, customer credit ceilings, aged balance [U].
**Local software**: GestiumPRO, Almawarid, Protid (DSD & Cash Van), DZ POS, AlgoStock (wholesalers listed), Konnash (credit book for wholesalers).
**Sources**: https://almawarid.app/logiciel-grossiste-distribution-algerie/ · https://www.cirtait.com/logiciel-gestion-commerciale/ar/ · https://www.protid.com/logiciels-algerie/gestion-restaurants-en-algerie/

### 3.13 Optique (نظارات)
**Needs**: ★ patient file, stock of **frames, articles, lenses**, devis → facture → commande, SMS/email, stats (NESOFT OVM) [V]; ★ **CNAS convention**: opticians reimbursed on invoiced amount, insured pays 20 % at 80 % coverage (2013, Oran — 11 opticians) [V]; ☆ prescription (OD/OG sphere/cylindre/axe/addition), lens orders to lab with status, deposit + balance on pickup, CNAS dossier tracking [U].
**Local software**: NESOFT Optic Vision Manager (OVM) and Provider Vision Manager (for optical distributors) [V]; Almawarid (clinics) [S].
**Sources**: https://www.nesoft.dz/nosproduits.php · https://www.acuite.fr/actualite/economie/2887/algerie-11-opticiens-lunetiers-rejoignent-la-cnas · https://conformepro.dz/resources/codes-activites/services/opticien-lunetier

### 3.14 Électroménager / meubles — vente à tempérament (البيع بالتقسيط / facilité)
**How it works in Algeria**
- Shops sell appliances/furniture/phones on instalments; customers often pay a **down payment**, rest in **monthly instalments** by cash, **CCP deduction (اقتطاع)** or cheques [V Echorouk / Vyvex].
- **Legal risk**: many merchants demand **guarantee cheques ("صكوك ضمان")** — "هذا السلوك مجرم قانونا"; customers risk 2–5 years prison for bounced cheques [V Echorouk]. → software should record guarantees neutrally (document type, not cheque-as-guarantee) and offer schedule/contract instead.
- Bank consumer credit alternative: 12–36 months, salary ≥30,000 DA/month, CDI contract, monthly deduction from CCP/bank (Al Salam) [S].
- Demand "modest"; instalment price notably above cash price (El Masdar, Jan 2024) [V].
**Needs (★ Vyvex [V])**: contract generated/printed from customer file; **automatic instalment schedule** printable; collection modes **Cash / CCP / direct withdrawal** in one ledger; **client scoring** flagging duplicate accounts and past delays before signing; late-payment dashboards; IMEI/serial tracking; multi-location stock; roles owner/manager/cashier/accountant.
- ★ Offline instalment-tracking app on Ouedkniss ("برنامج تسيير الأقساط للمحلات – Logiciel de Gestion de Facilité", Djelfa): add customers & debts, **auto-compute instalments, show arrears** [S].
- ☆ Guarantor (ضامن) data, documents (ID, work certificate, pay slips, CCP number), penalties, reminder SMS, early settlement [U].
**Local software**: Vyvex [V], Ouedkniss "Gestion de Facilité" (Djelfa) [S], Masroufi (BNPL app) [V].
**Sources**: https://www.vyvex.app/ar · https://www.echoroukonline.com/تجار-يشترطون-صكوك-ضمان-وزبائن-عرضة-للس… · https://www.elmasdaronline.dz/04/01/2024/… · https://www.alsalamalgeria.com/ar/produits/detail-312-47.html · https://www.ouedkniss.com/…-d52615477

### 3.15 Pressing / blanchisserie (مصبنة)
**Needs** [U — no Algerian pressing software found; Arabic search returned only generic results and a Moroccan pick-up service]: deposit ticket with numbered tags per item, item type & service price list (lavage, repassage, nettoyage à sec), promised date, status (reçu / en cours / prêt / livré), partial prepayment, SMS when ready, uncollected items after N days, damage notes/photos.
**Local software**: none verified. **Opportunity** (small module: "services with tickets" reusable for repair, pressing, tailoring, cordonnerie).
**Sources**: https://www.fikrasahla.com/2025/02/projectPressing.html (project idea, not software) · https://hany.ma/ar/services/pressing (Morocco)

### 3.16 Salon de coiffure / esthétique / barbershop
**Needs**: ★ **agenda RDV, caisse, fidélité clients, paie coiffeurs CNAS** (Almawarid salon Jijel) [S]; ★ Clyo Systems lists "institut de beauté" [V]; ☆ services + product retail in one ticket, staff commission per service, packages/abonnements, gift vouchers [U].
**Sources**: https://almawarid.app/logiciel-salon-coiffure-jijel/ · https://www.clyosystems.dz/

### 3.17 Other verticals named by Algerian vendors (for completeness)
Household utensils (أواني منزلية), computer shops, paint shops (دهون), hats/helmets (كنكاري is hardware), pastry-supply & packaging, fuel stations (GestiumPRO), hotels, gyms, private schools, clinics, delivery/transport (Almawarid), **tourist parks** (Smart Resto), **production workshops** (POWER PRODUCTION). Sources: BNsoft TikTok, https://almawarid.app/logiciel-boulangerie-patisserie-algerie/ (list of verticals), https://www.cirtait.com/logiciel-gestion-commerciale/ar/.

---

## 4. Local habits & cross-cutting features (the "Algerian way" of running a shop)

### 4.1 الكريدي / carnet de crédit / دفتر الكريدي
- The neighbourhood **kridi** is a deep habit: Masroufi founder: most people rely on shop debts to cover daily needs (Echaab 2024) [V]; Almawarid supérette case with 30 credit customers [V]; Awras article "من دفتر الكريدي إلى التطبيقات البنكية" [S]; Konnash/Inyad built a whole app around it [S].
- **Required features**: customer account with credit limit; sale "à crédit" (full or partial); **versements** (partial repayments, any amount, any time) with receipt; running balance & statement (كشف حساب) printable/PDF/WhatsApp-shareable; **opening balance** import from the paper notebook; reminders; favourite/blacklist ("المتجر"); supplier side symmetrical (achats à crédit, versements fournisseur); aged debts; debt per customer on the POS screen before validating a credit sale.

### 4.2 Prices
- **Multiple price levels**: 4 prices (BNsoft), détail/gros/professionnel (Dolisoft), retail vs wholesale customer types (المتجر), multi-criteria tables, seasonal promos, per-customer discounts (GestiumPRO) [V/S].
- Margin per product family or per item; auto sale price from purchase price + margin ("المتجر") [S].
- **Remises**: line & ticket discount (% or DA), with permission limits; promotions ("soldes") [V Almawarid clothing].

### 4.3 Returns, exchanges, damaged goods
- "تحديث المبيعات والمشتريات بعد إرجاع السلع و تسيير السلع التالفة" (returns & damaged goods — المتجر) [S]; sales returns (Factor Facile / generic); pharmacy "Retours" regime [V]; supplier returns (expired) [U].

### 4.4 Caisse journalière / fermeture de caisse
- **Session with fond de caisse**, expected cash = float + cash sales, **écart** (shortage/surplus) detection (Almawarid POS) [V]; daily closure with payment breakdown (Lamacta) [V]; X (interim) and **Z** (final) reports [V generic]; **ticket Z "conforme DGI"** repeatedly marketed (Almawarid) [V]; cash vs treasury comparison, seller audits (Silwane) [V]; "الإغلاق اليومي" (GestiumPRO) [V].
- **Expenses from the till** (transport, rent, electricity, wages — المتجر) and **net profit after expenses** [S].
- **Zakat** computation (stock value + cash + receivables − payables; nisab) — BNsoft & المتجر [S]. ★ Culturally strong differentiator for a free product.

### 4.5 Hold / multi-ticket
- "خاصية الإنتظار بالنسبة للزبائن" (BNsoft) [S]; multiple open bills (DZ POS) [S]; table tickets (restaurant).

### 4.6 Payment methods
| Method | Status / notes | Source |
|---|---|---|
| Cash (espèces) | Dominant; subject to **droit de timbre** on receipts when paid cash (LF 2025) | see §4.7 |
| Chèque | Used (incl. controversial guarantee cheques in instalment sales); timbre applies to cheques handed directly | Echorouk [V], Intellix/fatoura [S] |
| **CCP** (compte chèque postal) / virement / **prélèvement CCP** | Instalment collection by CCP deduction (Vyvex) | [V] |
| **Edahabia** (Algérie Poste) & **CIB** (banks) on TPE | 18 M Edahabia + 4.5 M CIB cards; ~100,000 TPE for ~1.2 M merchants (Mar 2026); ~10 tx/TPE/month | Algérie Eco 2026-05-12 [V] |
| **BaridiMob / BaridPay QR** | QR payment via BaridiMob: static QR (fixed amount) or customer-entered amount; SMS confirmation both sides; merchant needs **commercial CCP**, phone, RC, NIF card, ID, crossed cheque; daily cap 500,000 DA; first phase targets pharmacies & neighbourhood shops; announced June 2025, officially launched ~28 Dec 2025 | https://www.baridimob.com/baridpay-qrcode-paiement-algerie/ [V] · https://observalgerie.com/2025/12/29/… [V] · https://www.algerie360.com/algerie-poste-jusqua-500-000-da-jour-… [S] · https://www.poste.dz/services/professional/baridi_pay_pro [S] |
| TPE obligation | 11 sectors in 2026 (grande distribution/supérettes, pharmacies, fuel stations, insurance, travel agencies, large restaurants, e-commerce, supermarkets, 3★+ hotels, large appliance stores, large clothing stores) — "progressive, not coercive" (Algérie360, 25 Jan 2026) [V]; other reports cite an 8-day ultimatum and **50,000 DA fine** [S] | https://www.algerie360.com/tpe-obligatoires-en-2026-voici-les-11-secteurs-concernes/ · https://econostrum.info/algerie/terminaux-de-paiement-electronique-commercants/ |
| Customer credit (kridi) | See §4.1 | — |
| AlgoStock already lists **Cash, CIB/Edahabia, BaridiMob, crédit client** as POS payment modes | [V] | https://algosynapse.com/algostock/ |

**Implication**: POS must record **mixed payments** (e.g., part cash, part BaridiMob QR, rest on credit), store a reference (QR transaction/SMS code, cheque no., CCP no.), and show a static BaridPay QR on the customer display/ticket. No public BaridPay API for POS integration was found [V: not mentioned on BaridiMob page] → manual confirmation.

### 4.7 Fiscal & legal items affecting the POS
- **Droit de timbre (LF 2025, art. 100)**: per 100 DA tranche (fraction counts): 300–30,000 DA → 1 DA; 30,001–100,000 → 1.5 DA; >100,000 → 2 DA; **minimum 5 DA**; ≤300 DA exempt; **electronic payments exempt**; applies to cash and cheque handed directly. Inspectors interpret differently (exact %, cumulative, +5 DA) → make the formula **configurable** (Intellix) [V/S]. Sources: https://www.intellixgroup.com/blog/algerie-droit-de-timbre-2025-… · https://www.grantthornton.dz/…/gt_loi-de-finances-2025-fr.pdf · https://fatoura.app/blog/droit-de-timbre-2025-les-nouveautes-de-la-loi-de-finance/
- **Invoice/ticket mentions**: RC, NIF, NIS, AI, company name, capital; sequential numbering without gaps; NIF/NIS 15-digit validation (PharmaPro) [V]; D.E. 05-468 on invoicing conditions (Almawarid) [V]; TVA 19 % / 9 % [V]; "Toute vente à un consommateur final doit donner lieu à un ticket ou une facture simplifiée" (Lamacta) [V].
- **E-invoicing**: PharmaPro claims auto-generated **e-invoices in JSON "ready for DGI submission"** under LF 2024 [V vendor claim; official format not verified here — cross-check with French/legal sweep].
- **Data protection**: loi 18-07 (personal data) cited by Almawarid [V] → local-first storage is a selling point.
- **Accounting**: SCF-compatible exports (Managely) [V].

### 4.8 Other recurring expectations
- **Offline-first** + **lifetime licence** + **free updates** (virtually every local vendor).
- **Arabic RTL + French** (+ English), switch without restart (AlgoStock) [V].
- **Excel import** of products/customers (Dolisoft) [V]; **automatic backups** to USB/network (AlgoStock), periodic DB archiving (المتجر) [V/S].
- **Remote owner notifications** via Telegram (PharmaPro) / WhatsApp (Managely, Teajar) [V].
- **AI**: invoice-from-photo (Factor Facile, AlgoStock, Pharmacium "Scaneye") — trend in 2025–2026 [V].
- **Multi-device on LAN** with per-user permissions (Factor Facile, AlgoStock) [V].
- Video tutorials on YouTube are part of the product ("مقاطع فيديو على اليوتيوب بالتفصيل" — BNsoft) [S].

---

## 5. Hardware ecosystem in Algeria (what the POS must drive)

| Device | Models / price seen in Algeria | Integration requirement | Source |
|---|---|---|---|
| Barcode label scale | **Rongta RLS1000** 89,900 DA (6/15 or 15/30 kg, 80 mm/s, **RS232 + Ethernet**); **Rongta RLS1100 30 kg** 88,500 DA; **Aclas LS2X** 109,000 DA; Aclas LS6 | Parse weight/price-embedded EAN-13; optional PLU push over Ethernet | https://diamond-tech-dz.com/product/balance-code-a-barre-rongta-rls-1000/ · https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-ls6/ · https://dz.loozap.com/ads/balance-etiqueteuse-aclas-superette../31747979.html |
| Thermal ticket printer | 80 mm from ~25,000 DA; 58 mm also common | ESC/POS 80/58 mm, Windows-driver fallback, Arabic printing (raster) | Symloop [V], PharmaPro [V], AlgoStock [V] |
| Cash drawer | from ~15,000 DA | RJ11 kick via printer | Symloop [V] |
| Barcode scanner | laser/2D from ~12,000 DA; USB HID | Keyboard-wedge input, fast debounce | Symloop [V], AlgoStock [V] |
| POS touchscreen 15" | from ~85,000 DA | Touch-optimised UI | Symloop [V] |
| Waiter tablet (Android) | from ~35,000 DA | LAN companion app | Symloop [V] |
| Kitchen display 19" | from ~45,000 DA | KDS screen or kitchen printer routing | Symloop [V] |
| Customer display (pole/2nd screen) | used in pharmacies | Show total / QR | Silwane [V] |
| Label printer (barcode stickers) | — | Print EAN-13/Code128 labels with price, AR/FR | BNsoft/Dolisoft [S/V] |

---

## 6. Pricing landscape (DZD) — what "free" competes against

| Product | Model | Price |
|---|---|---|
| Codeshade POS | one-time | 7,000 / 9,000 DA |
| AlgoStock | one-time + optional yearly | 19,500 DA (+5,000 or +12,000 DA/yr) |
| PharmaPro | one-time | 45,000 DA |
| Shabakett Officine | one-time (assumed) | 80,000–150,000 DA |
| Almawarid | yearly SaaS | Free / 10,000 / 18,000 DA per year |
| Symloop (restaurant/commerce) | yearly incl. install | 35,000 / 65,000 / 120,000 DA per year |
| Tassyir (e-commerce) | monthly | 1,900 / 4,900 / 9,900 DA per month |
| Pharmacium | monthly (as displayed) | 80,000–200,000 DA/month + TVA |
| Dolisoft, BNsoft, KERS POS, Factor Facile | lifetime licence | not published |
| Hardware bundle (screen+printer+drawer+scanner) | one-time | ≈137,000 DA (Symloop "from" prices) |

---

## 7. Consolidated feature checklist for DZBuild POS (derived from this research)

**Core POS**
- Keyboard-first + touch checkout; barcode, name, reference, DCI search; quick keys with images; hold/park tickets (multiple open); mixed payments (cash, chèque, CCP, BaridiMob QR, Edahabia/CIB TPE reference, credit); change calculation; line/ticket discounts with permission; returns/exchanges/avoirs; price levels (≥4: détail, demi-gros, gros, professionnel/revendeur) + per-customer tariffs; weighed items (scale EAN-13, decimal qty); units & conversions (carton↔pièce, mètre, m², kg, sac, palette); serial/IMEI capture at sale; lots/expiry FEFO; ticket 80/58 mm AR/FR, A4/A5 invoice, BL; customer display with static BaridPay QR; drawer kick.

**Caisse**
- Sessions (open with fond de caisse, X report, Z close, écart), multi-cashier, cash in/out, expenses from till, daily report by payment type, timbre calculation (configurable formula), sequential numbering.

**Credit (kridi)**
- Customer/supplier accounts, opening balances, credit limits, versements, statements (print/PDF/WhatsApp share), reminders, blacklist, aged balance, instalment contracts & schedules (vente à tempérament) with CCP/cash/cheque collection and arrears.

**Stock**
- Multi-depot/van, transfers, inventory counts (with phone/scanner), min-stock alerts, expiry alerts, lots, serials, variants (size/colour matrix), locations/shelves, supplier refs + equivalences/alternate codes + vehicle compatibility tags, landed cost (douane/transport), damaged/loss write-offs, production/recipes (bakery, restaurant).

**Documents & purchases**
- Devis → commande → BL → facture → avoir; proforma; purchase orders, receptions, supplier returns; RC/NIF/NIS/AI on documents; TVA 19/9 %; Excel/CSV import-export of everything; supplier invoice from photo (optional, later).

**Vertical packs (toggle per shop type)**
- Restaurant/café (tables, waiter LAN app, kitchen printers/KDS, supplements, modes sur place/emporter/livraison), bakery (production vs sales, special orders with deposit), butcher/greengrocer (scale), pharmacy/parapharmacie (lots/FEFO; Chifa later), quincaillerie/matériaux (units, devis chantier, contractor credit ceilings, deliveries), auto parts (OEM/equivalences), clothing (variants, soldes, exchanges), phone/electronics (IMEI, warranty, repair tickets, flexy log), installment sales (contracts, schedules, scoring), wholesale (tournées, vendeurs, commissions), optique (prescriptions, lab orders), pressing/repair/tailoring ("service tickets"), salon (RDV + services), librairie (school-list packs, photocopy services).

**Reports**
- Sales/profit per period, net profit after expenses, top/slow movers, margins by family, stock valuation, debts, **Zakat**, per-cashier, per-vendeur.

**Platform**
- Windows installer, 100 % offline, local DB, auto backup (USB/network/cloud via DZBuild), AR RTL/FR/EN, roles & permissions, LAN multi-poste, optional DZBuild OAuth sync (backup, owner dashboard, e-commerce catalogue link), silent updates, Telegram/WhatsApp owner alerts (when online).

---

## 8. Gaps & opportunities for a free DZBuild POS

1. **No single product covers all verticals well**: pharmacy tools are pharmacy-only; restaurant tools are restaurant-only; general tools (BNsoft, KERS, AlgoStock) are shallow on restaurant, scale, instalments, pressing. A free modular app with vertical packs is unmatched.
2. **Free vs 7,000–150,000 DA lifetime licences** — strongest lever is price; second is trust (open data export, local data, no lock-in).
3. **Underserved**: butcher/scale integration, pressing, librairie rentrée packs, instalment sales (only Vyvex), multi-price demi-gros with tournées for small wholesalers.
4. **Cultural differentiators**: Zakat report, kridi first-class, Arabic-first UI, Darija-friendly labels, WhatsApp statements, BaridPay QR display.
5. **Compliance as a feature**: timbre calculator (configurable), RC/NIF/NIS/AI enforcement, Z report, TVA 19/9 — marketed by competitors as "conforme DGI".
6. **DZBuild link**: optional cloud backup, owner dashboard, e-commerce sync (Tassyir-like order flow) — competitors charge yearly for mobile/cloud add-ons (AlgoStock +5,000–12,000 DA/yr).
7. **Avoid**: guarantee-cheque workflows (legal risk, Echorouk) — model instalment guarantees as documents/guarantor instead.

---

## 9. Source index (all URLs cited)

Vendors / products
- https://devion.cloud/factorfacile
- https://kers-pos.shop/ · https://kers-pos.shop/blog-best-pos-algeria.php · https://www.youtube.com/watch?v=aG9tJC29Wpw
- https://codeshade.app/
- https://www.tiktok.com/@bnsoft_/video/7500743518688742662
- https://www.youtube.com/watch?v=0Bdx2hPSNYs · https://www.youtube.com/watch?v=bb_mlKs-g9E (YASSNA-POS)
- https://www.youtube.com/watch?v=lotrFNyo6Qo (POWER PRODUCTION)
- https://www.youtube.com/watch?v=sRxZ9EgcsrM (DIGISOFT GOLD)
- https://www.youtube.com/watch?v=wvBpdljTh9Q (Infinity AI)
- https://www.youtube.com/watch?v=U7rvBiu_TSo
- https://www.facebook.com/cposdz/ · https://www.facebook.com/LinkSoftLogiciel/ · https://www.facebook.com/dz.yellowstore/videos/3956026711338527/ · https://www.facebook.com/LSCDEV/videos/811969174081991/
- https://www.ouedkniss.com/applications-logiciels-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A7%D9%84%D9%85%D8%AD%D9%84%D8%A7%D8%AA-%D8%A7%D9%84%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A%D8%A9-tizi-nbechar-setif-algerie-d13827110
- https://www.ouedkniss.com/logiciels-abonnements-%D8%A8%D8%B1%D9%86%D8%A7%D9%85%D8%AC-hirosystem-%D9%84%D8%AA%D8%B3%D9%8A%D9%8A%D8%B1-%D8%A3%D9%8A-%D9%86%D8%B4%D8%A7%D8%B7-%D8%AA%D8%AC%D8%A7%D8%B1%D9%8A-birtouta-alger-algerie-d52265413
- https://shop.fibladi.com/ar/algerie/detail/www.ouedkniss.com-15722662.htm (Dimastore)
- https://ar.newsoft-dz.com/index.php/2017-11-07-20-11-13 (Stock X)
- https://algosynapse.com/algostock/
- https://almawarid.app/ and vertical pages: /logiciel-epicerie-superette-algerie/, /logiciel-boulangerie-patisserie-algerie/, /logiciel-pieces-auto-algerie/, /logiciel-pret-a-porter-algerie/, /logiciel-materiaux-construction-algerie/, /logiciel-quincaillerie-outillage-algerie/, /logiciel-grossiste-distribution-algerie/, /logiciel-pharmacie-algerie/, /logiciel-supermarche-alger/, /logiciel-gestion-restaurant-oran/, /logiciel-salon-coiffure-jijel/, /blog/logiciel-caisse-pos-algerie-point-de-vente/
- https://www.cirtait.com/logiciel-gestion-commerciale/ar/ · https://www.cirtasoft.com/secteurs/quincaillerie/ · https://www.cirtait.com/pharmacium/
- https://dzcompta.cloud/ar/stock/
- https://managely.cloud/ar/best-pos-software-in-algeria
- https://tassyir.io/
- https://pharmapro.tkawen.com/
- https://www.shabakett.com/products/shabakett-officine
- https://xpertsoft-dz.com/xpertpharm.php
- https://www.intellixgroup.com/ar/logiciel-pharmacie-silwane-signature
- https://ctib.dz/adm/gfichiers/upload/files/MesDoc/Manuel_cti_pharm.pdf
- https://dolisoft-dz.com/products/promo-dolisoft-logiciel-de-gestion-complet-pour-magasins-de-pieces-detachees-1
- https://www.general-it.dz/logiciel-de-gestion-des-pieces-detachees/ · https://www.general-it.dz/logiciel-de-gestion-de-quincaillerie/
- https://www.megasoft-office.com/logiciel-mega-auto/
- https://epdigitaldz.com/logiciel-gestion-stock/quincaillerie/
- https://www.solutionsinformatiques.dz/Logiciel-de-Gestion-de-stock-Algerie?lang=fr · https://www.solutionsinformatiques.dz/RestPlus-un-progiciel-de-gestion-des-cantines-et-des-restaurants-toutes-options?lang=fr
- https://www.basmatec.com/?page_id=145
- https://www.clyosystems.dz/ · https://www.facebook.com/ClyoSystemsAlgerie/
- https://www.protid.com/logiciels-algerie/gestion-restaurants-en-algerie/
- https://www.innova-dz.com/ · https://ithreeweb.com/entreprises/logiciel-de-gestion-de-restaurant-algerie/
- https://www.bistro.icu/ · https://arabusinesse.com/ · https://www.instagram.com/p/DXgZyo6jUJn/
- https://symloop.com/ar/blog/logiciel-caisse-pos-restaurant-commerce-setif-2026/
- https://www.natrix-soft.com/2019/11/gesrep.html · https://dzgsm.com/vb/threads/96766/ · https://dzgsm.com/vb/threads/65279/ · https://dzgsm.com/vb/threads/63815/ · https://dzphonehub.online/
- https://www.vyvex.app/ar
- https://www.nesoft.dz/nosproduits.php
- https://flexy-pro.com/?lang=ar
- Play Store: com.inyad.store, com.invyad.konnash, com.ngctechnologies.dzposAPP, com.ahmedelshazly2020d.sales_managers, com.alcashier, com.izam.daftraPos, omnipos.restaurant.pos, com.centuryegg.pdm, ougmos.elokaz.flexy
- https://www.mahaal.app/ar/… · https://inyad.com/
- Arabic foreign vendors: https://dexef.com/software/dexef-cashier/ · https://www.daftra.com/ · https://alcashier.com/ · https://albadrsales.com/ · https://kashierapp.com/ · https://fatoorah.ai/ · https://pioneers-solutions.com/

Hardware
- https://diamond-tech-dz.com/product/balance-code-a-barre-rongta-rls-1000/
- https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-2/ · https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-ls6/
- https://www.ouedkniss.com/autre-balance-code-a-barre-rongta-rls1000-blida-algerie-d25104259
- https://dz.loozap.com/ads/balance-etiqueteuse-aclas-superette../31747979.html

Habits, payments, regulation, context
- https://www.echaab.dz/2024/09/19/… (Masroufi / kridi)
- https://www.awras.com/من-دفتر-الكريدي-إلى-التطبيقات-البنكي/
- https://www.echoroukonline.com/تجار-يشترطون-صكوك-ضمان-وزبائن-عرضة-للس… (guarantee cheques)
- https://www.elmasdaronline.dz/04/01/2024/… (instalment shops)
- https://www.alsalamalgeria.com/ar/produits/detail-312-47.html
- https://www.baridimob.com/baridpay-qrcode-paiement-algerie/ · https://observalgerie.com/2025/12/29/economie/algerie-poste-le-paiement-par-qr-code-arrive/ · https://www.algerie360.com/algerie-poste-jusqua-500-000-da-jour-desormais-autorises-via-ce-mode-de-paiement/ · https://www.poste.dz/services/professional/baridi_pay_pro · https://maghrebemergent.news/fr/paiement-mobile-algerie-poste-lance-le-paiement-mobile-par-qr-code-via-baridimob/
- https://www.algerie360.com/tpe-obligatoires-en-2026-voici-les-11-secteurs-concernes/ · https://algerie-eco.com/2026/05/12/algerie-100-000-tpe-pour-12-million-de-commercants/ · https://econostrum.info/algerie/terminaux-de-paiement-electronique-commercants/
- https://www.intellixgroup.com/blog/algerie-droit-de-timbre-2025-loi-de-finance-journal-officiel-probleme-et-solutions · https://www.grantthornton.dz/contentassets/2cd27e8e1c9e4a3e818486914e27a39c/mes-pages/homepage/gt_loi-de-finances-2025-fr.pdf · https://fatoura.app/blog/droit-de-timbre-2025-les-nouveautes-de-la-loi-de-finance/
- https://lamacta.com/blog/logiciel-caisse-algerie-commerce/
- https://www.algerie360.com/il-genere-des-taches-administratives-fastidieuses-sans-permettre-une-bonne-gestion-le-remede-chifa-rend-malades-les-pharmaciens/
- https://cnas.dz/en/chifa-3/ · https://ar.awrasaljazair.com/carte-e-chifa-%D8%A8%D8%B7%D8%A7%D9%82%D8%A9-%D8%A7%D9%84%D8%B4%D9%81%D8%A7%D8%A1-%D8%A7%D9%84%D8%A7%D9%81%D8%AA%D8%B1%D8%A7%D8%B6%D9%8A%D8%A9/
- https://www.acuite.fr/actualite/economie/2887/algerie-11-opticiens-lunetiers-rejoignent-la-cnas
- https://www.education.gov.dz/2025/09/03/… (school supplies list 2025-2026)

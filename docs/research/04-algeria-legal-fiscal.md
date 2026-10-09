# 04 — Algerian legal, fiscal & commercial requirements for a POS / invoicing ("gestion commerciale") software

> Research date: 2026-09-28. Scope: what a free, offline-first Windows POS + stock/invoicing product offered by DZBuild must respect in Algeria (2024-2026 law, including **LF 2025** = loi n° 24-08 du 24/11/2024 and **LF 2026** = loi n° 25-17 du 14/12/2025, JORADP n° 88 du 31/12/2025).
>
> Method: primary sources first (JORADP official gazette PDFs, DGI website mfdgi.gov.dz, the official 2026 editions of the fiscal codes published by the DGI, DGI circulars, ANPDP). Where only secondary sources exist, it is flagged. Anything not verifiable is marked **UNVERIFIED**.
>
> Note on method: search engines stopped answering reliably mid-research, so the second half relied on direct fetches of known official URLs (JORADP, DGI Dropfiles API, ANPDP, GIE Monétique, SATIM, Algérie Poste, Chargily docs, GitHub). This is why some items (Ramadan hours, Maystro/Noest API docs, interbank QR standard) are UNVERIFIED.

---

## 0. Executive summary: the 15 things the POS must get right

| # | Requirement | Legal basis (verified) | Impact on product |
|---|---|---|---|
| 1 | **Invoice mandatory mentions** (seller + buyer identity, legal form, capital, RC, NIS, payment mode & due date, date + sequential number, designation/qty, PU HT, total HT, nature & rate of each tax, **TTC in figures AND in letters**) | Décret exécutif 05-468 art. 3 | Invoice template engine with validation that blocks printing if a mandatory field is missing |
| 2 | **NIF** of seller & buyer is an essential mention; omitting it = "défaut de facturation" (fine 80 % of amount) | Loi 04-02 art. 33-34 | NIF mandatory for B2B customers |
| 3 | **Consumer sales → ticket de caisse or "bon"**; facture only if customer asks | Loi 04-02 art. 10 (as amended by loi 10-06) ; LFC 2018 art. 4 added "ticket de caisse" to art. 12 | Default output = ticket; one-click "convert to facture" |
| 4 | **Numbering**: uninterrupted, chronological series; cancelled invoice kept with "facture annulée" diagonal mention; no erasures | Décret 05-468 art. 10 | Gapless sequence per document type; void ≠ delete |
| 5 | **TVA 19 % / 9 % / exempt**, shown distinctly (HT, rate, amount). **IFU taxpayers may NOT show TVA** (invoice TTC only) | CTCA art. 21, 23, 64 | Per-product tax code; regime switch changes invoice layout |
| 6 | **Droit de timbre on cash receipts**: 1 DA / 1.5 DA / 2 DA per 100 DA (or fraction) depending on bracket, **flat on the whole amount (not progressive)**, min 5 DA, ≤ 300 DA exempt, **no ceiling**; exempt when paid electronically / cheque / transfer | Code du timbre art. 100-I, 258, 258 quinquies ; LF 2025 art. 46-47 ; DGI circulaire n° 14/2025 | Exact algorithm in §4; "Net à payer = TTC + timbre" only for cash tender |
| 7 | **From 1 Jan 2027: cash-register / management software used for non-invoiced sales must have an "engagement de l'éditeur" or an accredited certificate** attesting **inaltérabilité, sécurisation, conservation, archivage** | LF 2026 art. 58 (CTCA 51 bis), 74 (CPF 20 ter), 81 (CPF 51 sexies), 109 (effective date) | DZBuild must design for inalterability now (hash-chained journal, Z-close, archives) and be ready to sign the publisher attestation once the arrêté fixes its model |
| 8 | **Data retention**: books & vouchers 10 years; IT data/processing documentation 6 years after the year | CPF art. 64 (as amended by LF 2026 art. 82); Code de commerce art. 12 | Never purge fiscal data; archive export |
| 9 | **Monthly G50** (réel) by the 20th; **G9 "état fournisseurs"** annex for TVA deduction (NIN, NIF, name, address, RC, invoice date/ref, TTC, TVA deducted); **état clients** annual for wholesale (name, NIF, RC, AI, address, HT, TVA) | CTCA art. 29, 76 ; CIDTA art. 183 ter ; DGI forms | Built-in G50 helper, G9 and état-clients exports (Excel/CSV) |
| 10 | **IFU**: threshold **8,000,000 DA** (not 15 M as some blogs say), rates 5 % (goods) / 12 % (other) / 0.5 % (auto-entrepreneur), minimum 30,000 DA (10,000 DA AE), G12 by 30 June, G12 bis by 20 January, purchase & sales registers | CIDTA 282 ter-sexies, 365 ; DGI IFU page (updated 20/09/2026) | IFU mode with annual CA tracker & threshold alerts |
| 11 | **Merchants must offer an electronic payment instrument** to consumers (fine 50,000 DA; deadline was 31/12/2024) | LF 2018 art. 111 as amended by LF 2024 art. 113 | Tender types: CIB/Edahabia TPE, QR/BaridiMob, transfer; record auth refs |
| 12 | **Personal data**: law 18-07 **as amended by loi 25-11 (24 July 2025)**: prior declaration to ANPDP, consent, DPO, processing register, **automated access log**, DPIA, **breach notification within 5 days**, cross-border transfer needs ANPDP authorization | Loi 18-07 art. 7, 12-14, 32, 38, 44 ; 41 bis-41 bis 3, 45 bis 6-10 | Local-first storage in Algeria; audit log of personal-data access; consent flags; export/erase tools |
| 13 | **Arabic**: consumer information must be "essentiellement en langue arabe" | Décret 13-378 art. 7 | Bilingual AR/FR tickets, labels, RTL printing |
| 14 | **69 wilayas** since loi 26-06 (4 April 2026); 1,541 communes unchanged | Loi 26-06 (secondary sources citing JO n° 25/2026) | Ship both 58- and 69-wilaya datasets + mapping (carriers may lag) |
| 15 | **Amount in letters** mandatory on invoice (FR and ideally AR) | Décret 05-468 art. 3 | Own FR/AR number-to-words engine (existing libs are buggy for DZD, see §8) |

---

## 1. Map of the legal texts that matter

| Text | What it governs for a POS | Source |
|---|---|---|
| **Loi 04-02 du 23/06/2004** (pratiques commerciales), amended by **loi 10-06 du 15/08/2010** and **LFC 2018 (loi 18-13) art. 4** | Price display, obligation of facture / ticket, sanctions, closure of shop | JO 2004 n° 41 https://www.joradp.dz/FTP/jo-francais/2004/F2004041.pdf ; JO 2010 n° 46 https://www.joradp.dz/FTP/jo-francais/2010/F2010046.pdf ; LFC 2018 art. 4 reproduced in CTCA 2026 edition https://www.mfdgi.gov.dz/files/803/2026/3725/CodedesTaxessurle-Chiffred-Affaires2026fr.pdf |
| **Décret exécutif 05-468 du 10/12/2005** | Facture, bon de transfert, bon de livraison, facture récapitulative | JO 2005 n° 80 https://www.joradp.dz/FTP/jo-francais/2005/F2005080.pdf (also listed on https://www.commerce.gov.dz/fr/reglementation/decret-executif-n05-468) |
| **Décret exécutif 16-66 du 16/02/2016** | "Bon de transaction commerciale" (document tenant lieu de facture) for agriculture, fishing, aquaculture, artisanat | JO 2016 n° 10 https://www.joradp.dz/FTP/jo-francais/2016/F2016010.pdf |
| **Code des taxes sur le chiffre d'affaires (CTCA)** 2026 ed. | TVA rates, base, deduction, invoice TVA display, IFU no-TVA rule, **new art. 51 bis (cash systems)** | https://www.mfdgi.gov.dz/files/803/2026/3725/CodedesTaxessurle-Chiffred-Affaires2026fr.pdf |
| **Code du timbre (CT)** 2026 ed. | Timbre de quittance (art. 100), exemptions (258-258 quinquies), payment "sur état" (art. 4, 19) | https://www.mfdgi.gov.dz/files/803/2026/3724/CodedeTimbre2026fr.pdf |
| **Code des procédures fiscales (CPF)** 2026 ed. | Retention (art. 64), IFU obligations (art. 1), **new art. 20 ter (computerised accounting audit) and 51 sexies (software publishers)** | https://www.mfdgi.gov.dz/files/803/2026/3723/CodedesProceduresFiscales2026fr.pdf |
| **Code des impôts directs (CIDTA)** 2026 ed. | IFU (282 bis-365), état clients (183 ter), réel regime, TAP abolished | https://www.mfdgi.gov.dz/files/803/2026/3726/CodedesImpotsDirectsetTaxesAssimilees2026fr.pdf |
| **LF 2025** (loi 24-08) | Timbre de quittance reform (art. 46-47), IFU minimum 30,000 DA (art. 29), IFU exclusions (art. 22), mandatory bank payment for some goods (art. 207) | Grant Thornton summary https://www.grantthornton.dz/contentassets/2cd27e8e1c9e4a3e818486914e27a39c/mes-pages/homepage/gt_loi-de-finances-2025-fr.pdf ; DGI circulars compilation https://www.mfdgi.gov.dz/files/583/2020-2025/3727/Recueil-des-circulaires-d-application-de-la-Loi-de-Finances-pour-2025.pdf |
| **LF 2026** (loi 25-17) | Cash-system certification from 2027, TVA fines ×10-×50, e-filing mandatory for réel, IFU contractual regime abandoned, 9 % TVA extensions, exemptions extended to 31/12/2026 | Full text (JO n° 88 du 31/12/2025) https://www.mfdgi.gov.dz/files/537/20212026/3710/LF2026.pdf ; DGI communiqué https://mfdgi.gov.dz/files/658/Communiques-de-loi-de-finances/3378/Communique-general-de-la-loi-de-finances-2026.pdf ; Deloitte https://blog.avocats.deloitte.fr/algerie-les-principales-mesures-de-la-loi-de-finances-pour-2026/ |
| **Loi 18-07 du 10/06/2018** (personal data) amended by **loi 25-11 du 24/07/2025** | Declaration, consent, security, DPO, logs, breaches, transfers | JO 2018 n° 34 https://www.joradp.dz/FTP/jo-francais/2018/F2018034.pdf ; ANPDP consolidated text https://anpdp.dz/ar/storage/2025/08/18-07-Edited.pdf |
| **Loi 18-05 du 10/05/2018** (e-commerce) | E-merchant obligations, invoice, COD, payment platforms, returns | JO 2018 n° 28 https://www.joradp.dz/FTP/jo-francais/2018/F2018028.pdf |
| **LF 2018 art. 111** (as amended, last by LF 2024 art. 113) | Obligation to offer electronic payment to consumers | JO 2017 n° 76 https://www.joradp.dz/FTP/jo-francais/2017/F2017076.pdf ; consolidated in CPF/CIDTA 2026 non-codified annex |
| **Décret exécutif 15-153 du 16/06/2015** | Cash-payment ceilings (5 M DA real estate; 1 M DA vehicles, yachts, precious metals, antiques, auctions) | JO 2015 n° 33 https://www.joradp.dz/FTP/jo-francais/2015/F2015033.pdf |
| **Décret exécutif 13-378 du 09/11/2013** | Consumer information: Arabic language, labelling, lot, DLC/DDM | JO 2013 n° 58 https://www.joradp.dz/FTP/jo-francais/2013/F2013058.pdf |
| **Loi 22-23 du 18/12/2022** | Statut de l'auto-entrepreneur | JO 2022 n° 85 https://www.joradp.dz/FTP/jo-francais/2022/F2022085.pdf |
| **Loi 26-06 du 04/04/2026** | 69 wilayas | Secondary: https://fr.wikipedia.org/wiki/Wilayas_d%27Alg%C3%A9rie ; datasets citing "JO n° 25 du 5 avril 2026" (e.g. https://github.com/islam-re/Algeria-wilayas). JO PDF not fetched → reference number/date **cross-checked by 3+ secondary sources, JO page not personally verified** |

---

## 2. Commercial documents (documents commerciaux)

### 2.1 Facture — décret exécutif 05-468, verbatim requirements

Source: JO n° 80 du 11/12/2005, pp. 16-18 (https://www.joradp.dz/FTP/jo-francais/2005/F2005080.pdf). Entered into force 6 months after publication (art. 19); abrogated décret 95-305 (art. 20).

**Art. 2** — every sale of goods / service **between economic agents** must be invoiced; seller must deliver it, buyer must request it, **at the moment of the sale/service**. With a consumer, the invoice is mandatory **if the consumer asks**.

**Art. 3-1° Mentions relatives au vendeur** (verbatim list):
- nom et prénom(s) de la personne physique;
- dénomination ou raison sociale de la personne morale;
- adresse, numéros de téléphone et de fax ainsi que, le cas échéant, l'adresse électronique;
- forme juridique de l'agent économique et nature de l'activité;
- **capital social, le cas échéant**;
- numéro du registre du commerce;
- numéro d'identification statistique (NIS);
- **mode de paiement et date de règlement de la facture**;
- date d'établissement et **numéro d'ordre** de la facture;
- dénomination et quantité des biens vendus et/ou des prestations;
- prix unitaire hors taxes;
- prix total hors taxes;
- **nature et taux des taxes et/ou droits et/ou contributions dus**, suivant la nature des biens/prestations. *La TVA n'est pas mentionnée si l'acheteur en est exonéré*;
- **prix total toutes taxes comprises, libellé en chiffres et en lettres**.

**Art. 3-2° Mentions relatives à l'acheteur**: nom et prénom(s) / dénomination ou raison sociale; forme juridique et nature de l'activité; adresse, téléphone, fax, email le cas échéant; numéro RC; NIS. **If the buyer is a consumer: nom, prénom(s) et adresse.**

**Art. 4** — invoice must bear the seller's **cachet humide (wet stamp) and signature**, *except* when issued "par voie télématique" under art. 11 (not usable for public-expenditure payments). Public-service operators issuing very large volumes may keep their current form.

**Art. 5-6** — TTC price includes rabais / remises / ristournes granted at sale time. Definitions: *remise* = quantity/quality/profession discount; *rabais* = compensation for late delivery/defect; *ristourne* = loyalty rebate computed on HT turnover over a period.

**Art. 7** — transport costs, if not invoiced separately or not included in the unit price, must be listed **in the margin** of the invoice.

**Art. 8** — price supplements (interest for credit sales, intermediaries' fees, commissions, brokerage, insurance premiums re-invoiced) must be listed expressly.

**Art. 9** — sums for **returnable packaging deposit (consignation)** and costs advanced on behalf of a third party must appear if not separately invoiced.

**Art. 10** — invoice must be **legible, no stains, erasures or overwriting**. It is regular if taken from a stub book ("facturier") **or established in dematerialised form via a computer process**. The facturier is "une série **ininterrompue et chronologique**"; a new facturier cannot be started before the previous one is exhausted. A regularly cancelled invoice must carry **"facture annulée" written diagonally**.

**Art. 11** — invoices may be established and transmitted "par voie télématique" (tele-transmission) under procedures to be set by a **joint arrêté** (commerce, finance, telecoms). → Status of that arrêté: **UNVERIFIED (not found)**.

**Chapitre II — Bon de transfert (art. 12-13)**: required when moving own goods between own storage / processing / packaging / selling units without a sale. Dated and numbered; accompanies goods; shows name/raison sociale, address/phones, RC number, nature & quantity of goods, **addresses of origin and destination**, signature + wet stamp, **name of driver/carrier and proof of capacity**.

**Chapitre III — Bon de livraison & facture récapitulative (art. 14-17)**: the BL may **replace** the invoice only for **repetitive, regular sales to the same customer**, and only with an **express authorisation decision from the commerce administration** (art. 16). The BL must show the **number and date of that authorisation decision**, the **name and ID-card number of the deliverer/carrier**, plus all art. 3 & 4 mentions. A **monthly facture récapitulative** per customer must be issued at month end, referencing **the numbers and dates of all BLs**.

**Art. 18** — infringements sanctioned under loi 04-02.

> Product implication: the "BL → facture récapitulative" workflow is legal only for authorised merchants; the software should store the *décision d'autorisation* number/date in settings and refuse BL-as-invoice mode if empty (or clearly label BLs as non-fiscal "bons de livraison" that must be followed by an invoice).

Secondary summaries that match the JO text: https://www.drcblida.dz/index.php/fr/informations-utiles/74-facture ; https://legal-doctrine.com/en/edition/quelles-sont-les-conditions-et-modalites-detablissement-dune-facture-en-algerie.
Caution: some blogs (e.g. https://lamacta.com/blog/decret-executif-05-468-facture-legale-algerie/, https://almawarid.app/blog/facture-conforme-algerie-decret-05-468-guide-complet/) list NIF/AI/RIB "per décret 05-468" and claim "numbering per exercice" and "DGI homologation of software" — the decree text itself does **not** say that (see 2.2 and §6 for where NIF/AI actually come from).

### 2.2 Mentions required by *other* texts (tax law)

| Mention | Where it comes from | Notes |
|---|---|---|
| **NIF** (numéro d'identification fiscale) seller & buyer | Loi 04-02 art. 34: omission of name/raison sociale, **NIF**, address, quantity, precise designation or unit price HT of seller **or buyer** = *défaut de facturation* (art. 33, fine 80 %) | Every person/entity must have a NIF (LF 1992 art. 110 per DGI https://www.mfdgi.gov.dz/fr/professionnels/nouveau-contribuable-fr/ouvrir-dossier-fiscal). Online NIF request/authentication portal: nifenligne.mfdgi.gov.dz |
| **NIS** | Décret 05-468 art. 3 ; LF 2002 art. 35 (failure to produce NIS suspends TVA franchise etc., quoted on DGI form G3) | Format per décret 97-396 art. 4-5: 15 digits (18 for secondary establishment) — per dz_kit README https://github.com/AbdeldjalilChougui/dz_kit (secondary) |
| **RC** | Décret 05-468 art. 3 | Auto-entrepreneurs have **no RC** but a *carte d'auto-entrepreneur* with a national registration number (loi 22-23 art. 6, 9) → template must allow "N° carte AE" instead of RC |
| **Article d'imposition (AI)** | Required in the **état clients** (CIDTA art. 183 ter) and in the header of form G9; not in décret 05-468 | Standard practice to print it; keep it mandatory for B2B customer records |
| **NIN** (numéro d'identification national, 18 digits) | Required for each supplier in **G9 état fournisseurs** (form 2025) and for employees in G29 (LF 2026 art. 8) | Store NIN for natural-person suppliers/customers |
| **TVA distinctly** | CTCA art. 64: invoices "doivent obligatoirement faire apparaître, d'une manière distincte, le montant de la TVA"; anyone who mentions TVA not actually paid is personally liable | DGI TVA page: invoice must show client identification, price HT, applicable TVA rate, TVA amount https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-reel/la-taxe-sur-la-valeur-ajoutee |
| **IFU: NO TVA on invoices** | CTCA art. 64 last §: IFU taxpayers "ne peuvent pas mentionner la TVA sur leurs factures" (sanction art. 114). DGI: IFU "facturation se fait en TTC" | Regime flag must hide TVA columns |
| **Specific taxes outside the TVA base, shown distinctly** | Tax on new imported tyres (750 DA heavy / 450 DA light vehicles) and tax on lubricating oils: "doit apparaître, de façon distincte, sur les factures établies à tous les niveaux de la distribution" (non-codified LF articles reproduced in CTCA 2026) | Product-level "extra tax" lines with a flag *in/out of TVA base* (e.g. tobacco additional tax is **inside** the TVA base per the same annex) |
| **Timbre** | Code du timbre art. 100 (see §4) | Printed as a separate line after TTC when the tender is cash |
| **RIB / RIP** | Not required by any text found (05-468 only requires "mode de paiement et date de règlement") | Recommended for B2B |

### 2.3 Consumer sales: ticket de caisse / bon

- Loi 04-02 art. 10 (version loi 10-06, JO 2010 n° 46): "Les ventes de biens ou les prestations de services faites au consommateur doivent faire l'objet d'**un ticket de caisse ou d'un bon justifiant la transaction**. Toutefois, la facture ou le document en tenant lieu doit être délivré si le client en fait la demande."
- LFC 2018 (loi 18-13) art. 4 rewrote art. 12: "La facture, le bon de livraison, la facture récapitulative, le bon de transfert **ainsi que le ticket de caisse**, doivent être établis conformément aux conditions et modalités fixées par voie réglementaire." → **The implementing decree defining ticket-de-caisse mentions was not found — UNVERIFIED whether it exists.** Safe design: print on every ticket the seller identity (name, address, RC, NIF, NIS, AI), sequential ticket number, date/time, cashier, lines (designation, qty, PU, amount), tax breakdown (if réel), total TTC, tender, timbre (if cash), and "Merci" text in AR/FR.
- The ticket is a "titre constatant un paiement" → subject to the timbre de quittance when paid cash (DGI circular 14/2025 explicitly lists "quittance, facture, **ticket de caisse**, titre de voyage").

### 2.4 Bon de transaction commerciale (document tenant lieu de facture) — décret 16-66

Source: JO 2016 n° 10, https://www.joradp.dz/FTP/jo-francais/2016/F2016010.pdf
- Applies to operators in **agriculture, fishing, aquaculture, artisanat & métiers** (art. 3); they must issue either this BTC or a facture.
- Mentions (art. 5): désignation; prix unitaire/DA; quantité; montant par produit/DA; montant total/DA; consignation of returnable packaging & costs advanced for a third party; signature + stamp of seller **and signature of buyer**; legible, no erasures; from a stub book **or electronic/dematerialised**, may be transmitted by telematics; uninterrupted chronological numbering; cancelled BTC barred diagonally with "ANNULE" in capitals. Prices can be provisional (ranges) when the buyer resells on behalf of the producer (art. 2).
- This is the legal basis for what the market calls "facture d'achat" / "bon d'achat" when a shop **buys** from farmers/fishermen/artisans who have no RC. → The POS purchase module should be able to generate/print a BTC for such suppliers.

### 2.5 Facture proforma, devis, bon de commande

- No Algerian legal text defining the facture proforma or devis was found (**UNVERIFIED / practice**). They are commercial offers, not fiscal documents: use a separate numbering series and print "FACTURE PROFORMA — ne constitue pas une facture". In practice proformas are required by banks for import domiciliation and by public buyers (practice, unverified).

### 2.6 Avoir (credit note), annulation, returns

- The décret only provides the cancellation mechanism ("facture annulée" diagonal). The concept of *facture d'avoir* is **not defined in the texts found** (practice).
- TVA side: **CTCA art. 18** — when a sale is later **résiliée, annulée ou impayée**, the TVA paid is imputed on later TVA due (or refunded if the person stopped being liable) by attaching to one of the next monthly returns an **"état spécial"** stating: (1) nature of the original operation and name/address of the counterparty, (2) its date, (3) the accounting register page (or special register art. 72), (4) amount refunded or not collected. **Deadline: 4 years from collection.**
- Product: credit note = new document in its own gapless series referencing the original invoice number/date, negative lines, reason code; generate the "état spécial art. 18" list automatically for the month.

### 2.7 Numbering: what is legally required vs. common practice

| Aspect | Law | Recommendation |
|---|---|---|
| Sequential, uninterrupted, chronological | Décret 05-468 art. 10 (facturier), décret 16-66 art. 5 (BTC) | Gapless counter per document type and per establishment/register, allocated inside the same DB transaction as the document |
| Reset each year | **Not stated** in the décret. Blogs claim "per exercice comptable" (e.g. Almawarid) — **UNVERIFIED** | Use prefix with year (FAC-2026-000001) — still unique & chronological; never reuse |
| Cancellation | "facture annulée" diagonal (05-468 art. 10) | Void status + watermark on reprint; keep in sequence |
| Retention | CPF art. 64: books 10 years from last entry, vouchers 10 years from their date (ref. Code de commerce art. 12) ; IT files kept **in electronic form**, data/processing documentation **until end of 6th year** following the year concerned (LF 2026 art. 82) | No hard delete; archive/export per fiscal year |

### 2.8 Sanctions (why the validations matter)

| Infraction | Sanction | Source |
|---|---|---|
| Défaut de facturation (no invoice/ticket, not presented on request) | **Fine = 80 % of the amount that should have been invoiced** | Loi 04-02 art. 33 |
| Facture non conforme (other than essential mentions) | 10,000 – 50,000 DA | Loi 04-02 art. 34 |
| Missing essential mentions (name, NIF, address, qty, designation, PU HT of seller or buyer) | treated as défaut de facturation (80 %) | Loi 04-02 art. 34 |
| Défaut d'information sur les prix (art. 4, 6, 7) | 5,000 – 100,000 DA | Loi 04-02 art. 31 |
| Illicit price practices (regulated prices/margins, price structures) | 20,000 – 10,000,000 DA | Loi 04-02 art. 36 (version loi 10-06) |
| Administrative closure of the shop | up to **60 days** by wali, for breaches of art. 4-14, 20, 22-28, 53 | Loi 04-02 art. 46 (version loi 10-06) |
| Recidivism within 2 years | penalty doubled; activity ban up to 10 years | Loi 04-02 art. 47 (version loi 10-06) |
| General TVA infringements | **25,000 DA** (was 500-2,500) ; fraud 100,000 DA | LF 2026 art. 59 (CTCA) |
| État clients missing | 2 % of annual CA; late: 30,000 / 50,000 / 80,000 DA; each error 1,000-10,000 DA | CIDTA art. 194-6 (quoted with 183 ter) |
| Aggravated tax fraud (organised, **cyber/IT-enabled**, national scale) | 5-10 years prison and/or 5-10 M DA | LF 2026 art. 25 (DGI communiqué) |

---

## 3. TVA (taxe sur la valeur ajoutée)

### 3.1 Rates (2026)

| Rate | Scope | Source |
|---|---|---|
| **19 %** | normal rate | CTCA art. 21 |
| **9 %** | list in CTCA art. 23 (long tariff-code list: live animals, fresh/chilled vegetables, dried legumes, rice, oats, sorghum, milling products, newsprint, books, gas cylinders…; electricity/gas below consumption thresholds; drinking water for households; cinema tickets; recyclable waste; agricultural plastic film; BUPRO; tilapia products; medical acts; education…) | CTCA 2026 ed. |
| 9 % — **new in LF 2026** (art. 53) | rehabilitation/viabilisation of old housing; meals & lodging of patients in health establishments; professional training by State-approved establishments (+ their accessory meals/lodging); **bus passenger transport**; (art. 138) raw materials & local feed for aquaculture | DGI circulaire n° 26/2026 https://www.mfdgi.gov.dz/files/804/2026/3740/Circulaire-n26-du-20-04-2026-relative-a-lextension-de-l-application-du-taux-reduit-d-certaines-prestations-TVA.pdf ; DGI communiqué LF 2026 |
| **Exempt** | CTCA art. 8-13 + many non-codified LF measures. **Extended to 31/12/2026**: import & sale of dried legumes and rice for human consumption; sale of **fresh fruits & vegetables, eggs, broiler chicken and turkey** produced locally (LF 2026 art. 112); frozen white meat imports (art. 140); robusta/arabica coffee imports (art. 142) | DGI communiqué LF 2026 ; circulaire n° 30/2026 |
| IFU taxpayers | outside TVA; invoice TTC; **cannot show TVA** | CTCA art. 64 ; DGI TVA page |

> Product: tax codes must be data, not code: {code, label AR/FR, rate, exempt flag, valid_from, valid_to}, with **end dates** (e.g. exemptions ending 31/12/2026) and a yearly "LF update pack" that the vendor can push. Exempt sales must still be reported (the G50 has "CA exonéré" boxes; LF 2024 art. 40 obliges declaring exempt CA monthly — DGI TVA page).

### 3.2 Base, fait générateur, HT/TTC

- Base = price of goods/works/services **"tous frais, droits et taxes inclus à l'exclusion de la TVA elle-même"** (CTCA art. 15). Hence excise-like taxes (TIC, tobacco additional tax) are in the base, except taxes explicitly excluded (tyres, lubricants, see 2.2).
- Fait générateur: sales → legal or physical delivery; services → collection (encaissement); (DGI TVA page).
- Display per 05-468/CTCA 64: PU HT, total HT, TVA rate & amount per rate, TTC.
- **Rounding: no Algerian text on rounding rules was found (UNVERIFIED).** Recommendation: store money as integer centimes; compute TVA per line or per rate on the invoice total consistently; round half-up to 0.01 DA; print totals per rate. Keep the method configurable (per-line vs per-document) because the G50 needs totals per rate that reconcile with the invoices.

### 3.3 Deduction and supplier data (what the purchases module must capture)

- CTCA art. 29: to deduct TVA, the monthly G50 must be supported by an **état fournisseurs** (also on digital medium) listing per supplier: **NIF, nom/raison sociale, adresse, RC, date et référence de la facture, montant des achats, TVA déduite**; NIF and RC "doivent être authentifiés selon la procédure en vigueur".
- Official form **Série G n° 9 (2025) "État des fournisseurs"** columns: **NIN, NIF, Nom-prénom/raison sociale, Adresse, N° RC, Date facture (or document), Référence facture, Montant TTC des achats, Montant TVA déduite**; header shows taxpayer NIF, NIN, AI. https://www.mfdgi.gov.dz/files/521/Formulaires-de-declaration/3142/G-9-Etat-fournisseurs--2025.pdf
- CTCA art. 30: **no TVA deduction on an invoice > 1,000,000 DA TTC paid in cash** (allowed if cash is deposited into a bank/postal account). Omitted deductions can be claimed until 20 December of the following year.
- → The purchases module must store supplier NIF/NIN/RC/address and the payment method of each supplier invoice, and flag cash-paid invoices > 1 M DA as non-deductible.

### 3.4 Monthly declaration G50 (régime réel)

- Due within the first **20 days** of the following month (CTCA; DGI TVA page; G50 notice https://www.mfdgi.gov.dz/files/521/Formulaires-de-declaration/63/Notice-comment-remplir-GN50.pdf). Payment may be made separately but within the same deadline.
- TVA part (page 2 of the G50): CA HT at 9 % (box 1), of which exempt (2), taxable (3), TVA 9 % (4); CA HT at 19 % (5), exempt (6), taxable (7), TVA 19 % (8); totals (9-12); then deductions (TVA on purchases, with G9 annex).
- Other G50 sections relevant to shops: **IRG salaires** withheld (monthly), **droit de timbre "sur état"** (section 4 — see §4.5), other taxes.
- **LF 2026 art. 111**: e-filing (Jibayatic) is **mandatory** for réel and régime simplifié taxpayers attached to Jibayatic-equipped offices; e-payment optional. For fiscal year 2025 paper was tolerated **except the G50, which must be e-filed** (DGI circulaire n° 31/2026 https://www.mfdgi.gov.dz/files/804/2026/3778/Circulaire-n31-du-05-05-2026-relative-aux-modalites-de-declaration-et-de-paiement-des-impots-et-taxes.pdf).
- A "G50 néant" must be filed even when no CA (DGI TLS page).
- The DGI occasionally extends deadlines (e.g. "Prorogation des délais de souscription des déclarations G50, G1 et G12", DGI news July 2026) → deadline dates must be editable in the app's fiscal calendar.

### 3.5 TAP and TLS

- **TAP (taxe sur l'activité professionnelle) is abolished**: CIDTA Titre III (art. 217-231) "abrogés" by **LF 2024 art. 14** (CIDTA 2026 ed.). The old G50 notice still shows TAP boxes — ignore them.
- **TLS (taxe locale de solidarité)** only applies to hydrocarbon pipeline transport (3 %) and mining (1.5 %) (DGI TLS page https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-reel/tls). Not relevant for normal retail/wholesale.

---

## 4. Droit de timbre on cash payments (timbre de quittance) — exact rule

### 4.1 Legal text (Code du timbre 2026, art. 100-I, as modified by LF 2025 art. 46)

> "Les Titres de quelle que nature qu'ils soient, signés ou non signés, faits sous signatures privées, qui comportent libération ou qui constatent des paiements ou des versements de sommes, sont assujettis à un droit de timbre dont la quotité est fixée **par tranche de 100 DA ou fraction de tranche de 100 DA**, comme suit : — Sommes dont le montant est supérieur à 300 DA et n'excédant pas les 30 000 DA : **1 DA** ; — Sommes dont le montant est supérieur à 30 000 DA et n'excédant pas les 100 000 DA : **1,5 DA** ; — Au-delà de la somme de 100 000 DA : **2 DA**. Toutefois, le montant du droit dû ne peut être inférieur à **5 DA**."

Source: https://www.mfdgi.gov.dz/files/803/2026/3724/CodedeTimbre2026fr.pdf (art. 100; footnote: modified by LF 2023 art. 18 and **LF 2025 art. 46**). Before 2025 the rule was 1 DA per 100 DA with a ceiling of 10,000 DA (DGI circular). **LF 2026 did not change art. 100** (not in the DGI communiqué LF 2026 or the LF 2026 text).

### 4.2 Official interpretation — DGI circulaire n° 14/MF/DGI/LF.2025 (05/03/2025)

(Reproduced in the DGI compilation of LF 2025 circulars, pp. 61-63: https://www.mfdgi.gov.dz/files/583/2020-2025/3727/Recueil-des-circulaires-d-application-de-la-Loi-de-Finances-pour-2025.pdf)

- Tariffs: ≤ 300 DA **exempt**; 301-30,000 DA → 1 DA per 100 DA; 30,000.01-100,000 DA → 1.5 DA per 100 DA; > 100,000 DA → 2 DA per 100 DA.
- **"Il est à signaler que ce droit de timbre n'est pas soumis à une progressivité par tranche"** → the rate of the bracket applies to the **whole amount** (not marginal).
- **"Toute tranche de somme inférieure à 100 DA est comptée comme une tranche entière de 100 DA."**
- No ceiling; minimum 5 DA.
- Scope: "quittance, facture, **ticket de caisse**, titre de voyage…".
- Official examples: ticket 25,000 DA cash → 250 × 1 = **250 DA**; invoice 35,000 DA cash → 350 × 1.5 = **525 DA**; invoice 350,000 DA cash → 3,500 × 2 = **7,000 DA**.

> Several software vendors/blogs discussed a "cumulative/marginal" reading (e.g. https://www.intellixgroup.com/blog/algerie-droit-de-timbre-2025-loi-de-finance-journal-officiel-probleme-et-solutions). The DGI circular settles it: **not progressive**. Some blogs also still say "1 % capped at 2,500/10,000 DA" (old rule) — outdated.

### 4.3 Exemptions (circular 14/2025 recap + CT)

- Payment titles ≤ 300 DA (art. 100-I).
- **Payments by electronic means "sous toutes leurs formes"** — bank cards (credit/debit, online & in-store), electronic bank transfers (art. 258 quinquies, created by LF 2025 art. 47).
- Payments by **cheque** (bank, postal) or **bank/postal transfer**, *provided the receipt mentions*: date and number of the cheque and drawee name (or postal account number, debit date and CCP office), or date of the transfer order, its execution date and the banks involved (art. 258).
- Sums settled by cash deposit into a bank or similar (art. 258 quater).
- Receipts given/received by public accountants (art. 102), zoo/museum entries.
- Separate rule art. 100-II: **uniform 50 DA** on simple receipts/discharges of titles, values or objects, and on receipts of cash deposits made with a company or natural person (not the sale receipt itself).
- Art. 2 CT general minimum (33 DA in the 2026 edition; Grant Thornton's LF 2025 summary quotes 30 DA) applies "sous réserve des dispositions de l'article 100", i.e. **not** to the timbre de quittance.

### 4.4 Algorithm for the POS

```
// amount = sum actually paid in cash for this receipt (DZD, TTC), in centimes
function timbreQuittance(amountCentimes):
    A = amountCentimes / 100.0                  // dinars
    if A <= 300: return 0
    tranches = ceil(A / 100)                    // fraction of 100 DA counts as a full tranche
    rate = (A <= 30000) ? 1.0 : (A <= 100000) ? 1.5 : 2.0     // bracket of the WHOLE amount
    duty = tranches * rate
    return max(duty, 5.0)                        // minimum 5 DA
```

| Cash amount (DA) | Tranches | Rate | Duty (DA) | Note |
|---|---|---|---|---|
| 300.00 | – | – | 0 | exempt |
| 301.00 | 4 | 1 | **5** | minimum applies (4 → 5) |
| 1,001.00 | 11 | 1 | 11 | fraction counted |
| 25,000.00 | 250 | 1 | 250 | DGI example 1 |
| 30,000.00 | 300 | 1 | 300 | |
| 30,001.00 | 301 | 1.5 | 451.50 | **cliff** (+151.50 DA for +1 DA) |
| 35,000.00 | 350 | 1.5 | 525 | DGI example 2 |
| 100,000.00 | 1,000 | 1.5 | 1,500 | |
| 100,001.00 | 1,001 | 2 | 2,002 | **cliff** |
| 350,000.00 | 3,500 | 2 | 7,000 | DGI example 3 |

Implementation notes (interpretations flagged):
- **Base = TTC amount of the receipt paid in cash.** The circular reasons on the "montant" of the ticket/facture. Whether the timbre is added on top ("Net à payer = TTC + timbre") is universal practice, but the legal text does not say who bears it — **UNVERIFIED which party legally bears it**; practice: customer.
- **Split tender** (part card, part cash): exemption covers sums paid electronically → compute on the cash part only. **Interpretation, UNVERIFIED** — make it a setting.
- Decimals: 1.5 DA rate yields .50 DA; keep 2 decimals (no rounding rule found).
- Because of the cliffs, the UI should show the cashier the timbre live and suggest card/QR payment (the law's stated purpose is to push electronic payments).
- Credit sales paid later in cash: the duty arises when the payment receipt (quittance) is issued → apply at payment time, not at invoice time.

### 4.5 Declaring the collected timbre

- CT art. 4: taxpayers under the **réel regime must pay stamp duties "sur état"** (or by approved printing machines); others may opt.
- CT art. 19: they pay the duty collected at the recette des impôts **within the first 20 days of the month following** the month (or quarter); late filing 10 % (25 % after formal notice); late payment 10 % + 3 %/month capped 25 %.
- G50 section "4 — Droit de timbre" is reserved for those paying "sur état" (G50 notice).
- → POS report: **monthly timbre collected** (by day, by register, by document), reconciled with cash tender totals.

---

## 5. Tax regimes and what the software must produce for each

### 5.1 IFU — Impôt forfaitaire unique (official DGI page, updated 20/09/2026)
Source: https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-forfaitaire-unique/ifu (and CIDTA art. 282 bis-282 duodecies, 365).

- Replaces TLS + TVA + IRG for small taxpayers (art. 282 bis).
- **Scope: natural persons, SCP, artisan cooperatives with annual CA ≤ 8,000,000 DA**; auto-entrepreneurs ≤ **5,000,000 DA**; sellers of regulated-price/margin mass-consumption products (art. 282 quater — base = **margin** for products like milk, sugar, oil); multi-shop owners if the **sum** of shops' CA ≤ 8 M DA (282 quinquies; LF 2026 art. 24 aligned the switch to réel "à la clôture de l'exercice suivant celui du dépassement" — DGI circulaire n° 29/2026).
  - ⚠ Many 2025-2026 blogs say "seuil IFU 15 M DA" (e.g. https://almawarid.app/blog/loi-de-finances-2026-algerie-resume-mesures-entreprises/ as summarised in search results) — **the DGI official page and circular 29/2026 say 8,000,000 DA**.
- **Excluded activities** (→ réel): real-estate promotion; **import for resale**; **wholesale ("achat-revente en l'état dans les conditions de gros")**; concessionaires; clinics/labs; classified hotels/restaurants; precious metals; public works; alcohol outlets; tobacco leaf; caterers; party-hall rental; **retail in large surfaces ("grandes surfaces")**; vehicle/equipment rental; travel agencies; advertising agencies; training; insurance agents (LF 2025 art. 22).
- **Rates** (282 sexies): **5 %** production & sale of goods; **12 %** other activities (incl. BNC, SCP); **0.5 %** auto-entrepreneurs; 5 % collection of recyclable waste; mixed activity → prorata.
- **5 % withholding (libératoire)** on TTC invoices for natural persons selling via **digital platforms or network direct-selling**, withheld by the producing/wholesaling supplier and remitted by the 20th of the following month (CIDTA 282 sexies A). → Relevant if DZBuild merchants supply online resellers.
- **Minimum IFU: 30,000 DA** (LF 2025 art. 29; also for new/partial-year taxpayers); **10,000 DA for auto-entrepreneurs**.
- **Declarations**: G8 déclaration d'existence within 30 days (fine 30,000 DA); **G12 prévisionnelle by 30 June** (pay 100 % or 50 % + 25 % 1-15 Sept + 25 % 1-15 Dec); **G12 bis définitive by 20 January N+1**, with complement if actual CA > forecast, and must state **revenu net** (LF 2025 art. 23); new taxpayers file only G12 bis. Employees: **G50 ter quarterly** (IRG salaires, by the 20th after each quarter), G29 annual (online mandatory where Jibayatic; NIN required, LF 2026 art. 8), TFPC/TA declaration now **semestrial** (LF 2026 art. 18-21).
- **LF 2026**: the "contractual" IFU regime planned by LF 2025 (art. 79-87, due 1/1/2026) was **abrogated** — the **declarative system is maintained** (LF 2026 art. 106/110; DGI circulaire n° 39/2026 https://www.mfdgi.gov.dz/files/804/2026/3737/Circulaire-n39-du-07-05-2026-relative-a-labrogation-du-regime-contractuel-de-l-Impot-Forfaitaire-Unique-IFU.pdf). IFU e-filing/e-payment remains **optional** (LF 2026 art. 111).
- **Books**: purchase-resale → **registre des achats** (côté & paraphé, yearly summary, backed by invoices) + **registre des ventes** (côté & paraphé) ; services → **livre journal** of receipts day by day (CPF art. 1). Missing registers: 10,000 DA fine (282 duodecies). Retention 10 years.
- Penalties: late G12/G12 bis 10/20/25 %; late payment 10 % + 3 %/month capped 25 %.

**Software outputs for IFU users**: daily/monthly/annual **CA TTC** (and margin for regulated products); printable **registre des ventes** and **registre des achats** (per-day lines, cumulative), G12/G12 bis helper (forecast vs actual, 8 M DA / 5 M DA threshold alerts at e.g. 70/90/100 %), IFU amount estimate (5 %/12 %/0.5 %, min 30,000/10,000), payment schedule reminders (30/06, 15/09, 15/12, 20/01).

### 5.2 Régime du réel
Sources: https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-reel/regime-reel ; TVA page; CIDTA.
- Applies to **all legal persons** whatever their CA, natural persons > 8 M DA or by option, and the excluded activities above (CIDTA art. 148 per DGI). **LF 2026 art. 70**: the option for réel made by a new taxpayer is **irrevocable** (circulaire n° 06/2026).
- Accounting per **SCF** (système comptable financier); present all accounting documents on request; 10-year retention.
- **G50 monthly** (TVA, IRG salaires, timbre sur état, IBS/IRG acomptes); G9 état fournisseurs annex; annual G4 (IBS) / G11 (IRG-BIC) + liasse G2; **état clients** for wholesale (183 ter); **ERA** (état récapitulatif annuel, CIDTA 151 bis) e-filed **before 30 September** (ERA 2024 & 2025 both due before 30/09/2026 — DGI communiqué https://www.mfdgi.gov.dz/files/781/2026-2025/3811/Com-ERA-2024-2025-fr.pdf).
- CTCA art. 66: a natural person liable to TVA who does not keep full accounting must keep a **book côté & paraphé** listing **day by day, without blanks or erasures**, each operation (date, summary designation, price), taxable vs non-taxable, closed monthly.

### 5.3 État clients (wholesale) — CIDTA art. 183 ter (created LF 2024 art. 12)
- Any person selling "dans les conditions de gros" files, with the annual declaration, a state (paper/digital/tele-declaration) listing per client: **name/raison sociale, NIF, RC, article d'imposition, precise address, annual HT sales, TVA invoiced**.
- "Vente en gros" = goods not usually used by individuals; deliveries at identical prices whether wholesale or retail; **any delivery of products intended for resale, whatever the quantity**.
- **Before each wholesale sale**, the seller must **authenticate the client's RC via the CNRC website and NIF via the DGI NIF site** (penalties 194-6).
- → Product: customer type "revendeur" requires NIF/RC/AI/address; a "vérifié le …" field + deep links to the CNRC and nifenligne portals; annual état-clients export.

### 5.4 Auto-entrepreneur — loi 22-23 (JO 2022 n° 85)
- Natural person, eligible activities list, CA below legal threshold (5 M DA for IFU per DGI page); registered in the **registre national de l'auto-entrepreneur** (public establishment — ANAE, name **UNVERIFIED** from the law text which says "l'établissement"), **carte d'auto-entrepreneur with a unique national registration number**; exempt from RC; simplified accounting on a **register côté & paraphé** (receipts and expenses); must obtain NIF within 30 days; commercial bank account allowed; annual CA certificate; forced to RC after 3 consecutive years above threshold.
- **Micro-importateur** under AE status (décret exécutif 25-170 du 28/06/2025 per DGI IFU page): IFU paid **at customs for each import**, base = customs value + customs duties + **30 % flat margin** (LF 2026 art. 143).
- → Template must support "N° carte AE" in place of RC, IFU 0.5 %, no TVA.

### 5.5 Régime simplifié
- For non-commercial professions (BNC) (DGI TVA page mentions "régime simplifié (professions libérales)"). Out of core POS scope but TVA rules apply.

### 5.6 Reports matrix (what to build)

| Report / export | IFU | Réel | AE | Legal hook |
|---|---|---|---|---|
| Z-report (daily closing) with totals by tender, tax rate, timbre | ✔ | ✔ | ✔ | LF 2026 cash-system rules (inalterability), CT art. 19 |
| Registre des ventes / livre journal (printable, paged) | ✔ | (art. 66 CTCA for natural persons) | ✔ | CPF art. 1; CTCA 66; loi 22-23 art. 9 |
| Registre des achats | ✔ | — | ✔ | CPF art. 1 |
| G50 helper (CA HT per rate, exempt CA, TVA collected, TVA deductible, timbre collected, IRG salaires input) | — | ✔ | — | CTCA 76; G50 notice |
| G9 état fournisseurs (NIN, NIF, name, address, RC, date, ref, TTC, TVA) Excel | — | ✔ | — | CTCA 29; form G9 2025 |
| État spécial TVA art. 18 (cancellations/unpaid) | — | ✔ | — | CTCA 18 |
| État clients annual (name, NIF, RC, AI, address, HT, TVA) | — | ✔ (wholesale) | — | CIDTA 183 ter |
| G12 / G12 bis helper, threshold alerts | ✔ | — | ✔ | CIDTA 282 quater, CPF 1 |
| Timbre monthly statement | ✔ (if cash) | ✔ (sur état) | ✔ | CT 100, 4, 19 |
| Fiscal archive export (all documents + journal + hash chain + schema doc) | ✔ | ✔ | ✔ | CPF 20 ter, 64 (LF 2026) |

---

## 6. Certified cash registers, software certification and e-invoicing (status 2026)

### 6.1 NEW: LF 2026 obligations on cash systems and software (effective **1 January 2027**)

Source: LF 2026 full text (JO n° 88 du 31/12/2025) https://www.mfdgi.gov.dz/files/537/20212026/3710/LF2026.pdf ; DGI communiqué LF 2026 §II-3.

- **Art. 58 → CTCA art. 51 bis**: "Toute personne assujettie à la TVA doit, si elle effectue des livraisons de biens et des prestations de services **ne donnant pas lieu à facturation** […] et enregistre ces opérations, **au moyen d'un logiciel ou d'un système de caisse**, présenter **un engagement de l'éditeur du logiciel ou d'un système de caisse, ou un certificat délivré par un organisme habilité d'accréditation** de logiciels ou de systèmes de caisse, qui atteste que le logiciel ou le système satisfait aux conditions d'**inaltérabilité, de sécurisation, de conservation et d'archivage des données**, en vue de tout contrôle fiscal."
- **Art. 74 → CPF art. 20 ter**:
  1. Taxpayers under audit keeping computerised accounting must present a **publisher's engagement** of conformity of their accounting software (inalterability, security, conservation, archiving).
  2. Same for cash-system users (per CTCA 51 bis); **the implementing rules and the models of the engagement and certificate are fixed by arrêté of the Minister of Finance** → **not found as of 2026-09-28 (UNVERIFIED / likely pending)**.
  3. Audit covers **all data and processing** contributing to results/declarations **plus documentation of analysis, programming and execution**; taxpayer must hand over at first visit a **dematerialised copy of accounting entries in a format fixed by arrêté**; surprise audits may seal two copies of files; auditor may reprocess files if copies are broken/unavailable.
- **Art. 81 → CPF art. 51 sexies**: publishers of accounting, management **or cash-system software**, and anyone intervening technically on them, must provide the tax administration, **on request, all codes, data, processing or documentation** related to them.
- **Art. 82 → CPF art. 64**: IT-held books/documents must be kept **in that electronic form**; data/processing/documentation retained **until the end of the 6th year** after the year concerned.
- **Art. 109**: articles 58, 74, 81 take effect **1 January 2027**. Extra 30 days of on-site audit for computerised accounting.

**What this means for DZBuild POS (design requirements, by analogy with comparable regimes; the Algerian arrêté will be authoritative):**
- Append-only fiscal journal: every ticket/invoice/credit note/void/Z-close/price change/drawer open stored with sequence number, timestamp, user, and a **hash chain** (each record includes hash of previous) + periodic signed checkpoints; no UPDATE/DELETE of fiscal records (corrections only via credit notes/voids).
- **Z-close** per day and per register with cumulative grand totals (perpetual totals) that cannot be reset.
- Secured access (roles, PINs), clock-change detection, audit trail of configuration changes (tax rates, prices).
- **Archive** per period (fiscal year) exported in open formats (CSV/JSON + PDF), with integrity manifest; retention ≥ 10 years for documents, ≥ 6 years for data & technical documentation.
- A ready "**engagement de l'éditeur**" template signed by DZBuild (as publisher) and a technical documentation pack (data dictionary, algorithms, export formats) — since the administration can request "tous codes, données, traitements ou documentation".
- Export in the format that the upcoming arrêté will define for accounting entries (design the exporter as a pluggable module).

### 6.2 E-invoicing / "facturation électronique"
- **No Algerian clearance/CTC e-invoicing obligation** (DGI real-time validation, QR per invoice, platform submission) was found in LF 2025, LF 2026, the 2026 codes, or DGI 2026 circulars. **Status: not mandatory (as far as verifiable on 2026-09-28).**
- Legal permissions that exist: décret 05-468 art. 10 (invoice "établie sous la forme dématérialisée à travers le recours à un procédé informatique" is regular) and art. 11 (télématique transmission subject to a joint arrêté — **arrêté UNVERIFIED**); décret 16-66 (BTC may be electronic and transmitted by telematics); loi 18-05 art. 20 (e-commerce invoice; the e-consumer may demand paper).
- Claims such as "l'Algérie se dirige progressivement vers la facturation électronique obligatoire avec homologation DGI des logiciels" (Lamacta blog) are **UNVERIFIED**; the only verified software obligation is the 2027 publisher-engagement/certificate regime above.
- The wet-stamp + signature requirement (05-468 art. 4) remains for non-telematic invoices → printed invoices should include a stamp/signature box; PDFs sent by email are "dématérialisées" but the telematic exemption requires the (unverified) arrêté — advise merchants to stamp/sign printed copies for B2B.

---

## 7. Personal data — loi 18-07 as amended by loi 25-11 (24 July 2025), ANPDP

Sources: JO 2018 n° 34 https://www.joradp.dz/FTP/jo-francais/2018/F2018034.pdf ; consolidated Arabic text with 2025 amendments published by the ANPDP https://anpdp.dz/ar/storage/2025/08/18-07-Edited.pdf ; ANPDP portal https://anpdp.dz/ar/ (declarations & authorisation requests, complaints https://plaintes.anpdp.dz).

| Rule | Article | POS implication |
|---|---|---|
| Processing requires **express consent**, except when necessary for a **legal obligation**, a **contract** with the person, vital interests, public mission, or **legitimate interest** | art. 7 | Invoice data for B2B/tax = legal obligation/contract; **marketing (SMS, loyalty) needs explicit consent** checkbox, revocable |
| Children's data needs legal representative consent | art. 8 | Avoid |
| **Prior declaration** to the ANPDP for every processing (or authorisation if risky); receipt within 48 h; can be done online; single declaration for related purposes; content: controller, purposes, categories of persons/data, recipients, **transfers abroad**, **retention period**, rights desk, security measures, interconnections/subcontracting | art. 12-14, 17 | Merchant (controller) must declare the customer file; DZBuild should provide a pre-filled declaration guide |
| Sensitive data forbidden (with exceptions) | art. 18 | Do not store health/religion etc. (pharmacy vertical needs care) |
| Information of the person (identity of controller, purposes, recipients, rights, transfer abroad) | art. 32 | Printable privacy notice / ticket footer |
| Access, rectification, opposition rights | art. 34-36; refusal punished (art. 64: 2 months-2 years, 20,000-200,000 DA) | Customer data export/rectify/anonymise tools |
| **Security measures** appropriate to risk | art. 38; 45 bis 7 | Encryption at rest for customer PII, role-based access |
| **Subcontractor** must give guarantees, written contract, act only on instructions | art. 39 | **DZBuild cloud sync = sous-traitant** → DPA/contract in ToS |
| Professional secrecy | art. 40 | |
| **Transfer abroad only with ANPDP authorisation** and adequate protection (derogations: consent, contract performance…) | art. 44-45 | **Keep data local/offline by default; if DZBuild cloud is hosted outside Algeria, an ANPDP authorisation or explicit consent is needed** — strong argument for Algerian hosting of the sync backend |
| **DPO** (mandatory designation by controllers) | art. 41 bis (loi 25-11) | Settings field for merchant DPO contact |
| **Register of processing activities** (controller and subcontractor) | art. 41 bis 2 (loi 25-11) | Provide template |
| **Automated operations log** ("دفتر آلي للعمليات"): collection, recording, modification, consultation, extraction, communication, erasure… with **date, time, justification, identity of user** and recipients | art. 41 bis 3 (loi 25-11) | **Audit log of every read/write/export of customer personal data** — build it into the data layer |
| **DPIA** before high-risk processing | art. 45 bis 6 (loi 25-11) | |
| **Breach notification to ANPDP within 5 days**; notify persons if high risk; document all breaches | art. 45 bis 8-10 (loi 25-11) | Incident procedure for DZBuild cloud |
| Processing without declaration | art. 56: **2-5 years prison + 200,000-500,000 DA** | |
| E-commerce: collect only data necessary, prior consent, security | Loi 18-05 art. 26 | Online orders synced from dzbuild.dev |

---

## 8. Currency, amounts in figures and letters

### 8.1 DZD basics
- ISO code **DZD**, 1 dinar = **100 centimes**; banknotes in use 200, 500, 1,000, 2,000 DA (100 DA coin/note); polymer 2,000 DA issued end 2022; small fractional coins no longer used in practice (https://fr.wikipedia.org/wiki/Dinar_alg%C3%A9rien — secondary).
- Colloquial speech often counts in **centimes** (old "franc": 1 DA = 100) — e.g. "mille" = 10 DA (Wikipedia, secondary). → Optional "prix en centimes" display on the customer display/labels can help older customers (practice).
- Formatting (recommended, per https://github.com/AbdeldjalilChougui/dz_kit — secondary): French `1 234,56 DA` (narrow no-break space U+202F as thousands separator, comma decimal); Arabic `1 234,56 دج` with U+00A0 before the symbol so RTL does not reorder digit groups. Store amounts as **integer centimes**.

### 8.2 "Montant en lettres" (legally required)
- Décret 05-468 art. 3: "prix total toutes taxes comprises, libellé en chiffres **et en lettres**". Language not specified → French is standard practice; Arabic recommended given décret 13-378 art. 7 (consumer info essentially in Arabic).
- French rules (grammar, not law): "Arrêtée la présente facture à la somme de : **deux cent quatre-vingt-dix-neuf mille neuf cent quatre-vingt-dix-neuf dinars algériens et cinquante centimes**". Key pitfalls: *cent/vingt* plural only when multiplied and not followed by another number (quatre-vingts, deux cents / deux cent un); *mille* invariable; *million(s)/milliard(s)* are nouns ("un million de dinars"); 1990 spelling reform allows hyphens everywhere (deux-cent-un) — choose one convention; "un dinar", "zéro centime".
- Arabic rules (تفقيط): number–noun agreement: 1 → دينار واحد ; 2 → ديناران ; 3-10 → دنانير (masc. noun ⇒ number in feminine form: ثلاثة دنانير) ; 11-99 → ديناراً (accusative singular) ; 100, 1000… → دينار (genitive singular); centimes "سنتيم" (plural سنتيمات / accusative سنتيماً). Typical phrase: «أوقفت هذه الفاتورة على مبلغ: ... دينار جزائري و ... سنتيم».
- **Library gaps (verified)**: Python `num2words` has `lang_FR_DZ` but its currency form is spelled **'dinard'/'dinards'** and uses code `'DIN'` not `'DZD'` (https://github.com/savoirfairelinux/num2words/blob/master/num2words/lang_FR_DZ.py); its Arabic module has SAR/EGP/KWD/TND forms but **no DZD** (lang_AR.py). → Build and unit-test an in-house FR/AR converter (reuse TND Arabic forms structure).

---

## 9. Language, labelling, price display

- Consumer information "doit être rédigée **essentiellement en langue arabe** et à titre accessoire dans une ou plusieurs autres langues" (décret 13-378 art. 7). → Bilingual AR/FR tickets and shelf labels; RTL-capable printing (thermal printers need Arabic shaping).
- Food labelling (décret 13-378): **lot** identification (art. 29), **DLC / DDM** (art. 31-33); non-food: lot/series, date limite d'utilisation (art. 45). → Inventory should track **lot + expiry** and block/alert sale of expired items (practice + consumer-protection logic).
- Price display (loi 04-02 art. 4-6): prices must be visible and legible by marking, labelling or display; **displayed price = total amount the customer pays** (i.e. TTC); goods sold by unit/weight/measure are counted/weighed in front of the buyer; B2B price lists on request (art. 7). Fine 5,000-100,000 DA (art. 31).
- Regulated prices/margins must be respected and **price structures deposited** (loi 04-02 art. 22, 22 bis; fines up to 10 M DA art. 36). IFU base for such products = margin (CIDTA 282 quater). → Product flag "prix réglementé" with max price/max margin checks.
- Weighing instruments are subject to metrological verification fees ("peseuses et doseuses pondérales") — LF 2026 table of fees (JO n° 88, p. 89). → Scale integration must use verified scales (practice).

---

## 10. Business calendar

- **Weekend: Friday–Saturday** (since 2009; legal reference commonly cited as décret présidentiel 09-244 — **reference UNVERIFIED**; weekend fact confirmed by https://fr.wikipedia.org/wiki/F%C3%AAtes_et_jours_f%C3%A9ri%C3%A9s_en_Alg%C3%A9rie).
- **Public holidays** (loi 63-278 of 26/07/1963 as amended; Yennayer added from 12/01/2018; Eids extended to **3 days** in 2023 — per Wikipedia, secondary):

| Holiday | Date | Days |
|---|---|---|
| Nouvel An | 1 January | 1 |
| Yennayer (Amazigh New Year) | 12 January | 1 |
| Fête du Travail | 1 May | 1 |
| Fête de l'Indépendance | 5 July | 1 |
| Déclenchement de la Révolution | 1 November | 1 |
| Awal Muharram (Hijri New Year) | 1 Muharram | 1 |
| Achoura | 10 Muharram | 1 |
| Mawlid Ennabaoui | 12 Rabi' al-Awwal | 1 |
| Aïd el-Fitr | 1 Shawwal | 3 |
| Aïd el-Adha | 10 Dhu al-Hijja | 3 |

- Islamic dates depend on moon sighting announced by the authorities (practice) → the app must let the user **edit/confirm holiday dates** each year; ship a Hijri calendar (Umm al-Qura approximation) + manual override.
- Tax deadline falling on a holiday moves to the next working day (stated for IFU payments on the DGI IFU page; for ERA in CIDTA 151 bis).
- **Ramadan**: shops commonly change hours (late-night opening); official administration hours are set yearly — **UNVERIFIED (no source fetched)**. Product: per-day opening hours profiles and "Ramadan mode" shift templates; Z-close after midnight must attribute to the correct business day (configurable day-cut time).
- JO documents show dual Hijri/Gregorian dates; showing the Hijri date on tickets is optional (no requirement found).

### Fiscal calendar to embed (reminders)
| Deadline | What | Who |
|---|---|---|
| 20th of each month | G50 (TVA, IRG salaires, timbre sur état…) + G9 annex | Réel |
| 20 Apr / 20 Jul / 20 Oct / 20 Jan | G50 ter IRG salaires | IFU with employees |
| 20 January | G12 bis définitive (IFU); new IFU taxpayers | IFU |
| 30 April | G29 salaries (online where Jibayatic) | Employers |
| 30 June | G12 prévisionnelle + payment (or 50 %) | IFU |
| 1-15 Sept, 1-15 Dec | IFU instalments (25 % each) | IFU (fractionné) |
| 20th of month after semester | TFPC / TA declaration (LF 2026) | Employers |
| before 30 September | ERA e-filing | Réel |
| 31 Dec 2026 | End of several TVA exemptions; voluntary regularisation (8 %) deadline; tax-debt settlement scheme | All |
| **1 Jan 2027** | Cash-system/software engagement or certificate required | TVA-liable users of POS |

---

## 11. Payment methods

### 11.1 Legal obligations
- **Merchants selling to consumers must provide electronic payment instruments** allowing payment "à travers son compte bancaire ou postal" — LF 2018 art. 111 as last amended by **LF 2024 art. 113** (deadline **31/12/2024**); failure = **50,000 DA** fine, controlled by police, commerce inspectors and tax agents (text consolidated in CPF/CIDTA 2026 editions). Original 2018 version required TPE specifically (JO 2017 n° 76).
- **Cash ceilings** (décret 15-153): payments ≥ 5,000,000 DA for real estate and ≥ 1,000,000 DA for new vehicles/motorcycles from dealers, yachts, **goods from precious-metal/stone merchants**, antiques/art, auction sales must go through banking channels. LF 2025 art. 207 also imposes non-cash payment for real estate, vehicle/equipment dealer sales, yachts, mandatory insurance.
- **TVA not deductible** on invoices > 1 M DA TTC paid cash (CTCA 30).
- **Timbre** only on cash (and exempt for electronic payments since 2025) → strong incentive to record the exact tender.
- E-commerce (loi 18-05 art. 27-29): payment remote **or at delivery (COD allowed)**; electronic payments must go through platforms operated **exclusively by banks approved by the Banque d'Algérie and Algérie Poste**, connected via the public telecom operator network; site-to-platform link secured by electronic certification; platforms controlled by Banque d'Algérie.

### 11.2 Ecosystem (verified facts)
| Method | Facts | Source |
|---|---|---|
| **CIB card** (interbank, SATIM network) | SATIM runs CIB network, e-payment and webmerchant integration; REST e-payment endpoint exists (`https://test.satim.dz/payment/rest/register.do` returns `{"errorCode":4,"errorMessage":"Empty merchant user name"}`) | https://www.satim.dz/index.php/fr/ ; endpoint probe 2026-09-28 |
| **Edahabia** (Algérie Poste card) | **Interoperability CIB/EDAHABIA in production since 05/01/2020** | GIE Monétique https://giemonetique.dz/qui-sommes-nous/chiffres/ |
| Web merchant onboarding | **CIBWEB** portal (GIE Web Marchand): integration request, technical tests, certified payment modules | https://www.cibweb.dz/fr/ |
| **Chargily Pay V2** (licensed aggregator) | Accepts EDAHABIA and CIB; API `https://pay.chargily.net/api/v2` (test: `/test/api/v2`); Bearer secret key; objects: customers, products, prices, **checkouts**, **payment links**; `payment_method`: "edahabia", "cib", "chargily_app"; amounts in DZD; webhooks with `signature` header = HMAC of payload with API secret; events e.g. `checkout.paid`; 0 % fees first month then plans incl. a free one; fees can be passed to customer | https://dev.chargily.com/llms.txt and pages linked there |
| **Algérie Poste "CCP Business (Cashless)"** | Online opening of a professional CCP account for merchants/AE/e-merchants; collect by **CIB, Edahabia, BaridiMob, QR code, contactless, instant transfers**; "directement intégré à vos applications et outils métiers"; ePay online payment and TPE | https://cashless.poste.dz/ (text from the site's application bundle) |
| **BaridiMob** | Algérie Poste mobile app (payments/transfers); QR acceptance advertised in CCP Business | same |
| **RIP / CCP** | RIP = `007` + `99999` + 10-digit account + 2-digit key (`97 − ((N×100) mod 97)`); CCP key = weighted sum mod 100 (secondary, with published test vectors) | https://github.com/AbdeldjalilChougui/dz_kit |
| Cheque | Exempt from timbre if receipt states cheque date/number/drawee (CT art. 258) | Code du timbre |
| Visa/Mastercard | GIE Monétique working with VISA/Mastercard on international card acceptance (news items) | https://giemonetique.dz/actualites-medias/actualites/ |

- **Interbank QR-code standard (Banque d'Algérie) in 2026: UNVERIFIED** — only Algérie Poste's CCP Business QR is verified.
- **TPE ↔ POS integration (ECR protocol)**: no public Algerian standard found — **UNVERIFIED**; assume stand-alone TPE: cashier keys amount on the TPE and records tender "Carte CIB/Edahabia" + authorisation number/last 4 digits on the POS.
- Other gateways often mentioned (Slick Pay, etc.): **UNVERIFIED** in this research.

**POS tender model**: Espèces, Chèque (n°, banque, date), Carte CIB, Carte Edahabia, QR/BaridiMob, Virement bancaire/CCP (date ordre, date exécution, banques), Chargily/lien de paiement, Crédit client (à terme), Avoir/bon d'achat, Contre-remboursement (COD via transporteur), mixed tender; timbre computed on cash part.

---

## 12. Delivery & e-commerce (COD) in Algeria

### 12.1 E-commerce law 18-05 (JO 2018 n° 28) — obligations relevant when syncing with dzbuild.dev
- Art. 8: e-commerce activity requires registration in the **RC** (or artisan register) and publication of a website/page **hosted in Algeria with a ".com.dz" extension**; art. 9: national file of e-suppliers at CNRC; domain name must be deposited with CNRC. (Relevant to DZBuild merchant storefronts.)
- Art. 11: offer must show **NIF**, physical & email address, phone, **RC or artisan card number**, TTC prices, availability, delivery terms/costs/delays, T&Cs incl. personal-data policy, warranty & after-sales, payment modes, return/exchange/refund procedure, etc.
- Art. 12: 3 mandatory order steps (terms shown, order review/edit, explicit confirmation).
- **Art. 20: every e-sale gives rise to an invoice** (per the invoicing law); consumer may demand paper.
- Art. 22-23: late delivery or non-conforming/defective product → return within **4 working days**, refund within **15 days**.
- Art. 24: **must not validate orders for out-of-stock products** → real-time stock sync POS ↔ web.
- Art. 25: keep registers of transactions and **transmit them electronically to CNRC** (modalities by regulation — **UNVERIFIED if implemented**).
- Art. 27: payment remote or **at delivery** (COD legal).

### 12.2 Carriers (APIs) — verification status

| Carrier | API facts | Status |
|---|---|---|
| **Yalidine** (and Guepex/Yalitec same platform) | Base `https://api.yalidine.app/v1/`; auth headers API ID + API token (endpoint answers "API ID and/or Token must be specified"); webhooks signed with `X-Yalidine-Signature` HMAC (per SDK) | Endpoint verified 2026-09-28; docs page 403 to bots; webhook detail secondary (https://github.com/tkawen/shipping-dz) |
| **ZR Express** | Uses **Procolis** API `https://procolis.com/api_v1/` with **token + key** (endpoint answers "Clé non détectée") | Endpoint verified; field details secondary |
| **Ecotrack** family (DHD, GoLivri, World Express, Packers, Anderson, …) | Per-carrier subdomain `https://{carrier}.ecotrack.dz/api/v1/…` with Bearer token (e.g. `dhd.ecotrack.dz/api/v1/get/wilayas` → 401 Unauthorized) | Endpoint verified; SDK secondary |
| **Maystro Delivery** | Website up (https://maystro-delivery.com/); API details | **UNVERIFIED** |
| **Noest Express** | Site returns 403 to scripts | **UNVERIFIED** |

Secondary integration reference (MIT, PHP): https://github.com/tkawen/shipping-dz — notes that many integrations break on "wilaya lists frozen at 58" and wrong credentials (Yalidine id+token, ZR token+key, Ecotrack token).

**COD workflow the POS needs**: order → shipment creation (carrier API) → label/tracking → status webhooks (shipped/delivered/returned) → **COD remittance reconciliation** (carrier pays out net of fees) → returns re-stock; per-wilaya delivery fees (home vs stop-desk); invoice/ticket generated at delivery or order confirmation (loi 18-05 art. 20).

### 12.3 Wilayas & communes dataset
- **69 wilayas** since **loi n° 26-06 du 4 avril 2026** (JO n° 25 du 5 avril 2026 per datasets); announced by the Council of Ministers on 16/11/2025; **1,541 communes unchanged**; 92 communes moved to the 11 new wilayas (https://en.wikipedia.org/wiki/Provinces_of_Algeria ; https://github.com/wadiemendja/algeria-communes-wilayas).

| Code | New wilaya | Arabic | Split from |
|---|---|---|---|
| 59 | Aflou | أفلو | Laghouat |
| 60 | Barika | بريكة | Batna |
| 61 | El Kantara | القنطرة | Biskra |
| 62 | Bir El Ater | بئر العاتر | Tébessa |
| 63 | El Aricha | العريشة | Tlemcen |
| 64 | Ksar Chellala | قصر الشلالة | Tiaret |
| 65 | Aïn Oussera | عين وسارة | Djelfa |
| 66 | Messaad | مسعد | Djelfa |
| 67 | Ksar El Boukhari | قصر البخاري | Médéa |
| 68 | Bou Saâda | بوسعادة | M'Sila |
| 69 | El Abiodh Sidi Cheikh | الأبيض سيدي الشيخ | El Bayadh |

(Parent mapping from wadiemendja dataset; Wikipedia EN lists the affected parents without Tébessa — minor discrepancy, **verify Bir El Ater's parent against the JO** before shipping.)

- Open datasets (MIT): https://github.com/islam-re/Algeria-wilayas (69 wilayas, dairas, communes; JSON/CSV/SQL/XLSX; AR+Latin); https://github.com/wadiemendja/algeria-communes-wilayas (**both 58- and 69-wilaya files**, same commune IDs — ideal for carrier mapping); older 58-wilaya reference https://github.com/othmanus/algeria-cities (58 wilayas, 546 dairas, 1,541 communes, 3,940 postcodes).
- Recommendation: ship the 69 structure as default, keep a `wilaya_code_2019` column for carriers still on 58 codes, and allow updates without a new installer.

---

## 13. Importers & wholesalers (grossistes)

- Import-for-resale and wholesale are **excluded from IFU** → réel + TVA + état clients (§5.3).
- **Précompte IBS 2 %** on imports of goods for resale "en l'état", on total value incl. customs duties and import taxes except DAPS (LF 2022 art. 113); **border barter (troc frontalier) excluded** from 2026 (LF 2026 art. 104; circulaire n° 41/2026 https://www.mfdgi.gov.dz/files/804/2026/3776/Circulaire-n41-du-13-05-2026-relative-au-precompte-a-l-importation-de-2--Exclusion-des-operations-d-importation-de-marchandisesIBS.pdf).
- **Taxe de domiciliation bancaire**: 0.5 % of import amount for goods for resale (1 % CKD/SKD), min 20,000 DA; 4 % on imported services (text of LFC 2005 art. 2 as last modified by LF 2025 art. 123, reproduced in DGI LF 2025 circulars compilation).
- **DAPS** (droit additionnel provisoire de sauvegarde) 30 %-200 % on listed goods (LFC 2018 art. 2, reproduced in CTCA 2026).
- **TVA on imports**: base = customs value + all duties and taxes (CTCA art. 19; "introduction en douane" is the taxable event per DGI TVA page).
- Micro-importateur AE: IFU at customs per import (base customs value + duties + 30 %).
- **Bon de transfert** for moving goods between own depots/shops (05-468 art. 12-13); **BL + facture récapitulative** only with authorisation.
- Customs forms (e.g. D10/DAU), certificate of conformity, import licences: **not researched in detail — UNVERIFIED**.
- → Product: **landed-cost** calculator (FOB/CIF → customs value, DD, DAPS, TVA import, domiciliation tax, précompte, transit/transport) feeding weighted average cost (PMP); multi-depot transfers printing bons de transfert; wholesale price levels; client verification (RC/NIF).

---

## 14. Other sector-specific items worth supporting

- **Precious metals / jewellers**: registers côté & paraphé (purchases/sales with suppliers' names; **client register**), authorisation from tax administration, AML compliance (LF 2026 art. 62-68; DGI communiqué). Cash ≥ 1 M DA forbidden (décret 15-153).
- **Pharmaceutical establishments** (manufacturers/importers/wholesalers): must transmit stock states via the ministry's digital platform (LF 2023 art. 61 as modified; reproduced in CIDTA annex).
- **Tobacco**: TIC and additional tax (75 DA per pack from LF 2026 art. 102), tobacco additional tax is inside the TVA base.
- **Tyres & lubricants**: specific taxes shown distinctly, outside the TVA base (§2.2).

---

## 15. Derived requirement checklist for the DZBuild POS

**MUST (legal)**
1. Company profile: raison sociale / nom-prénom, forme juridique, nature d'activité, capital social, adresse, tél/fax/email, RC (or N° carte AE), NIF, NIS, AI, NIN (natural persons), régime fiscal (IFU / Réel / AE / Simplifié), BL authorisation decision (n°, date), bank/CCP (optional).
2. Customer & supplier master data with the same identifiers; mandatory for B2B/revendeur types; RC/NIF verification date.
3. Document types with independent gapless series: Ticket, Facture, Avoir, Facture proforma/Devis (non-fiscal), Bon de livraison, Facture récapitulative mensuelle, Bon de transfert, Bon de transaction commerciale, Bon de commande/réception (purchase side).
4. Invoice layout containing every art. 3 mention; TTC in letters (FR, optional AR); stamp/signature box; "facture annulée" diagonal on voids; transport/supplements/consignation lines.
5. TVA engine: 19/9/0/exempt codes with validity dates; IFU mode hides TVA; specific taxes (in/out of TVA base, printed distinctly).
6. Timbre engine per §4 (flat bracket, ceil per 100 DA, min 5 DA, ≤ 300 exempt, no cap, cash only; configurable split-tender rule).
7. Tender recording (cash, cheque details, card with auth no., QR/BaridiMob, transfer details, COD, credit).
8. Inalterable fiscal journal + Z-close + archives + publisher-engagement readiness (2027).
9. Reports: G50 helper, G9, état spécial art. 18, état clients, registres IFU, G12/G12 bis helper, timbre statement, VAT per rate.
10. Retention ≥ 10 years (documents), ≥ 6 years (data/tech docs); no hard delete.
11. Personal-data compliance: consent flags, access log, export/rectify/erase, DPO field, local storage by default, sync contract (DZBuild as subcontractor), breach procedure.
12. Arabic/French bilingual UI, tickets and labels; RTL printing.
13. 69-wilaya dataset (+58 mapping) for customers & deliveries.

**SHOULD (market/practice)**
- Lot & expiry tracking; regulated-price flags; landed cost for importers; carrier integrations (Yalidine, ZR/Procolis, Ecotrack family) with COD reconciliation; Chargily payment links; CCP Business/BaridiMob QR display; Hijri calendar & editable holidays; Ramadan hours profiles; fiscal calendar reminders; yearly "LF update pack" (rates, thresholds, dates) pushed by DZBuild without reinstall.

---

## 16. Open questions / UNVERIFIED items (to re-check before release)

1. Implementing decree for **ticket de caisse** mentions (loi 04-02 art. 12 as amended by LFC 2018) — exists?
2. **Arrêté** fixing the model of the publisher engagement / certificate and the accredited bodies (LF 2026 art. 74 → CPF 20 ter-2), and the arrêté on the format of dematerialised accounting files (CPF 20 ter-3-B). Expected before 1/1/2027.
3. Joint arrêté on **telematic invoices** (décret 05-468 art. 11).
4. Whether invoice numbering must restart each fiscal year (not in the décret).
5. Who legally bears the timbre de quittance; treatment of split tenders; rounding.
6. Official weekend decree reference (2009); Eid 3-day law reference (2023); Ramadan official hours.
7. Interbank QR payment standard (Banque d'Algérie/GIE Monétique) status 2026; TPE-ECR integration standard.
8. Maystro and Noest API documentation; Yalidine full field list (docs blocked to bots).
9. Loi 26-06 JO page itself (parent wilaya of Bir El Ater: Tébessa vs other).
10. Whether an Arabic-language requirement applies to invoices themselves (loi 91-05; JO 1991 scan not machine-readable).
11. Name of the auto-entrepreneur registry body (ANAE) and its platform URL.

---

## 17. Sources (all accessed 2026-09-28)

**Official — JORADP**
- Décret 05-468 — JO 2005 n° 80: https://www.joradp.dz/FTP/jo-francais/2005/F2005080.pdf
- Loi 04-02 — JO 2004 n° 41: https://www.joradp.dz/FTP/jo-francais/2004/F2004041.pdf
- Loi 10-06 — JO 2010 n° 46: https://www.joradp.dz/FTP/jo-francais/2010/F2010046.pdf
- Décret 16-66 — JO 2016 n° 10: https://www.joradp.dz/FTP/jo-francais/2016/F2016010.pdf
- Loi 18-07 — JO 2018 n° 34: https://www.joradp.dz/FTP/jo-francais/2018/F2018034.pdf
- Loi 18-05 — JO 2018 n° 28: https://www.joradp.dz/FTP/jo-francais/2018/F2018028.pdf
- LF 2018 (art. 111) — JO 2017 n° 76: https://www.joradp.dz/FTP/jo-francais/2017/F2017076.pdf
- Décret 15-153 — JO 2015 n° 33: https://www.joradp.dz/FTP/jo-francais/2015/F2015033.pdf
- Décret 13-378 — JO 2013 n° 58: https://www.joradp.dz/FTP/jo-francais/2013/F2013058.pdf
- Loi 22-23 (auto-entrepreneur) — JO 2022 n° 85: https://www.joradp.dz/FTP/jo-francais/2022/F2022085.pdf

**Official — DGI (mfdgi.gov.dz)**
- LF 2026 full text (JO n° 88): https://www.mfdgi.gov.dz/files/537/20212026/3710/LF2026.pdf
- Communiqué général LF 2026: https://mfdgi.gov.dz/files/658/Communiques-de-loi-de-finances/3378/Communique-general-de-la-loi-de-finances-2026.pdf
- Recueil des circulaires LF 2025 (incl. circulaire 14 timbre de quittance): https://www.mfdgi.gov.dz/files/583/2020-2025/3727/Recueil-des-circulaires-d-application-de-la-Loi-de-Finances-pour-2025.pdf
- Code du timbre 2026: https://www.mfdgi.gov.dz/files/803/2026/3724/CodedeTimbre2026fr.pdf
- CTCA 2026: https://www.mfdgi.gov.dz/files/803/2026/3725/CodedesTaxessurle-Chiffred-Affaires2026fr.pdf
- CPF 2026: https://www.mfdgi.gov.dz/files/803/2026/3723/CodedesProceduresFiscales2026fr.pdf
- CIDTA 2026: https://www.mfdgi.gov.dz/files/803/2026/3726/CodedesImpotsDirectsetTaxesAssimilees2026fr.pdf
- Circulaire 06/2026 (option réel irrévocable): https://www.mfdgi.gov.dz/files/804/2026/3563/Circulaire-n06-du-04-03-2026-relative-aux-nouveaux-contribuables--irrevocabilite-de-l-option-au-regime-du-benefice-reel-ou-au-regime-simplifie-des-professions-non-commerciales.pdf
- Circulaire 26/2026 (TVA 9 %): https://www.mfdgi.gov.dz/files/804/2026/3740/Circulaire-n26-du-20-04-2026-relative-a-lextension-de-l-application-du-taux-reduit-d-certaines-prestations-TVA.pdf
- Circulaire 29/2026 (mesures d'ordre, IFU 8 M DA switch): https://www.mfdgi.gov.dz/files/804/2026/3777/Circulaire-n29-du-05-05-2026-relative-aux-mesures-d-ordre.pdf
- Circulaire 31/2026 (e-filing): https://www.mfdgi.gov.dz/files/804/2026/3778/Circulaire-n31-du-05-05-2026-relative-aux-modalites-de-declaration-et-de-paiement-des-impots-et-taxes.pdf
- Circulaire 39/2026 (IFU contractual regime abrogated): https://www.mfdgi.gov.dz/files/804/2026/3737/Circulaire-n39-du-07-05-2026-relative-a-labrogation-du-regime-contractuel-de-l-Impot-Forfaitaire-Unique-IFU.pdf
- Circulaire 41/2026 (précompte 2 %): https://www.mfdgi.gov.dz/files/804/2026/3776/Circulaire-n41-du-13-05-2026-relative-au-precompte-a-l-importation-de-2--Exclusion-des-operations-d-importation-de-marchandisesIBS.pdf
- IFU page: https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-forfaitaire-unique/ifu
- TVA page: https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-reel/la-taxe-sur-la-valeur-ajoutee
- Régime réel page: https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-reel/regime-reel
- TLS page: https://www.mfdgi.gov.dz/fr/professionnels/services-pro/regime-reel/tls
- NIF / dossier fiscal: https://www.mfdgi.gov.dz/fr/professionnels/nouveau-contribuable-fr/ouvrir-dossier-fiscal
- Fiscal calendar: https://www.mfdgi.gov.dz/fr/professionnels/calandrier-fiscal-pro
- ERA communiqué: https://www.mfdgi.gov.dz/files/781/2026-2025/3811/Com-ERA-2024-2025-fr.pdf
- G50 notice: https://www.mfdgi.gov.dz/files/521/Formulaires-de-declaration/63/Notice-comment-remplir-GN50.pdf
- G9 état fournisseurs 2025: https://www.mfdgi.gov.dz/files/521/Formulaires-de-declaration/3142/G-9-Etat-fournisseurs--2025.pdf
- G3: https://www.mfdgi.gov.dz/files/521/Formulaires-de-declaration/57/GN3.pdf

**Official — other**
- Ministère du Commerce, décret 05-468 page: https://www.commerce.gov.dz/fr/reglementation/decret-executif-n05-468
- DRC Blida, facture: https://www.drcblida.dz/index.php/fr/informations-utiles/74-facture
- ANPDP: https://anpdp.dz/ar/ ; consolidated loi 18-07 (with loi 25-11): https://anpdp.dz/ar/storage/2025/08/18-07-Edited.pdf
- GIE Monétique: https://giemonetique.dz/qui-sommes-nous/chiffres/ ; https://giemonetique.dz/actualites-medias/actualites/
- CIBWEB: https://www.cibweb.dz/fr/
- SATIM: https://www.satim.dz/index.php/fr/
- Algérie Poste CCP Business Cashless: https://cashless.poste.dz/

**Secondary / technical**
- Grant Thornton, LF 2025 principales mesures: https://www.grantthornton.dz/contentassets/2cd27e8e1c9e4a3e818486914e27a39c/mes-pages/homepage/gt_loi-de-finances-2025-fr.pdf
- Deloitte, LF 2026: https://blog.avocats.deloitte.fr/algerie-les-principales-mesures-de-la-loi-de-finances-pour-2026/
- Legal Doctrine, facture & timbre: https://legal-doctrine.com/en/edition/quelles-sont-les-conditions-et-modalites-detablissement-dune-facture-en-algerie ; https://legal-doctrine.com/en/edition/reamenagement-du-mode-de-calcul-du-droit-de-timbre-de-quittance-une-reforme-fiscale-importante-b8cda97cd9e7d45028f7b4fa96c01bc6
- IntelliX (timbre interpretation debate): https://www.intellixgroup.com/blog/algerie-droit-de-timbre-2025-loi-de-finance-journal-officiel-probleme-et-solutions
- Cirtasoft (timbre 2025): https://www.cirtasoft.com/droit-de-timbre-selon-la-loi-de-finances-2025
- Lamacta / Almawarid blogs (contain unverified claims, see §2.1, §6.2): https://lamacta.com/blog/decret-executif-05-468-facture-legale-algerie/ ; https://almawarid.app/blog/facture-conforme-algerie-decret-05-468-guide-complet/
- Chargily Pay docs: https://dev.chargily.com/llms.txt ; https://dev.chargily.com/pay-v2/webhooks.md ; https://dev.chargily.com/pay-v2/api-reference/checkouts/create.md ; https://dev.chargily.com/pay-v2/fees.md
- Carrier SDK: https://github.com/tkawen/shipping-dz ; endpoints probed: https://api.yalidine.app/v1/ , https://procolis.com/api_v1/ , https://dhd.ecotrack.dz/api/v1/get/wilayas
- Wilaya datasets: https://github.com/islam-re/Algeria-wilayas ; https://github.com/wadiemendja/algeria-communes-wilayas ; https://github.com/othmanus/algeria-cities ; https://github.com/AbdeldjalilChougui/dz_kit
- Wikipedia: https://fr.wikipedia.org/wiki/Wilayas_d%27Alg%C3%A9rie ; https://en.wikipedia.org/wiki/Provinces_of_Algeria ; https://fr.wikipedia.org/wiki/F%C3%AAtes_et_jours_f%C3%A9ri%C3%A9s_en_Alg%C3%A9rie ; https://fr.wikipedia.org/wiki/Dinar_alg%C3%A9rien
- num2words (FR_DZ / AR): https://github.com/savoirfairelinux/num2words/blob/master/num2words/lang_FR_DZ.py ; https://github.com/savoirfairelinux/num2words/blob/master/num2words/lang_AR.py

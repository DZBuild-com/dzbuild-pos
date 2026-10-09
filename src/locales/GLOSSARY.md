# DZBuild POS glossary (ar / fr / en)

One concept, one word, everywhere (spec §10.6). Algerian business usage wins over textbook terms.
Add a row before introducing a new business word in any catalogue.

| # | Concept | Français | العربية | English | Notes |
|---|---|---|---|---|---|
| 1 | Consumer sale receipt | Ticket de caisse | تذكرة الصندوق | Receipt | "ticket" in speech |
| 2 | Invoice | Facture | فاتورة | Invoice | mentions of décret 05-468 |
| 3 | Credit note | Avoir | إشعار دائن (أفوار) | Credit note | corrects a validated invoice |
| 4 | Delivery note | Bon de livraison (BL) | وصل التسليم | Delivery note | |
| 5 | Quote | Devis | عرض سعر | Quote | |
| 6 | Proforma invoice | Facture proforma | فاتورة شكلية | Proforma invoice | |
| 7 | Customer credit | Crédit (kridi) | الكريدي (الدين) | Credit (kridi) | kridi is the shop word |
| 8 | Credit payment | Versement | دفعة | Credit payment | "versement" also said in Arabic |
| 9 | Opening float | Fond de caisse | رصيد الصندوق الافتتاحي | Opening float | |
| 10 | Cash close | Clôture de caisse | إغلاق الصندوق | Cash close | |
| 11 | Z report | Rapport Z (clôture Z) | تقرير Z | Z report | numbered per terminal |
| 12 | X report | Rapport X | تقرير X | X report | no reset |
| 13 | Cash drawer | Tiroir-caisse | درج النقود | Cash drawer | |
| 14 | Discount | Remise | تخفيض | Discount | |
| 15 | VAT | TVA | الرسم على القيمة المضافة | VAT (TVA) | keep "TVA" in fr/en UI |
| 16 | Stamp duty | Droit de timbre | حقوق الطابع | Stamp duty | cash payments only |
| 17 | Tax ID | NIF | رقم التعريف الجبائي | Tax ID (NIF) | 15 digits, 18 for a secondary establishment (lengths from the data pack) |
| 18 | Statistical ID | NIS | رقم التعريف الإحصائي | Statistical ID (NIS) | |
| 19 | Trade register | Registre du commerce (RC) | السجل التجاري | Trade register (RC) | |
| 20 | Tax article | Article d'imposition (AI) | رقم المادة الضريبية | Tax article (AI) | |
| 21 | Flat-rate regime | Régime IFU | الضريبة الجزافية الوحيدة | IFU regime | TTC only, no TVA shown |
| 22 | Real regime | Régime du réel | النظام الحقيقي | Real regime | TVA per rate |
| 23 | All taxes included | TTC | بكل الرسوم | Incl. tax (TTC) | |
| 24 | Before tax | HT | خارج الرسوم | Excl. tax (HT) | |
| 25 | Amount due | Net à payer | الصافي للدفع | Amount due | |
| 26 | Change given | Rendu (monnaie) | الباقي | Change | |
| 27 | Take payment | Encaisser | قبض | Take payment | button: Encaisser (F12) |
| 28 | Stock | Stock | المخزون | Stock | |
| 29 | Stock count | Inventaire | الجرد | Stock count | |
| 30 | Supplier | Fournisseur | المورّد | Supplier | |
| 31 | Purchase | Achat | الشراء | Purchase | |
| 32 | Purchase order | Bon de commande | وصل الطلب | Purchase order | |
| 33 | Goods receipt | Bon de réception | وصل الاستلام | Goods receipt | |
| 34 | Category | Catégorie | الفئة | Category | |
| 35 | Unit | Unité | الوحدة | Unit | |
| 36 | Barcode | Code-barres | الرمز الشريطي (باركود) | Barcode | |
| 37 | Retail price | Prix de détail | سعر التجزئة | Retail price | |
| 38 | Wholesale price | Prix de gros | سعر الجملة | Wholesale price | |
| 39 | Semi-wholesale price | Prix de demi-gros | سعر نصف الجملة | Semi-wholesale price | |
| 40 | Cash session | Session de caisse | جلسة الصندوق | Cash session | |
| 41 | Return | Retour | إرجاع | Return | bon de retour |
| 42 | Hold a sale | Mettre en attente | تعليق | Hold | |
| 43 | Backup | Sauvegarde | نسخة احتياطية | Backup | |
| 44 | Restore | Restaurer | استرجاع | Restore | |
| 45 | User | Utilisateur | المستخدم | User | |
| 46 | Role | Rôle | الدور | Role | |
| 47 | Owner | Propriétaire | المالك | Owner | built-in role |
| 48 | Manager | Gérant | المسيّر | Manager | built-in role |
| 49 | Cashier | Caissier | أمين الصندوق | Cashier | built-in role |
| 50 | Storekeeper | Magasinier | أمين المخزن | Storekeeper | built-in role |
| 51 | Terminal | Poste | الجهاز | Terminal | codes C1, C2… |
| 52 | Printer | Imprimante | الطابعة | Printer | |
| 53 | Label | Étiquette | الملصق | Label | |
| 54 | Scale | Balance | الميزان | Scale | |
| 55 | Customer | Client | الزبون | Customer | |
| 56 | PIN code | Code PIN | الرمز السري | PIN | 4-8 digits |
| 57 | Audit journal | Journal des opérations | سجل العمليات | Action log | each line locked to the one before (hash-chained); was "Journal d'audit" / "سجل التدقيق" / "Audit journal" until 1.0.0 |
| 58 | Wilaya | Wilaya | الولاية | Wilaya | 69 since 2026 |
| 59 | Settings | Réglages | الإعدادات | Settings | |
| 60 | Hardware | Matériel | العتاد | Hardware | settings section (M4) |
| 61 | Shop logo | Logo | الشعار | Logo | PNG or JPEG, at most 512 KB; printed on documents |
| 62 | Customer statement | Relevé de compte | كشف حساب | Statement | kridi and supplier accounts |
| 63 | Versement receipt | Reçu de versement | وصل دفع | Payment receipt | numbered `RV2026-000001`, carries the timbre |
| 64 | Store credit | Avoir client (bon d'achat) | رصيد لدى المحل | Store credit | negative balance of the account |
| 65 | Credit limit | Plafond de crédit | سقف الكريدي | Credit limit | |
| 66 | Ageing | Ancienneté des créances | أعمار الديون | Ageing | 0–30, 31–60, 61–90, 90+ days |
| 67 | Supplier invoice | Facture fournisseur | فاتورة المورد | Supplier invoice | `FF…`, their number kept |
| 68 | Supplier return | Retour fournisseur | إرجاع للمورد | Supplier return | `BR2026-F-…` |
| 69 | Stock transfer | Bon de transfert | وصل تحويل | Transfer note | `BT…` |
| 70 | Lot | Lot | الحصة | Lot | with its expiry date |
| 71 | Expiry date | Date de péremption (DLC) | تاريخ انتهاء الصلاحية | Expiry date | |
| 72 | Serial number | Numéro de série | الرقم التسلسلي | Serial number | IMEI for phones |
| 73 | Weighted average cost | CMUP (coût moyen d'achat) | التكلفة المتوسطة المرجحة | Average cost (CMUP) | HT; first use on a screen glosses it: "average purchase cost (CMUP)" |
| 74 | Promotion | Promotion | ترويج | Promotion | |
| 75 | Legal sale | Soldes | التخفيضات الموسمية | Seasonal sale | dates set by the wali |
| 76 | Expense | Dépense | مصروف | Expense | |
| 77 | Import | Importer | استيراد | Import | from Excel or CSV |
| 78 | Export | Exporter | تصدير | Export | xlsx, csv |
| 79 | Dry run | Vérification | تحقق مسبق | Check | "Vérifier le fichier" before importing |
| 80 | Dashboard | Tableau de bord | لوحة القيادة | Dashboard | |
| 81 | Report | Rapport | تقرير | Report | |
| 82 | National ID number | NIN | رقم التعريف الوطني | National ID (NIN) | 18 digits; the owner of a sole-trader shop, a customer's ID |
| 83 | Stock location | Emplacement | الموقع (موقع التخزين) | Location | shop floor, dépôt, van; the seeded "Magasin" is the default |
| 84 | Count variance | Écart | الفارق | Variance | counted minus expected, valued at the CMUP |
| 85 | Minimum stock | Stock minimum | الحد الأدنى للمخزون | Minimum stock | per product and location; the low-stock alert |
| 86 | Blind count | Inventaire à l'aveugle | جرد أعمى | Blind count | the counter does not see the expected quantities |
| 87 | Opening balance | Solde d'ouverture | رصيد افتتاحي | Opening balance | kridi or supplier debt carried over from before DZBuild POS; once per account |
| 88 | Opening stock | Stock d'ouverture | المخزون الافتتاحي | Opening stock | the quantities on hand when the shop starts with DZBuild POS (import) |
| 89 | Mandatory mentions | Mentions obligatoires | البيانات الإلزامية | Mandatory mentions | décret 05-468 art. 3 |
| 90 | Document archive | Archive des documents | أرشيف الوثائق | Document archive | every document, never deleted |
| 91 | Fiscal archive | Archive fiscale | الأرشيف الجبائي | Fiscal archive | zip with manifest, 10 years |
| 92 | Settlement date | Date de règlement | تاريخ التسديد | Due date | on invoices paid later |
| 93 | Valid until | Valable jusqu'au | صالح إلى غاية | Valid until | quotes, proformas |
| 94 | Amount in words | Montant en lettres | المبلغ بالحروف | Amount in words | "Arrêtée la présente facture à la somme de" |
| 95 | Duplicate | Duplicata | نسخة مكررة | Duplicate | every output after the first |
| 96 | VAT statement | État de TVA | كشف الرسم على القيمة المضافة | VAT statement | per rate |
| 97 | Stamp duty statement | État du droit de timbre | كشف حقوق الطابع | Stamp duty statement | monthly |
| 98 | G50 return | Déclaration G50 | التصريح G50 | G50 return | monthly, réel |
| 99 | Supplier statement (G9) | État des fournisseurs (G9) | كشف الموردين (G9) | Supplier statement (G9) | G50 annex |
| 100 | Annual customer statement | État clients | كشف الزبائن | Annual customer statement | CIDTA art. 183 ter |
| 101 | Sales register | Registre des ventes | سجل المبيعات | Sales register | |
| 102 | Purchase register | Registre des achats | سجل المشتريات | Purchase register | |
| 103 | Deductible VAT | TVA déductible | الرسم على القيمة المضافة القابل للخصم | Deductible VAT | from supplier invoices |
| 104 | Output VAT | TVA collectée | الرسم على القيمة المضافة المحصّل | Output VAT | on sales |
| 105 | VAT-exempt customer | Client exonéré de TVA | زبون معفى من الرسم على القيمة المضافة | VAT-exempt customer | with an attestation number |
| 106 | Warranty certificate | Certificat de garantie | شهادة الضمان | Warranty certificate | décret 13-327 |
| 107 | Goods credit note | Avoir marchandise | إشعار دائن بالبضاعة | Goods credit note | goods come back |
| 108 | Financial credit note | Avoir financier | إشعار دائن مالي | Financial credit note | price reduction |
| 109 | Print preview | Aperçu avant impression | معاينة قبل الطباعة | Print preview | |
| 110 | Bilingual | Bilingue | ثنائي اللغة | Bilingual | AR + FR |
| 111 | Stamp and signature | Cachet et signature | الختم والإمضاء | Stamp and signature | |
| 112 | Draft | Brouillon | مسودة | Draft | not numbered |
| 113 | Convert | Transformer en | تحويل إلى | Convert to | DV → FA, BL → FA |
| 114 | Business activity | Nature de l'activité | طبيعة النشاط | Business activity | |
| 115 | Legal form | Forme juridique | الشكل القانوني | Legal form | SARL, EURL… |
| 116 | Share capital | Capital social | رأس المال الاجتماعي | Share capital | |
| 117 | Auto-entrepreneur card | Carte d'auto-entrepreneur | بطاقة المقاول الذاتي | Auto-entrepreneur card | instead of the RC |
| 118 | Label printer | Imprimante d'étiquettes | طابعة الملصقات | Label printer | TSPL (Xprinter, TSC) or ZPL (Zebra, Godex) |
| 119 | Label template | Modèle d'étiquette | نموذج الملصق | Label template | size, fields, language |
| 120 | Shelf label | Étiquette de rayon | ملصق الرف | Shelf label | the price the customer pays (TTC) |
| 121 | Label sheet | Planche d'étiquettes A4 | ورقة ملصقات A4 | Label sheet | 3×7, 3×8, 4×10, 5×13 |
| 122 | Labels to reprint | Étiquettes à refaire | ملصقات يجب إعادة طبعها | Labels to reprint | price, promotion, name or barcode changed |
| 123 | Weighed item | Article au poids | سلعة بالوزن | Weighed item | sold in kg, weight = quantity |
| 124 | PLU | Numéro PLU | رقم PLU | PLU | 1–99999, the scale's item number |
| 125 | Tare | Tare | وزن الغلاف (تارة) | Tare | subtracted from the gross weight |
| 126 | Stable weight | Poids stable | وزن مستقر | Stable weight | only a stable weight is added |
| 127 | Weight barcode | Code-barres poids/prix | باركود الوزن/السعر | Weight barcode | EAN-13 starting with 2 |
| 128 | Pole display | Afficheur client | شاشة الزبون (VFD) | Pole display | 2 lines × 20, Latin text |
| 129 | Customer screen | Écran client | الشاشة الثانية للزبون | Customer screen | second monitor, trilingual |
| 130 | Data collector | Terminal de collecte | جهاز الجرد المحمول | Data collector | `barcode;qty` files |
| 131 | Serial port | Port série (COM) | المنفذ التسلسلي (COM) | Serial port | COM3, USB-serial adapters |
| 132 | Windows Credential Manager | Gestionnaire d'identification de Windows | مدير بيانات الاعتماد في Windows | Windows Credential Manager | keeps a till's key (M5) |
| 133 | Local network | Réseau local | الشبكة المحلية | Local network | settings section "Réseau" |
| 134 | Main PC (host) | Poste principal | الجهاز الرئيسي | Main PC | holds the books, serves the tills |
| 135 | Till (client) | Poste de caisse | جهاز البيع | Till | sells on its own copy |
| 136 | Single PC | Poste unique | جهاز واحد | Single PC | the default role |
| 137 | Pairing code | Code d'appairage | رمز الربط | Pairing code | 6 digits, 10 minutes, once |
| 138 | Add a till | Ajouter un poste de caisse | إضافة جهاز بيع | Add a till | button on the main PC |
| 139 | Join the main PC | Rejoindre le poste principal | الانضمام إلى الجهاز الرئيسي | Join the main PC | on the new till |
| 140 | Till series | Série du poste | سلسلة الجهاز | Till series | C2, C3…; in every ticket number |
| 141 | Offline | Hors ligne | غير متصل | Offline | "La vente continue hors ligne" |
| 142 | To send | À envoyer | في انتظار الإرسال | To send | "3 opérations à envoyer" |
| 143 | Offline Z | Z hors ligne | إقفال Z دون اتصال | Offline Z | a Z closed with operations still to send |
| 144 | Make this till the main PC | Faire de ce poste le poste principal | جعل هذا الجهاز الجهاز الرئيسي | Make this the main PC | only when the main PC is dead |
| 145 | Leave the network | Quitter le réseau | مغادرة الشبكة | Leave the network | the till's copy is archived |
| 146 | Firewall | Pare-feu | جدار الحماية | Firewall | Windows Defender Firewall |
| 147 | PC clock | Heure du PC | ساعة الجهاز | PC clock | compared with the main PC |
| 148 | DZBuild link | Liaison DZBuild | الربط مع DZBuild | DZBuild link | optional, never required |
| 149 | Online store | Boutique en ligne | المتجر الإلكتروني | Online store | the merchant's DZBuild store |
| 150 | Link the store | Lier la boutique | ربط المتجر | Link the store | button; "Délier" / "إلغاء الربط" / "Unlink" |
| 151 | Sync | Synchronisation | المزامنة | Sync | "Synchroniser maintenant" |
| 152 | Online order | Commande en ligne | طلبية إلكترونية | Online order | from the DZBuild store |
| 153 | Claim an order | Prendre en charge | التكفّل بالطلبية | Claim | one till prepares it |
| 154 | Prepare | Préparer | تحضير | Prepare | goods picked and packed |
| 155 | Cash on delivery | Paiement à la livraison | الدفع عند الاستلام | Cash on delivery (COD) | |
| 156 | Publish online | Publier en ligne | نشر على الإنترنت | Publish online | per product |
| 157 | Draft (online) | Brouillon | مسودة | Draft | product kept off the storefront (plan cap) |
| 158 | Device code | Code de l'appareil | رمز الجهاز | Device code | e.g. DZKM-4Q7P, approved from a phone |
| 159 | Online backup | Sauvegarde en ligne | نسخة احتياطية على الإنترنت | Online backup | removed in 1.0.0 (free edition); kept for the journal labels of older versions |
| 160 | Passphrase | Phrase secrète | العبارة السرية | Passphrase | removed with the online backup in 1.0.0 |
| 161 | Recovery code | Code de secours | رمز الاسترجاع | Recovery code | removed with the online backup in 1.0.0 |
| 162 | Not available yet | Pas encore disponible | غير متوفر بعد | Not available yet | a DZBuild feature not shipped |
| 163 | Blacklisted buyer | Acheteur signalé | مشترٍ مُبلَّغ عنه | Flagged buyer | DZBuild `is_banned` / fraud score |
| 164 | Crash | Plantage | عطل | Crash | `crash.log`: "Dernier plantage" / "آخر عطل" / "Last crash" (About, Diagnostic) |
| 165 | Update | Mise à jour | تحديث | Update | "Installer la mise à jour" / "تثبيت التحديث" / "Install the update" |
| 166 | What's new | Nouveautés | ما الجديد | What's new | release notes shown before and after an update |
| 167 | Update channel | Canal de mise à jour (stable, bêta) | قناة التحديث (مستقرة، تجريبية) | Update channel (stable, beta) | |
| 168 | Second copy | Copie de secours (clé USB ou dossier) | نسخة احتياطية ثانية (مفتاح USB أو مجلد) | Second copy (USB stick or folder) | the mirror of the backups |
| 169 | Integrity check | Vérification d'intégrité | فحص السلامة | Integrity check | `PRAGMA integrity_check` |
| 170 | Guided restore | Restauration guidée | استرجاع موجَّه | Guided restore | from the fatal screen |
| 171 | Diagnostic | Diagnostic | التشخيص | Diagnostic | the zip for support |
| 172 | Support | Assistance | الدعم الفني | Support | WhatsApp or web page |
| 173 | Demo shop | Boutique de démonstration | متجر تجريبي | Demo shop | "DÉMO" on every screen and receipt |
| 174 | Help centre | Centre d'aide | مركز المساعدة | Help centre | F1 |
| 175 | On-screen keyboard | Clavier à l'écran | لوحة المفاتيح على الشاشة | On-screen keyboard | AZERTY and Arabic |
| 176 | Light mode | Mode allégé | الوضع الخفيف | Light mode | fewer animations, for slow PCs |
| 177 | Activity template | Modèle d'activité | نموذج النشاط | Activity template | chosen in the setup wizard, §30.2 |
| 178 | Module | Module | وحدة | Module | a pack feature the owner turns on or off |
| 179 | Table | Table | طاولة | Table | restaurant |
| 180 | Dining room | Salle | قاعة | Dining room | terrace, first floor… |
| 181 | Table ticket | Ticket de table | تذكرة الطاولة | Table ticket | open until paid, not a receipt |
| 182 | Pre-bill | Addition | الحساب | Pre-bill | "ce document n'est pas un ticket de caisse" |
| 183 | Kitchen ticket | Bon de cuisine | وصل المطبخ | Kitchen ticket | per station, cancellation tickets too |
| 184 | Preparation station | Poste de préparation | مركز التحضير | Station | kitchen, grill, pizza, bar |
| 185 | Modifier | Option (supplément) | خيار (إضافة) | Modifier | sauce, fromage, taille |
| 186 | Dine in | Sur place | في المحل | Dine in | |
| 187 | Takeaway | À emporter | للأخذ | Takeaway | with a daily order number |
| 188 | Delivery | Livraison | توصيل | Delivery | |
| 189 | Covers | Couverts | عدد الأشخاص | Covers | |
| 190 | Waiter | Serveur | النادل | Waiter | built-in role |
| 191 | Daily menu | Menu du jour | قائمة اليوم | Daily menu | |
| 192 | Controlled medicines register | Registre des médicaments contrôlés | سجل الأدوية الخاضعة للرقابة | Controlled medicines register | pharmacy pack only |
| 193 | Chifa reference price | Tarif de référence Chifa | السعر المرجعي للشفاء | Chifa reference price | informative |
| 194 | Generic | Générique | دواء جنيس | Generic | same DCI |
| 195 | Cutting yield | Rendement de découpe | مردود التقطيع | Cutting yield | carcass → cuts |
| 196 | Waste | Pertes (casse) | التالف | Waste | daily log |
| 197 | Repair ticket | Bon de réparation | وصل التصليح | Repair ticket | the intake slip (وصل دخول) |
| 198 | Technician | Technicien | التقني | Technician | built-in role |
| 199 | IMEI | IMEI | رقم IMEI | IMEI | 15 digits, Luhn check |
| 200 | Top-up | Flexy | فليكسي | Top-up (flexy) | Mobilis, Djezzy, Ooredoo |
| 201 | Mobile operator | Opérateur | المتعامل | Operator | |
| 202 | Commission | Commission | العمولة | Commission | |
| 203 | Instalment sale | Vente à tempérament (facilité) | البيع بالتقسيط | Instalment sale | |
| 204 | Instalment schedule | Échéancier | جدول الأقساط | Schedule | |
| 205 | Down payment | Apport initial | الدفعة الأولى | Down payment | |
| 206 | Instalment | Échéance | القسط | Instalment | |
| 207 | Supporting document | Pièce justificative | وثيقة إثبات | Supporting document | never a cheque |
| 208 | Delivery round | Tournée | جولة التوزيع | Delivery round | `TR…` |
| 209 | Route sheet | Bon de route | ورقة الطريق | Route sheet | driver, plate, BLs |
| 210 | Driver | Chauffeur | السائق | Driver | |
| 211 | Customer price | Prix client | سعر الزبون | Customer price | per customer and product |
| 212 | Minimum order | Quantité minimale de commande | الحد الأدنى للطلب | Minimum order | |
| 213 | Customer order | Commande client | طلبية زبون | Customer order | `CC…`, printed "Bon de commande client" |
| 214 | Deposit | Arrhes (acompte) | العربون | Deposit | `AC…` receipt |
| 215 | Layaway | Vente avec réservation | البيع مع الحجز | Layaway | sold when fully paid |
| 216 | Appointment | Rendez-vous | موعد | Appointment | |
| 217 | Loyalty points | Points fidélité | نقاط الوفاء | Loyalty points | |
| 218 | Loyalty card | Carte fidélité | بطاقة الوفاء | Loyalty card | barcode |
| 219 | Gift voucher | Bon cadeau | قسيمة هدية | Gift voucher | `BA…` |
| 220 | Exchange voucher | Bon d'échange | قسيمة استبدال | Exchange voucher | a return without customer |
| 221 | Loyalty voucher | Chèque fidélité | قسيمة الوفاء | Loyalty voucher | points turned into a voucher |
| 222 | Price per gram | Prix du gramme | سعر الغرام | Price per gram | per metal and fineness |
| 223 | Fineness | Titre | العيار | Fineness | ‰, 750 = 18 carats |
| 224 | Making charge | Façon | المصنعية | Making charge | |
| 225 | Hallmark | Poinçon | الدمغة | Hallmark | |
| 226 | Jeweller register | Registre spécial des clients | سجل الزبائن الخاص | Jeweller register | CII art. 359, LF 2026 art. 62 |
| 227 | Employee | Employé | الموظف | Employee | |
| 228 | Salary advance | Avance sur salaire | تسبيق على الأجر | Salary advance | |
| 229 | Zakat | Zakat | الزكاة | Zakat | not a fatwa |
| 230 | Nisab | Nissab | النصاب | Nisab | data pack |
| 231 | Hijri date | Date hégirienne | التاريخ الهجري | Hijri date | Umm al-Qura, ±1 day |
| 232 | Department key | Touche rayon | زر القسم | Department key | price typed at the till |
| 233 | Regulated price | Prix plafonné (subventionné) | سعر مسقف (مدعم) | Regulated price | |
| 234 | Payment type | Type de paiement | نوع الدفع | Payment type | e.g. prélèvement CCP |
| 235 | Service ticket | Ticket de service | تذكرة خدمة | Service ticket | pressing, tailoring |
| 236 | Seat | Place | المقعد | Seat | a guest's place at a table; a bill splits by seat |
| 237 | Course | Service | ترتيب التقديم | Course | Entrée, Plat, Dessert, Boisson: a label on the kitchen ticket |
| 238 | Comp (offered line) | Offert (offrir) | إهداء | Comp | 100 % off with a reason (RST-008) |
| 239 | Voucher (tender tile) | Bon d'achat | قسيمة شراء | Voucher | the payment tile that spends a gift voucher, a deposit or the customer's store credit |
| 240 | Van sale | Vente depuis le camion | البيع من الشاحنة | Sale from the van | VRT-101: the till sells a van's stock on its round |
| 241 | INN (generic name) | DCI (dénomination commune) | الاسم العلمي المشترك (DCI) | INN | a medicine's generic name: the key of its generics (row 194) |
| 242 | Carcass | Carcasse | الذبيحة | Carcass | cut up in a cutting run (`DC…`) |
| 243 | Cut | Morceau | القطعة | Cut | gigot, épaule, côtelettes |
| 244 | Cutting template | Modèle de découpe | نموذج التقطيع | Cutting template | a carcass and the expected share of each cut |
| 245 | Variant grid | Grille (tailles × couleurs) | الشبكة (المقاسات × الألوان) | Grid (sizes × colours) | VRT-060: stock, counts and receipts per variant |
| 246 | Ceiling | Plafond | السقف | Ceiling | of a regulated price (row 233) |
| 247 | Value at cost | Valeur au coût | القيمة بسعر التكلفة | Value at cost | quantity × CMUP (expiry board, waste) |
| 248 | Float (top-up credit) | Stock flexy | رصيد فليكسي | Float | credit bought from the distributor, per operator |
| 249 | Labour | Main-d'œuvre | اليد العاملة | Labour | a repair's service line, its price agreed |
| 250 | Not collected | Non récupéré | غير مستلم | Not collected | a ready item still waiting for its customer |
| 251 | Instalment reminder | Relance | تذكير | Reminder | a due's text copied into WhatsApp or an SMS; the POS sends nothing |
| 252 | Kept deposit | Arrhes conservées | عربون محتفظ به | Kept deposit | the customer gave up: sold on a ticket, never written off |
| 253 | Reserved stock | Stock réservé | المخزون المحجوز | Reserved stock | held for an order or a layaway until it is served or closed |
| 254 | Stop (of a round) | Arrêt | محطة | Stop | one delivery note or invoice delivered on a round |
| 255 | Van | Camion | الشاحنة | Van | a stock location loaded and unloaded by transfer notes |
| 256 | Cash to hand in | Espèces à remettre | النقد الواجب تسليمه | Cash to hand in | a round's cash collections with the customers' stamp duty |
| 257 | Pack (packaging) | Pack (conditionnement) | عبوة | Pack | carton, bag, pallet, box or set sold from the unit's stock |
| 258 | Delivery note standing as invoice | BL valant facture | وصل التسليم بمثابة فاتورة | Delivery note standing as invoice | only under the tax administration's decision (number and date) |
| 259 | Licence | Licence | الرخصة | Licence | per PC; "Réglages › Licence" / "الإعدادات › الرخصة" / "Settings › Licence" |
| 260 | Trial licence | Licence d'essai (7 jours) | رخصة تجريبية (7 أيام) | Trial licence (7 days) | counted from the day DZBuild issues it |
| 261 | Lifetime licence | Licence à vie | رخصة مدى الحياة | Lifetime licence | |
| 262 | Device code (licence) | Code de ce PC | رمز هذا الحاسوب | This PC's code | `DZPOS-XXXX-XXXX-XXXX-XXXX`, sent to DZBuild; not row 158's DZBuild-link code |
| 263 | Licence key | Clé de licence | مفتاح الرخصة | Licence key | starts with `DZL1.`; also a `.dzlic` file |
| 264 | Read-only mode | Mode consultation | وضع الاطلاع فقط | Read-only mode | no sales, no changes; the books stay open |
| 265 | Shop's data (database) | Données de la boutique | بيانات المحل | Shop's data | never "database" / "base de données" / "قاعدة البيانات" in user text; the file is "the shop's data file" |
| 266 | Check code (hash) | Code de contrôle | رمز التحقق | Check code | shown on a receipt or an archive (SHA-256); never "fingerprint" / "empreinte" / "بصمة" |
| 267 | Receivables / payables | Ce que les clients vous doivent / ce que vous devez aux fournisseurs | ما يدين به الزبائن لك / ما تدين به للموردين | Customers owe you / You owe suppliers | dashboard tiles; never "receivables" / "payables" |
| 268 | Technical logs | Journaux techniques | السجلات التقنية | Technical logs | a record of what the software did, for support; not the action log (row 57) |

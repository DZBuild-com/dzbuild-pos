//! The 25 activity templates of master spec §30.2: which modules they switch on, the till mode
//! and what they seed. Every seeded name exists in Arabic, French and English.

use super::{Module, TillMode};
use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
use crate::company::ShopActivity;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedName {
    pub ar: &'static str,
    pub fr: &'static str,
    pub en: &'static str,
}

pub const fn n(ar: &'static str, fr: &'static str, en: &'static str) -> SeedName {
    SeedName { ar, fr, en }
}

/// A service line the template creates (DOC-008 transport, labour, pressing grid…): a product of
/// kind `service`, no stock, `SVC-…` reference; `open_price` when its price is typed at the sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedService {
    pub code: &'static str,
    pub name: SeedName,
    pub unit_id: &'static str,
    pub tax_code: &'static str,
    /// Centimes, TTC in a TTC shop; 0 for an open-price service.
    pub price: i64,
    pub open_price: bool,
}

const fn svc(code: &'static str, name: SeedName, price: i64, open_price: bool) -> SeedService {
    SeedService {
        code,
        name,
        unit_id: UNIT_PIECE_ID,
        tax_code: "TVA19",
        price,
        open_price,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PackRelease {
    Mvp,
    V1,
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackTemplate {
    pub activity: ShopActivity,
    /// Master spec §30.2 "MVP?" column: what the pack's features are expected to cover today.
    pub release: PackRelease,
    pub modules: &'static [Module],
    pub till_mode: TillMode,
    pub categories: &'static [SeedName],
    pub services: &'static [SeedService],
    /// Receipt footer written when the shop has none (ONB-001 receipt layout).
    pub footer: Option<SeedName>,
}

use Module as M;

const TRANSPORT: SeedService = svc(
    "SVC-TRANSPORT",
    n("مصاريف النقل", "Frais de transport", "Transport"),
    0,
    true,
);
const DELIVERY: SeedService = svc(
    "SVC-DELIVERY",
    n("رسوم التوصيل", "Frais de livraison", "Delivery fee"),
    20_000,
    false,
);
const COPY: SeedService = svc(
    "SVC-COPY",
    n("نسخ ورقة", "Photocopie", "Photocopy"),
    1_000,
    false,
);
const PRINT: SeedService = svc(
    "SVC-PRINT",
    n("طباعة ورقة", "Impression", "Printing"),
    2_000,
    false,
);
const CONSIGN: SeedService = SeedService {
    code: "SVC-CONSIGN",
    name: n(
        "أمانة الأغلفة",
        "Consignation emballages",
        "Container deposit",
    ),
    unit_id: UNIT_PIECE_ID,
    tax_code: "EXO",
    price: 0,
    open_price: true,
};
const LABOUR: SeedService = svc(
    "SVC-LABOUR",
    n("يد عاملة", "Main d'œuvre", "Labour"),
    0,
    true,
);

pub const TEMPLATES: [PackTemplate; 25] = [
    PackTemplate {
        activity: ShopActivity::General,
        release: PackRelease::Mvp,
        modules: &[
            M::DepartmentKeys,
            M::RegulatedPrices,
            M::Lots,
            M::Loyalty,
            M::GiftVouchers,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("بقالة", "Épicerie", "Groceries"),
            n("مشتقات الحليب", "Produits laitiers", "Dairy"),
            n("مشروبات", "Boissons", "Drinks"),
            n(
                "نظافة وتنظيف",
                "Hygiène et entretien",
                "Hygiene and cleaning",
            ),
            n("خضر وفواكه", "Fruits et légumes", "Fruit and vegetables"),
            n("خبز ومعجنات", "Pain et viennoiserie", "Bread and pastries"),
            n("حلويات", "Confiserie", "Confectionery"),
        ],
        services: &[],
        footer: Some(n(
            "شكراً لزيارتكم",
            "Merci de votre visite",
            "Thank you for your visit",
        )),
    },
    PackTemplate {
        activity: ShopActivity::Butcher,
        release: PackRelease::Mvp,
        modules: &[
            M::DepartmentKeys,
            M::Yield,
            M::Waste,
            M::Loyalty,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("لحم أحمر", "Viande rouge", "Red meat"),
            n("دواجن", "Volaille", "Poultry"),
            n(
                "لحم مفروم ومرقاز",
                "Viande hachée et merguez",
                "Mince and merguez",
            ),
            n("أحشاء", "Abats", "Offal"),
            n("لحوم مصنعة حلال", "Charcuterie halal", "Halal cold cuts"),
            n("فواكه", "Fruits", "Fruit"),
            n("خضر", "Légumes", "Vegetables"),
        ],
        services: &[SeedService {
            code: "SVC-CUTTING",
            name: n(
                "تقطيع وتحضير",
                "Découpe et préparation",
                "Cutting and preparation",
            ),
            unit_id: UNIT_KG_ID,
            tax_code: "TVA19",
            price: 0,
            open_price: true,
        }],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Bakery,
        release: PackRelease::Mvp,
        modules: &[
            M::DepartmentKeys,
            M::Waste,
            M::Orders,
            M::Loyalty,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("خبز", "Pain", "Bread"),
            n("معجنات", "Viennoiserie", "Viennoiserie"),
            n("حلويات عصرية", "Pâtisserie", "Pastries"),
            n(
                "حلويات تقليدية",
                "Gâteaux traditionnels",
                "Traditional cakes",
            ),
            n("طلبيات خاصة", "Commandes spéciales", "Special orders"),
            n("مشروبات", "Boissons", "Drinks"),
        ],
        services: &[],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::FoodService,
        release: PackRelease::Mvp,
        modules: &[
            M::Restaurant,
            M::GiftVouchers,
            M::Loyalty,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Restaurant,
        categories: &[
            n("أطباق", "Plats", "Dishes"),
            n("سندويشات", "Sandwichs", "Sandwiches"),
            n("بيتزا", "Pizzas", "Pizzas"),
            n("مشروبات ساخنة", "Boissons chaudes", "Hot drinks"),
            n("مشروبات باردة", "Boissons fraîches", "Cold drinks"),
            n("تحليات", "Desserts", "Desserts"),
        ],
        services: &[
            DELIVERY,
            svc(
                "SVC-COVER",
                n("خدمة المائدة للشخص", "Couvert", "Cover charge"),
                5_000,
                false,
            ),
        ],
        footer: Some(n(
            "بالهناء والشفاء، شكراً",
            "Bon appétit, merci",
            "Enjoy your meal, thank you",
        )),
    },
    PackTemplate {
        activity: ShopActivity::Parapharmacy,
        release: PackRelease::Mvp,
        modules: &[M::Lots, M::Loyalty, M::GiftVouchers, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("العناية بالوجه", "Soins du visage", "Face care"),
            n("العناية بالجسم", "Soins du corps", "Body care"),
            n("نظافة", "Hygiène", "Hygiene"),
            n("الرضيع والأم", "Bébé et maman", "Baby and mother"),
            n(
                "مكملات غذائية",
                "Compléments alimentaires",
                "Food supplements",
            ),
            n("واقيات الشمس", "Solaires", "Sun care"),
        ],
        services: &[],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Pharmacy,
        release: PackRelease::V2,
        modules: &[M::Pharmacy, M::Lots, M::Loyalty, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("أدوية", "Médicaments", "Medicines"),
            n("شبه صيدلية", "Parapharmacie", "Parapharmacy"),
            n("عتاد طبي", "Matériel médical", "Medical supplies"),
            n("أجهزة تقويم", "Orthopédie", "Orthopaedics"),
            n("الرضيع", "Bébé", "Baby"),
        ],
        services: &[],
        footer: Some(n(
            "احتفظوا بهذه التذكرة",
            "Conservez ce ticket",
            "Keep this receipt",
        )),
    },
    PackTemplate {
        activity: ShopActivity::Hardware,
        release: PackRelease::Mvp,
        modules: &[
            M::Wholesale,
            M::Orders,
            M::DeliveryRounds,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("أدوات", "Outillage", "Tools"),
            n("ترصيص", "Plomberie", "Plumbing"),
            n("كهرباء", "Électricité", "Electrical"),
            n("دهن", "Peinture", "Paint"),
            n(
                "مواد البناء",
                "Matériaux de construction",
                "Building materials",
            ),
            n("خردوات", "Quincaillerie", "Hardware"),
        ],
        services: &[
            TRANSPORT,
            svc("SVC-CUT", n("قص", "Découpe", "Cutting"), 0, true),
        ],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::AutoParts,
        release: PackRelease::Mvp,
        modules: &[M::Wholesale, M::Orders, M::Serials, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("المحرك", "Moteur", "Engine"),
            n("الفرامل", "Freinage", "Brakes"),
            n("المصافي", "Filtration", "Filters"),
            n("كهرباء السيارات", "Électricité auto", "Auto electrics"),
            n("هيكل السيارة", "Carrosserie", "Bodywork"),
            n(
                "زيوت ومزلقات",
                "Huiles et lubrifiants",
                "Oils and lubricants",
            ),
        ],
        services: &[svc(
            "SVC-FITTING",
            n("تركيب", "Montage", "Fitting"),
            0,
            true,
        )],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Clothing,
        release: PackRelease::Mvp,
        modules: &[
            M::VariantGrid,
            M::Loyalty,
            M::GiftVouchers,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("رجال", "Homme", "Men"),
            n("نساء", "Femme", "Women"),
            n("أطفال", "Enfant", "Children"),
            n("أحذية", "Chaussures", "Shoes"),
            n("إكسسوارات", "Accessoires", "Accessories"),
        ],
        services: &[],
        footer: Some(n(
            "الاستبدال خلال 7 أيام بهذه التذكرة",
            "Échange sous 7 jours avec ce ticket",
            "Exchange within 7 days with this receipt",
        )),
    },
    PackTemplate {
        activity: ShopActivity::Phones,
        release: PackRelease::Mvp,
        modules: &[
            M::Serials,
            M::Repairs,
            M::Flexy,
            M::Instalments,
            M::GiftVouchers,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("هواتف", "Téléphones", "Phones"),
            n("إكسسوارات", "Accessoires", "Accessories"),
            n("إعلام آلي", "Informatique", "Computers"),
            n("قطع غيار", "Pièces détachées", "Spare parts"),
            n("بطاقات وتعبئة", "Cartes et recharges", "Cards and top-ups"),
        ],
        services: &[
            svc(
                "SVC-REPAIR",
                n(
                    "يد عاملة للتصليح",
                    "Main d'œuvre réparation",
                    "Repair labour",
                ),
                0,
                true,
            ),
            svc(
                "SVC-DIAG",
                n("تشخيص", "Diagnostic", "Diagnosis"),
                50_000,
                false,
            ),
        ],
        footer: Some(n(
            "الضمان حسب الشهادة المسلمة",
            "Garantie selon le certificat remis",
            "Warranty as per the certificate given",
        )),
    },
    PackTemplate {
        activity: ShopActivity::Cosmetics,
        release: PackRelease::Mvp,
        modules: &[
            M::Lots,
            M::VariantGrid,
            M::Loyalty,
            M::GiftVouchers,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("مكياج", "Maquillage", "Make-up"),
            n("عناية بالبشرة", "Soins", "Skin care"),
            n("عطور", "Parfums", "Perfumes"),
            n("الشعر", "Cheveux", "Hair"),
            n("إكسسوارات", "Accessoires", "Accessories"),
        ],
        services: &[],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Stationery,
        release: PackRelease::V1,
        modules: &[M::DepartmentKeys, M::Orders, M::Loyalty, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("كتب", "Livres", "Books"),
            n("أدوات مدرسية", "Fournitures scolaires", "School supplies"),
            n("قرطاسية", "Papeterie", "Stationery"),
            n("لوازم المكتب", "Bureautique", "Office supplies"),
            n("خدمات", "Services", "Services"),
        ],
        services: &[COPY, PRINT],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Wholesale,
        release: PackRelease::Mvp,
        modules: &[
            M::Wholesale,
            M::DeliveryRounds,
            M::Orders,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Wholesale,
        categories: &[
            n("مواد غذائية", "Alimentation", "Food"),
            n("مشروبات", "Boissons", "Drinks"),
            n("نظافة", "Hygiène", "Hygiene"),
            n("تغليف", "Emballages", "Packaging"),
        ],
        services: &[TRANSPORT, CONSIGN],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Instalments,
        release: PackRelease::V1,
        modules: &[M::Instalments, M::Serials, M::Orders, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("أجهزة كهرومنزلية", "Électroménager", "Home appliances"),
            n("أثاث", "Meubles", "Furniture"),
            n("هواتف", "Téléphones", "Phones"),
            n("إعلام آلي", "Informatique", "Computers"),
        ],
        services: &[DELIVERY],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Optics,
        release: PackRelease::V2,
        modules: &[M::Orders, M::Repairs, M::Appointments, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("إطارات", "Montures", "Frames"),
            n("عدسات النظارات", "Verres", "Lenses"),
            n("عدسات لاصقة", "Lentilles de contact", "Contact lenses"),
            n("نظارات شمسية", "Lunettes de soleil", "Sunglasses"),
            n("إكسسوارات", "Accessoires", "Accessories"),
        ],
        services: &[svc(
            "SVC-MOUNTING",
            n("تركيب العدسات", "Montage des verres", "Lens fitting"),
            50_000,
            false,
        )],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Pressing,
        release: PackRelease::V1,
        modules: &[M::Repairs, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("تنظيف جاف", "Nettoyage à sec", "Dry cleaning"),
            n("غسيل", "Lavage", "Washing"),
            n("كي", "Repassage", "Ironing"),
            n("تعديلات", "Retouches", "Alterations"),
        ],
        services: &[
            svc(
                "SVC-DRY-SUIT",
                n(
                    "تنظيف جاف لبدلة",
                    "Nettoyage à sec costume",
                    "Suit dry cleaning",
                ),
                80_000,
                false,
            ),
            svc(
                "SVC-WASH-SHIRT",
                n(
                    "غسل وكي قميص",
                    "Lavage et repassage chemise",
                    "Shirt wash and iron",
                ),
                20_000,
                false,
            ),
            svc(
                "SVC-IRON",
                n("كي قطعة", "Repassage pièce", "Ironing per piece"),
                10_000,
                false,
            ),
            svc(
                "SVC-DUVET",
                n("غسل لحاف", "Couette", "Duvet"),
                120_000,
                false,
            ),
        ],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Salon,
        release: PackRelease::V1,
        modules: &[
            M::Appointments,
            M::GiftVouchers,
            M::Loyalty,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("حلاقة وتصفيف", "Coiffure", "Hairdressing"),
            n("عناية تجميلية", "Soins esthétiques", "Beauty care"),
            n("منتجات", "Produits", "Products"),
            n("باقات", "Forfaits", "Packages"),
        ],
        services: &[
            svc(
                "SVC-CUT",
                n("حلاقة رجالية", "Coupe homme", "Men's haircut"),
                40_000,
                false,
            ),
            svc(
                "SVC-BLOWDRY",
                n("تسريح", "Brushing", "Blow-dry"),
                80_000,
                false,
            ),
        ],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Jewellery,
        release: PackRelease::V1,
        modules: &[
            M::Jewellery,
            M::Serials,
            M::Instalments,
            M::Orders,
            M::Staff,
            M::Zakat,
        ],
        till_mode: TillMode::Retail,
        categories: &[
            n("ذهب", "Or", "Gold"),
            n("فضة", "Argent", "Silver"),
            n("ساعات", "Montres", "Watches"),
            n("أحجار كريمة", "Pierres", "Stones"),
            n("تصليح", "Réparations", "Repairs"),
        ],
        services: &[svc(
            "SVC-JEWEL-REPAIR",
            n("تصليح مجوهرات", "Réparation bijou", "Jewellery repair"),
            0,
            true,
        )],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Gym,
        release: PackRelease::V2,
        modules: &[M::Loyalty, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("اشتراكات", "Abonnements", "Memberships"),
            n("حصص", "Séances", "Sessions"),
            n("مكملات", "Compléments", "Supplements"),
            n("مشروبات", "Boissons", "Drinks"),
        ],
        services: &[svc(
            "SVC-SESSION",
            n("حصة", "Séance", "Session"),
            30_000,
            false,
        )],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Kiosk,
        release: PackRelease::Mvp,
        modules: &[M::DepartmentKeys, M::Flexy, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("تبغ وجرائد", "Tabac et journaux", "Tobacco and papers"),
            n("تعبئة وبطاقات", "Recharges et cartes", "Top-ups and cards"),
            n("حلويات", "Confiserie", "Confectionery"),
            n("مشروبات", "Boissons", "Drinks"),
            n("خدمات", "Services", "Services"),
        ],
        services: &[COPY, PRINT],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Containers,
        release: PackRelease::V1,
        modules: &[M::Wholesale, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("غاز البوتان", "Gaz butane", "Butane gas"),
            n("مشروبات", "Boissons", "Drinks"),
            n(
                "الأغلفة المسترجعة",
                "Emballages consignés",
                "Returnable containers",
            ),
        ],
        services: &[CONSIGN],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Importer,
        release: PackRelease::V1,
        modules: &[M::Wholesale, M::Orders, M::Staff, M::Zakat],
        till_mode: TillMode::Wholesale,
        categories: &[
            n("سلع مستوردة", "Marchandises importées", "Imported goods"),
            n("قطع", "Pièces", "Parts"),
            n("تغليف", "Emballages", "Packaging"),
        ],
        services: &[TRANSPORT],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::VanSales,
        release: PackRelease::V2,
        modules: &[M::Wholesale, M::DeliveryRounds, M::Staff, M::Zakat],
        till_mode: TillMode::Wholesale,
        categories: &[
            n("مواد غذائية", "Alimentation", "Food"),
            n("مشروبات", "Boissons", "Drinks"),
            n("نظافة", "Hygiène", "Hygiene"),
        ],
        services: &[TRANSPORT],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Workshop,
        release: PackRelease::V1,
        modules: &[M::Orders, M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[
            n("مواد أولية", "Matières premières", "Raw materials"),
            n("منتجات تامة الصنع", "Produits finis", "Finished goods"),
            n("خدمات", "Services", "Services"),
        ],
        services: &[LABOUR],
        footer: None,
    },
    PackTemplate {
        activity: ShopActivity::Other,
        release: PackRelease::Mvp,
        modules: &[M::Staff, M::Zakat],
        till_mode: TillMode::Retail,
        categories: &[n("عام", "Général", "General")],
        services: &[],
        footer: None,
    },
];

/// The template of an activity; `TEMPLATES` holds exactly one per activity (tested).
pub fn template(activity: ShopActivity) -> &'static PackTemplate {
    TEMPLATES
        .iter()
        .find(|t| t.activity == activity)
        .unwrap_or(&TEMPLATES[TEMPLATES.len() - 1])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalName {
    pub ar: String,
    pub fr: String,
    pub en: String,
}

impl From<&SeedName> for LocalName {
    fn from(n: &SeedName) -> LocalName {
        LocalName {
            ar: n.ar.to_owned(),
            fr: n.fr.to_owned(),
            en: n.en.to_owned(),
        }
    }
}

/// What the setup wizard and the settings show about a template before it is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub activity: ShopActivity,
    pub release: PackRelease,
    pub modules: Vec<Module>,
    pub till_mode: TillMode,
    pub categories: Vec<LocalName>,
    pub services: Vec<LocalName>,
}

pub fn infos() -> Vec<TemplateInfo> {
    TEMPLATES
        .iter()
        .map(|t| TemplateInfo {
            activity: t.activity,
            release: t.release,
            modules: t.modules.to_vec(),
            till_mode: t.till_mode,
            categories: t.categories.iter().map(LocalName::from).collect(),
            services: t
                .services
                .iter()
                .map(|s| LocalName::from(&s.name))
                .collect(),
        })
        .collect()
}

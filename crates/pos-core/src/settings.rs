//! Typed key/value settings stored as JSON (spec §5.1 `settings`). Unknown keys are refused.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::company::ShopActivity;
use crate::error::AppError;
use crate::lang::Lang;
use crate::tax::timbre::StampMode;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// A known setting: who may change it, its default and its validator.
/// `permission: None` means any user (or the login screen) may change it.
#[derive(Debug, Clone, Copy)]
pub struct SettingDef {
    pub key: &'static str,
    pub permission: Option<Permission>,
    pub default: fn() -> Value,
    pub validate: fn(&Value) -> bool,
}

pub const REGISTRY: &[SettingDef] = &[
    SettingDef {
        key: "ui.theme",
        permission: None,
        default: || json!("system"),
        validate: |v| matches!(v.as_str(), Some("system" | "light" | "dark")),
    },
    SettingDef {
        key: "ui.login_lang",
        permission: None,
        default: || json!("fr"),
        validate: |v| v.as_str().and_then(Lang::parse).is_some(),
    },
    SettingDef {
        key: "security.auto_lock_minutes",
        permission: Some(Permission::SettingsUsers),
        default: || json!(0),
        validate: |v| v.as_i64().is_some_and(|m| (0..=240).contains(&m)),
    },
    SettingDef {
        key: "shop.activity",
        permission: Some(Permission::SettingsCompany),
        default: || json!("general"),
        validate: |v| v.as_str().and_then(ShopActivity::parse).is_some(),
    },
    // M8: the modules a shop uses and its till mode; `null` follows the activity template.
    SettingDef {
        key: "packs.modules",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| {
            v.is_null()
                || v.as_array().is_some_and(|items| {
                    let mut seen = std::collections::BTreeSet::new();
                    items.len() <= crate::packs::Module::ALL.len()
                        && items.iter().all(|m| {
                            m.as_str().and_then(crate::packs::Module::parse).is_some()
                                && seen.insert(m.as_str().unwrap_or_default().to_owned())
                        })
                })
        },
    },
    SettingDef {
        key: "packs.till_mode",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| {
            v.is_null()
                || matches!(
                    v.as_str(),
                    Some("retail" | "restaurant" | "wholesale" | "mixed")
                )
        },
    },
    // M8: a regulated staple sold above its legal ceiling warns or is refused (SAL-030).
    SettingDef {
        key: "sales.regulated_policy",
        permission: Some(Permission::SettingsCompany),
        default: || json!("warn"),
        validate: |v| matches!(v.as_str(), Some("warn" | "block")),
    },
    // M8 wholesale: a line below its product's minimum order warns or is refused.
    SettingDef {
        key: "wholesale.min_order_policy",
        permission: Some(Permission::SettingsCompany),
        default: || json!("warn"),
        validate: |v| matches!(v.as_str(), Some("warn" | "block")),
    },
    // M8 wholesale (CFG-003): delivery notes standing as invoices, only with the decision's
    // number and date; `null` = off.
    SettingDef {
        key: "documents.bl_as_invoice",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: bl_as_invoice_ok,
    },
    // M8: predefined tickets of the restaurant's counter (RST-003).
    SettingDef {
        key: "restaurant.predefined_tickets",
        permission: Some(Permission::SettingsCompany),
        default: || json!(["Comptoir", "Terrasse"]),
        validate: |v| {
            v.as_array().is_some_and(|items| {
                items.len() <= 30
                    && items.iter().all(|l| {
                        l.as_str()
                            .is_some_and(|s| (1..=40).contains(&s.trim().chars().count()))
                    })
            })
        },
    },
    // M8: the language kitchen tickets are printed in (RST-006).
    SettingDef {
        key: "restaurant.kitchen_lang",
        permission: Some(Permission::SettingsCompany),
        default: || json!("fr"),
        validate: |v| v.as_str().and_then(Lang::parse).is_some(),
    },
    // M8: unsent lines go to the kitchen when the table ticket is paid.
    SettingDef {
        key: "restaurant.send_on_pay",
        permission: Some(Permission::SettingsCompany),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    // A service charge per cover is off until the owner names the product sold per cover.
    SettingDef {
        key: "restaurant.cover_product_id",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| v.is_null() || v.as_str().is_some_and(crate::ids::is_valid_id),
    },
    // M8 flexy (VRT-002): the service product and the commission of each operator.
    SettingDef {
        key: "flexy.operators",
        permission: Some(Permission::SettingsCompany),
        default: || json!([]),
        validate: |v| {
            v.as_array().is_some_and(|items| {
                let mut codes = std::collections::BTreeSet::new();
                items.len() <= 10
                    && items.iter().all(|o| {
                        o.get("code")
                            .and_then(Value::as_str)
                            .is_some_and(|c| !c.is_empty() && codes.insert(c.to_owned()))
                            && o.get("productId")
                                .and_then(Value::as_str)
                                .is_some_and(crate::ids::is_valid_id)
                            && o.get("commissionBp")
                                .and_then(Value::as_i64)
                                .is_some_and(|b| (0..=2_000).contains(&b))
                    })
            })
        },
    },
    // M8 repairs (ORD-005): the "ready" text, the intake slip's conditions, the uncollected days.
    SettingDef {
        key: "repairs.ready_message",
        permission: Some(Permission::SettingsCompany),
        default: || {
            json!({
                "ar": "{shop}: جهازكم {item} (الوصل {number}) جاهز. المبلغ المتبقي: {amount} دج.",
                "fr": "{shop} : votre {item} (bon {number}) est prêt. Reste à payer : {amount} DA.",
                "en": "{shop}: your {item} (ticket {number}) is ready. Left to pay: {amount} DA."
            })
        },
        validate: |v| crate::billing::settings::trilingual_ok(v, 300),
    },
    SettingDef {
        key: "repairs.conditions",
        permission: Some(Permission::SettingsCompany),
        default: || {
            json!({
                "ar": "كل جهاز لا يُستلم خلال 90 يوماً بعد الإشعار بجاهزيته يصبح تحت مسؤولية صاحبه.",
                "fr": "Tout appareil non retiré dans les 90 jours après l'avis « prêt » reste sous la responsabilité de son propriétaire.",
                "en": "A device not collected within 90 days of the ready notice remains its owner's responsibility."
            })
        },
        validate: |v| crate::billing::settings::trilingual_ok(v, 600),
    },
    SettingDef {
        key: "repairs.uncollected_days",
        permission: Some(Permission::SettingsCompany),
        default: || json!(30),
        validate: |v| v.as_i64().is_some_and(|d| (1..=365).contains(&d)),
    },
    // M8 instalment sales (TMP-002, TMP-007): the longest schedule, how many days ahead a due is
    // listed for a reminder, and the reminder text to copy.
    SettingDef {
        key: "instalments.max_count",
        permission: Some(Permission::SettingsCompany),
        default: || json!(24),
        validate: |v| v.as_i64().is_some_and(|n| (1..=60).contains(&n)),
    },
    SettingDef {
        key: "instalments.reminder_days",
        permission: Some(Permission::SettingsCompany),
        default: || json!(3),
        validate: |v| v.as_i64().is_some_and(|n| (0..=30).contains(&n)),
    },
    SettingDef {
        key: "instalments.reminder_message",
        permission: Some(Permission::SettingsCompany),
        default: || {
            json!({
                "ar": "{shop}: نذكّركم بالقسط {amount} دج من العقد {number} المستحق في {date}.",
                "fr": "{shop} : rappel de l'échéance de {amount} DA du contrat {number}, due le {date}.",
                "en": "{shop}: reminder of the {amount} DA instalment of contract {number}, due on {date}."
            })
        },
        validate: |v| crate::billing::settings::trilingual_ok(v, 300),
    },
    // M8 customer orders (ORD-003): the least deposit of a layaway in basis points of its total,
    // and the "Arrhes conservées" service product `orders::kept_deposit_product` creates.
    SettingDef {
        key: "orders.layaway_min_deposit_bp",
        permission: Some(Permission::SettingsCompany),
        default: || json!(2_000),
        validate: |v| v.as_i64().is_some_and(|b| (0..=10_000).contains(&b)),
    },
    SettingDef {
        key: "orders.kept_deposit_product_id",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| v.is_null() || v.as_str().is_some_and(|id| id.len() == 36),
    },
    // M8 loyalty (LOY-002 lite): points per whole 100 DA paid, what a point is worth in a loyalty
    // voucher (centimes), the least points turned into a voucher, how long earned points last
    // (months, 0 = never expire); and how long a gift, exchange or loyalty voucher lasts.
    SettingDef {
        key: "loyalty.points_per_100_da",
        permission: Some(Permission::SettingsCompany),
        default: || json!(1),
        validate: |v| v.as_i64().is_some_and(|n| (0..=100).contains(&n)),
    },
    SettingDef {
        key: "loyalty.point_value",
        permission: Some(Permission::SettingsCompany),
        default: || json!(100),
        validate: |v| v.as_i64().is_some_and(|n| (1..=10_000).contains(&n)),
    },
    SettingDef {
        key: "loyalty.min_redeem_points",
        permission: Some(Permission::SettingsCompany),
        default: || json!(500),
        validate: |v| v.as_i64().is_some_and(|n| (1..=1_000_000).contains(&n)),
    },
    SettingDef {
        key: "loyalty.points_validity_months",
        permission: Some(Permission::SettingsCompany),
        default: || json!(12),
        validate: |v| v.as_i64().is_some_and(|n| (0..=60).contains(&n)),
    },
    SettingDef {
        key: "vouchers.validity_months",
        permission: Some(Permission::SettingsCompany),
        default: || json!(12),
        validate: |v| v.as_i64().is_some_and(|n| (1..=60).contains(&n)),
    },
    // M8 jewellery: the trade authorisation printed on the jeweller register's header, and the
    // most a precious-metal ticket may take in cash (centimes, VRT-144); `null` = none set.
    SettingDef {
        key: "jewellery.authorisation",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: jeweller_authorisation_ok,
    },
    SettingDef {
        key: "jewellery.max_cash",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| v.is_null() || v.as_i64().is_some_and(|n| n > 0),
    },
    // Zakat: the nisab of the current Hijri year as the shop typed it (centimes, the figure the
    // Ministry of Religious Affairs publishes), `null` until entered: the software never fills
    // it in. Its source is the shop's own short note ("MARW, 1447").
    SettingDef {
        key: "zakat.nisab",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| v.is_null() || v.as_i64().is_some_and(|n| n > 0),
    },
    SettingDef {
        key: "zakat.nisab_source",
        permission: Some(Permission::SettingsCompany),
        default: || json!(""),
        validate: |v| {
            v.as_str()
                .is_some_and(|s| s.chars().count() <= 120 && !s.chars().any(char::is_control))
        },
    },
    // Frozen once a document is numbered (`set_value`).
    SettingDef {
        key: "numbering.restart_yearly",
        permission: Some(Permission::SettingsCompany),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    // M1: droit de timbre handling (spec §6).
    SettingDef {
        key: "stamp_duty.mode",
        permission: Some(Permission::SettingsCompany),
        default: || json!("charge_customer"),
        validate: |v| v.as_str().and_then(StampMode::parse).is_some(),
    },
    // M1: selling below zero stock, STK-017 (per product `allow_negative_stock` always allows).
    SettingDef {
        key: "stock.negative_policy",
        permission: Some(Permission::SettingsCompany),
        default: || json!("warn"),
        validate: |v| matches!(v.as_str(), Some("allow" | "warn" | "block")),
    },
    // M1: round all-cash totals to 5 or 10 DA (PAY-016); 0 = off. Centimes.
    SettingDef {
        key: "sales.cash_rounding",
        permission: Some(Permission::SettingsCompany),
        default: || json!(0),
        validate: |v| matches!(v.as_i64(), Some(0 | 500 | 1_000)),
    },
    // M1: a cash variance above this (centimes) needs a manager (CSH-005).
    SettingDef {
        key: "cash.variance_threshold",
        permission: Some(Permission::SettingsCompany),
        default: || json!(50_000),
        validate: |v| v.as_i64().is_some_and(|c| (0..=100_000_000).contains(&c)),
    },
    // M1: blind close, the cashier counts without seeing the expected cash (CSH-004).
    SettingDef {
        key: "cash.blind_close",
        permission: Some(Permission::SettingsCompany),
        default: || json!(false),
        validate: Value::is_boolean,
    },
    // M2: change limit (PAY-017), cheque customer (PAY-020), expired lots (PRD-053), soldes
    // windows (PRM-021), internal EAN prefix (PRD-003), NIN and social links (CFG-001/004).
    SettingDef {
        key: "sales.max_change",
        permission: Some(Permission::SettingsCompany),
        default: || json!(0),
        validate: |v| v.as_i64().is_some_and(|m| (0..=100_000_000).contains(&m)),
    },
    SettingDef {
        key: "payments.cheque_requires_customer",
        permission: Some(Permission::SettingsCompany),
        default: || json!(false),
        validate: Value::is_boolean,
    },
    // M8 (HW-042): the text of the shop's BaridiMob payment QR shown on the customer screen;
    // `null` = none set.
    SettingDef {
        key: "payments.baridpay_qr",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: |v| {
            v.is_null()
                || v.as_str().is_some_and(|s| {
                    (10..=512).contains(&s.chars().count()) && !s.chars().any(char::is_control)
                })
        },
    },
    // How the till takes payment (Settings › Payments): "ask" opens the payment dialog with the
    // allowed methods as tiles; "fixed" finishes the sale at once in the one method, which never
    // needs details nor a customer (cash, card, Edahabia, BaridiMob, or `type:<id>`, a custom
    // payment type riding one of them).
    SettingDef {
        key: "payments.till_mode",
        permission: Some(Permission::SettingsCompany),
        default: || json!("ask"),
        validate: |v| matches!(v.as_str(), Some("ask" | "fixed")),
    },
    SettingDef {
        key: "payments.fixed_method",
        permission: Some(Permission::SettingsCompany),
        default: || json!("cash"),
        validate: fixed_method_ok,
    },
    SettingDef {
        key: "payments.allowed_methods",
        permission: Some(Permission::SettingsCompany),
        default: || {
            json!([
                "cash",
                "card_cib",
                "edahabia",
                "baridimob",
                "cheque",
                "transfer",
                "credit",
                "voucher"
            ])
        },
        validate: allowed_methods_ok,
    },
    SettingDef {
        key: "stock.expired_lots",
        permission: Some(Permission::SettingsCompany),
        default: || json!("warn"),
        validate: |v| matches!(v.as_str(), Some("warn" | "block")),
    },
    SettingDef {
        key: "stock.expiry_alert_days",
        permission: Some(Permission::SettingsCompany),
        default: || json!(30),
        validate: |v| v.as_i64().is_some_and(|d| (0..=365).contains(&d)),
    },
    SettingDef {
        key: "promotions.soldes_windows",
        permission: Some(Permission::SettingsCompany),
        default: || json!([]),
        validate: is_soldes_windows,
    },
    SettingDef {
        key: "products.internal_ean_prefix",
        permission: Some(Permission::SettingsCompany),
        default: || json!("200"),
        validate: |v| {
            v.as_str().is_some_and(|p| {
                p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()) && p.starts_with('2')
            })
        },
    },
    SettingDef {
        key: "company.nin",
        permission: Some(Permission::SettingsCompany),
        default: || json!(""),
        validate: |v| {
            v.as_str().is_some_and(|n| {
                n.is_empty() || (n.len() == 18 && n.bytes().all(|b| b.is_ascii_digit()))
            })
        },
    },
    SettingDef {
        key: "company.social",
        permission: Some(Permission::SettingsCompany),
        default: || json!({}),
        validate: is_social,
    },
    // M3: fiscal identity the company row lacks (décret 05-468 art. 3; loi 22-23).
    SettingDef {
        key: "company.activity",
        permission: Some(Permission::SettingsCompany),
        default: || json!({ "ar": "", "fr": "", "en": "" }),
        validate: |v| {
            crate::billing::settings::trilingual_ok(v, crate::billing::settings::ACTIVITY_MAX)
        },
    },
    SettingDef {
        key: "company.ae_card",
        permission: Some(Permission::SettingsCompany),
        default: || json!(""),
        validate: |v| {
            v.as_str()
                .is_some_and(|s| s.chars().count() <= 40 && !s.chars().any(char::is_control))
        },
    },
    // M3: A4/A5 documents (spec §9.2).
    SettingDef {
        key: "documents.layout",
        permission: Some(Permission::SettingsCompany),
        default: || {
            json!({ "accent": "brand", "showLogo": true, "paper": "a4", "secondLang": null,
                    "showDeliveryPrices": true, "showStampBox": true })
        },
        validate: crate::billing::settings::layout_ok,
    },
    SettingDef {
        key: "documents.texts",
        permission: Some(Permission::SettingsCompany),
        default: || {
            let empty = json!({ "ar": "", "fr": "", "en": "" });
            json!({ "header": empty, "footer": empty, "terms": empty, "warranty": empty })
        },
        validate: crate::billing::settings::texts_ok,
    },
    SettingDef {
        key: "documents.quote_validity_days",
        permission: Some(Permission::SettingsCompany),
        default: || json!(30),
        validate: |v| v.as_i64().is_some_and(|d| (1..=365).contains(&d)),
    },
    SettingDef {
        key: "documents.invoice_due_days",
        permission: Some(Permission::SettingsCompany),
        default: || json!(30),
        validate: |v| v.as_i64().is_some_and(|d| (0..=365).contains(&d)),
    },
    // M3: DOC-034 (décret 13-378 art. 7, consumer information essentially in Arabic).
    SettingDef {
        key: "documents.arabic_guard",
        permission: Some(Permission::SettingsCompany),
        default: || json!("consumer"),
        validate: |v| matches!(v.as_str(), Some("off" | "consumer" | "categories")),
    },
    SettingDef {
        key: "documents.arabic_guard_categories",
        permission: Some(Permission::SettingsCompany),
        default: || json!([]),
        validate: |v| {
            v.as_array().is_some_and(|a| {
                a.len() <= 200
                    && a.iter()
                        .all(|id| id.as_str().is_some_and(crate::ids::is_valid_id))
            })
        },
    },
    // M4: weight and price barcode layouts (spec §8, HW-030); null = the data pack's layouts.
    SettingDef {
        key: crate::catalog::scale_codes::SETTING,
        permission: Some(Permission::SettingsHardware),
        default: || Value::Null,
        validate: crate::catalog::scale_codes::valid_setting,
    },
    // M4: label templates (LBL-001), one per category, and the DOC-034 Arabic guard (`null` =
    // every category when the shop sells food or regulated goods).
    SettingDef {
        key: "labels.templates",
        permission: Some(Permission::ProductsLabels),
        default: || serde_json::to_value(crate::labels::builtin_templates()).unwrap_or(Value::Null),
        validate: crate::labels::template::valid_templates,
    },
    SettingDef {
        key: "labels.category_templates",
        permission: Some(Permission::ProductsLabels),
        default: || json!({}),
        validate: crate::labels::template::valid_category_templates,
    },
    SettingDef {
        key: "labels.arabic_categories",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: crate::labels::template::valid_arabic_categories,
    },
    // M7: DZBuild link (spec §4.7). Every switch is off-network by default where data leaves.
    SettingDef {
        key: "cloud.sync.enabled",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.publish.mode",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!("selected"),
        validate: |v| matches!(v.as_str(), Some("selected" | "all")),
    },
    SettingDef {
        key: "cloud.images.push",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.stock.push",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    // A price changed on DZBuild becomes the till's price only when the owner says so: the
    // shop's prices are set at the shop.
    SettingDef {
        key: "cloud.prices.pull",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(false),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.stock.pull",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.sales.push",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.orders.intake",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.orders.document",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!("ticket"),
        validate: |v| matches!(v.as_str(), Some("ticket" | "delivery_note")),
    },
    SettingDef {
        key: "cloud.orders.retention_days",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(90),
        validate: |v| v.as_i64().is_some_and(|d| (30..=365).contains(&d)),
    },
    // loi 18-07: customer phones and names leave the PC only when the owner turns this on.
    SettingDef {
        key: "cloud.customers.sync",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(false),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.customers.flags",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "cloud.receipt_qr",
        permission: Some(Permission::SettingsDzbuild),
        default: || json!(false),
        validate: Value::is_boolean,
    },
    // M7: the linked store's public address, written by the main PC with every change of its
    // link (`cloud::store`), so the tills print the "Commandez en ligne" QR: a shared setting
    // replicates, the main PC's `cloud_links` does not.
    SettingDef {
        key: "cloud.store_public_url",
        permission: Some(Permission::SettingsDzbuild),
        default: || Value::Null,
        validate: |v| v.is_null() || v.as_str().is_some_and(|u| u.starts_with("https://")),
    },
    // M6: backup retention (DAT-007) and the backup when the app closes (DAT-024).
    SettingDef {
        key: "backup.keep_daily",
        permission: Some(Permission::SettingsData),
        default: || json!(14),
        validate: |v| v.as_i64().is_some_and(|n| (7..=60).contains(&n)),
    },
    SettingDef {
        key: "backup.keep_weekly",
        permission: Some(Permission::SettingsData),
        default: || json!(8),
        validate: |v| v.as_i64().is_some_and(|n| (0..=52).contains(&n)),
    },
    SettingDef {
        key: "backup.keep_monthly",
        permission: Some(Permission::SettingsData),
        default: || json!(12),
        validate: |v| v.as_i64().is_some_and(|n| (0..=36).contains(&n)),
    },
    SettingDef {
        key: "backup.on_close",
        permission: Some(Permission::SettingsData),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    // M8 Task 17 (DAT-007, hourly part): today's automatic copies kept while a cash session is
    // open on the PC; 0 = no hourly copies.
    SettingDef {
        key: "backup.keep_hourly",
        permission: Some(Permission::SettingsData),
        default: || json!(24),
        validate: |v| v.as_i64().is_some_and(|n| (0..=48).contains(&n)),
    },
    // M8 Task 17 (CFG-008): the hour the business day turns (0 = midnight; a café open past
    // midnight books its 01:30 sales on the day before).
    SettingDef {
        key: "sales.day_cutoff_hour",
        permission: Some(Permission::SettingsCompany),
        default: || json!(0),
        validate: |v| v.as_i64().is_some_and(|h| (0..=6).contains(&h)),
    },
    // M8 Task 17 (UX-017): the owner's till keys, `{ action: key }` over `till::DEFAULT_KEYS`.
    SettingDef {
        key: "till.key_map",
        permission: Some(Permission::SettingsCompany),
        default: || json!({}),
        validate: crate::till::key_map_ok,
    },
    // M8 Task 17 (PDP-005): the shop's data protection officer, printed on the processing
    // register; `null` = none named.
    SettingDef {
        key: "privacy.dpo",
        permission: Some(Permission::SettingsCompany),
        default: || Value::Null,
        validate: dpo_ok,
    },
    // M6: updates (PLT-010): the channel and whether checks and downloads run by themselves.
    SettingDef {
        key: "updates.channel",
        permission: Some(Permission::SettingsData),
        default: || json!("stable"),
        validate: |v| {
            v.as_str()
                .and_then(crate::updates::Channel::parse)
                .is_some()
        },
    },
    SettingDef {
        key: "updates.auto_check",
        permission: Some(Permission::SettingsData),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    SettingDef {
        key: "updates.auto_download",
        permission: Some(Permission::SettingsData),
        default: || json!(true),
        validate: Value::is_boolean,
    },
    // M6: who the "Contacter l'assistance" button reaches (ONB-012): the shop's reseller or
    // DZBuild's page. Opened by Rust in the system browser, never by the webview.
    SettingDef {
        key: "support.contact",
        permission: Some(Permission::SettingsCompany),
        default: || json!({ "url": "https://dzbuild.com/pos/support" }),
        validate: is_support_contact,
    },
];

/// `[{ "from": "YYYY-MM-DD", "to": "YYYY-MM-DD" }]`, at most 12 windows, each from ≤ to
/// (PRM-021: the wali's soldes dates).
fn is_soldes_windows(v: &Value) -> bool {
    let day = |w: &Value, key: &str| {
        w.get(key)
            .and_then(Value::as_str)
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    };
    v.as_array().is_some_and(|items| {
        items.len() <= 12
            && items.iter().all(|w| match (day(w, "from"), day(w, "to")) {
                (Some(from), Some(to)) => from <= to,
                _ => false,
            })
    })
}

/// The till's one payment method: cash, CIB card, Edahabia or BaridiMob, or `type:<id>` (a custom
/// payment type, which the till checks is still live and rides one of those).
fn fixed_method_ok(v: &Value) -> bool {
    match v.as_str() {
        Some("cash" | "card_cib" | "edahabia" | "baridimob") => true,
        Some(s) => s.strip_prefix("type:").is_some_and(|id| {
            (1..=64).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }),
        None => false,
    }
}

/// The tenders the payment dialog offers: at least one, each a known tender once (never the
/// change, which no one pays with).
fn allowed_methods_ok(v: &Value) -> bool {
    v.as_array().is_some_and(|items| {
        let mut seen = std::collections::BTreeSet::new();
        !items.is_empty()
            && items.iter().all(|m| {
                m.as_str()
                    .and_then(crate::payments::PaymentMethod::parse)
                    .is_some_and(|method| {
                        method != crate::payments::PaymentMethod::Change && seen.insert(method)
                    })
            })
    })
}

/// `null`, or exactly `{ "number": 1–40 characters, "date": "YYYY-MM-DD" }` (CFG-003).
fn bl_as_invoice_ok(v: &Value) -> bool {
    if v.is_null() {
        return true;
    }
    v.as_object().is_some_and(|map| {
        let number = map.get("number").and_then(Value::as_str).map(str::trim);
        let date = map.get("date").and_then(Value::as_str);
        map.len() == 2
            && number.is_some_and(|n| {
                (1..=40).contains(&n.chars().count()) && !n.chars().any(char::is_control)
            })
            && date.is_some_and(|d| {
                d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
            })
    })
}

/// `null`, or exactly `{ "number": 1–40 characters, "date": "YYYY-MM-DD", "authority": 1–80
/// characters }`: the jeweller's authorisation on the register's header.
fn jeweller_authorisation_ok(v: &Value) -> bool {
    if v.is_null() {
        return true;
    }
    let text = |map: &serde_json::Map<String, Value>, key: &str, max: usize| {
        map.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|s| {
                (1..=max).contains(&s.chars().count()) && !s.chars().any(char::is_control)
            })
    };
    v.as_object().is_some_and(|map| {
        let date = map.get("date").and_then(Value::as_str);
        map.len() == 3
            && text(map, "number", 40)
            && text(map, "authority", 80)
            && date.is_some_and(|d| {
                d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
            })
    })
}

/// `null`, or `{ name: 1–120 characters, email?: ≤ 120 with an @, phone?: ≤ 20 }`.
fn dpo_ok(v: &Value) -> bool {
    if v.is_null() {
        return true;
    }
    let text = |s: &str, max: usize| {
        let s = s.trim();
        (1..=max).contains(&s.chars().count()) && !s.chars().any(char::is_control)
    };
    v.as_object().is_some_and(|map| {
        map.get("name")
            .and_then(Value::as_str)
            .is_some_and(|n| text(n, 120))
            && map.iter().all(|(k, v)| match (k.as_str(), v) {
                ("name", _) => true,
                ("email", Value::String(e)) => text(e, 120) && e.contains('@'),
                ("phone", Value::String(p)) => text(p, 20),
                ("email" | "phone", Value::Null) => true,
                _ => false,
            })
    })
}

/// `{ facebook?, instagram?, tiktok?, website? }`, strings of at most 120 characters.
fn is_social(v: &Value) -> bool {
    v.as_object().is_some_and(|map| {
        map.iter().all(|(k, v)| {
            matches!(k.as_str(), "facebook" | "instagram" | "tiktok" | "website")
                && v.as_str().is_some_and(|s| s.chars().count() <= 120)
        })
    })
}

/// `{ whatsapp?: 9–15 digits, url?: https://… (≤ 200, no spaces), label?: ≤ 60 characters }`.
fn is_support_contact(v: &Value) -> bool {
    v.as_object().is_some_and(|map| {
        map.iter().all(|(k, v)| match (k.as_str(), v.as_str()) {
            ("whatsapp", Some(s)) => {
                (9..=15).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
            }
            ("url", Some(s)) => {
                s.starts_with("https://") && s.len() <= 200 && !s.contains(char::is_whitespace)
            }
            ("label", Some(s)) => s.chars().count() <= 60,
            _ => false,
        })
    })
}

pub fn def(key: &str) -> Result<&'static SettingDef, AppError> {
    REGISTRY
        .iter()
        .find(|d| d.key == key)
        .ok_or_else(|| AppError::validation("key", "setting_unknown"))
}

/// Whether `key` holds a stored value (set by the owner or a template), not its default.
pub(crate) fn is_stored(conn: &Connection, key: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM settings WHERE key = ?1)",
        [key],
        |r| r.get(0),
    )?)
}

/// Stored value, or the default when unset or no longer valid.
pub fn get_value(conn: &Connection, key: &str) -> Result<Value, AppError> {
    let d = def(key)?;
    let stored: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(text) = stored else {
        return Ok((d.default)());
    };
    let value: Value = serde_json::from_str(&text)?;
    if (d.validate)(&value) {
        Ok(value)
    } else {
        tracing::warn!(key, "stored setting is no longer valid; using its default");
        Ok((d.default)())
    }
}

pub fn get<T: DeserializeOwned>(conn: &Connection, key: &str) -> Result<T, AppError> {
    Ok(serde_json::from_value(get_value(conn, key)?)?)
}

/// Validates and stores `value`, and audits the change as `{ before, after }` (`before` is the
/// value in effect until now, the default when the key was unset).
pub fn set_value(
    tx: &Transaction<'_>,
    actor: &Actor,
    key: &str,
    value: Value,
) -> Result<Value, AppError> {
    let d = def(key)?;
    // The scale layouts are edited as a table: the error names the row and field at fault.
    if key == crate::catalog::scale_codes::SETTING {
        crate::catalog::scale_codes::check_setting(&value)?;
    }
    if !(d.validate)(&value) {
        return Err(AppError::validation("value", "value_invalid"));
    }
    let before = get_value(tx, key)?;
    if key == "numbering.restart_yearly" && value != before && numbering_started(tx)? {
        return Err(AppError::validation("value", "value_invalid"));
    }
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value.to_string(), now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "settings.updated",
            "setting",
            Some(key),
            json!({ "before": before, "after": value }),
        ),
    )?;
    Ok(value)
}

/// Validates and stores `value` without an audit row: the caller's operation journals the
/// change once (a pack template writes several keys).
pub(crate) fn set_quiet(tx: &Transaction<'_>, key: &str, value: Value) -> Result<Value, AppError> {
    let d = def(key)?;
    if !(d.validate)(&value) {
        return Err(AppError::validation("value", "value_invalid"));
    }
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value.to_string(), now_ms()],
    )?;
    Ok(value)
}

/// `numbering.restart_yearly` picks the counter key (fiscal year or 0) while the printed number
/// always carries the year, so changing it once documents are numbered would hand out numbers
/// already issued (spec §5.2). It is frozen as soon as a document counter exists; the Z counter
/// never restarts, so it does not count.
fn numbering_started(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM counters WHERE doc_type <> ?1)",
        [crate::numbering::Z_COUNTER],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    #[test]
    fn unset_keys_return_their_defaults() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.read(|c| get::<String>(c, "ui.theme")).unwrap(), "system");
        assert_eq!(
            db.read(|c| get::<Lang>(c, "ui.login_lang")).unwrap(),
            Lang::Fr
        );
        assert_eq!(
            db.read(|c| get::<i64>(c, "security.auto_lock_minutes"))
                .unwrap(),
            0
        );
        assert_eq!(
            db.read(|c| get::<ShopActivity>(c, "shop.activity"))
                .unwrap(),
            ShopActivity::General
        );
        assert!(
            db.read(|c| get::<bool>(c, "numbering.restart_yearly"))
                .unwrap()
        );
    }

    #[test]
    fn set_value_validates_persists_and_audits() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            set_value(
                tx,
                &Actor::default(),
                "security.auto_lock_minutes",
                json!(10),
            )
        })
        .unwrap();
        assert_eq!(
            db.read(|c| get::<i64>(c, "security.auto_lock_minutes"))
                .unwrap(),
            10
        );
        let bad = db
            .write(|tx| set_value(tx, &Actor::default(), "ui.theme", json!("blue")))
            .unwrap_err();
        assert_eq!(bad.code(), "value_invalid");
        let unknown = db
            .write(|tx| set_value(tx, &Actor::default(), "ui.colour", json!(1)))
            .unwrap_err();
        assert_eq!(unknown.code(), "setting_unknown");
        let audited: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'settings.updated'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(audited, 1);
    }

    #[test]
    fn registry_is_consistent() {
        let keys: std::collections::BTreeSet<&str> = REGISTRY.iter().map(|d| d.key).collect();
        assert_eq!(keys.len(), REGISTRY.len());
        for d in REGISTRY {
            assert!(
                (d.validate)(&(d.default)()),
                "default of {} must be valid",
                d.key
            );
        }
        assert_eq!(def("ui.theme").unwrap().permission, None);
        assert_eq!(
            def("shop.activity").unwrap().permission,
            Some(Permission::SettingsCompany)
        );
    }

    /// The nisab is the shop's own figure (the ministry's for the Hijri year): unset until the
    /// owner types it, then a positive amount; its source is a short free note.
    #[test]
    fn the_zakat_nisab_is_unset_until_the_shop_enters_it() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            db.read(|c| get::<Option<i64>>(c, "zakat.nisab")).unwrap(),
            None
        );
        assert_eq!(
            db.read(|c| get::<String>(c, "zakat.nisab_source")).unwrap(),
            ""
        );
        for d in [
            def("zakat.nisab").unwrap(),
            def("zakat.nisab_source").unwrap(),
        ] {
            assert_eq!(d.permission, Some(Permission::SettingsCompany), "{}", d.key);
        }
        db.write(|tx| {
            set_value(tx, &Actor::default(), "zakat.nisab", json!(229_500_000))?;
            set_value(
                tx,
                &Actor::default(),
                "zakat.nisab_source",
                json!("MARW, 1447"),
            )
        })
        .unwrap();
        assert_eq!(
            db.read(|c| get::<Option<i64>>(c, "zakat.nisab")).unwrap(),
            Some(229_500_000)
        );
        assert_eq!(
            db.read(|c| get::<String>(c, "zakat.nisab_source")).unwrap(),
            "MARW, 1447"
        );
        db.write(|tx| set_value(tx, &Actor::default(), "zakat.nisab", Value::Null))
            .unwrap();
        for (key, bad) in [
            ("zakat.nisab", json!(0)),
            ("zakat.nisab", json!(-1)),
            ("zakat.nisab", json!("229500000")),
            ("zakat.nisab", json!(2_295_000.5)),
            ("zakat.nisab_source", json!("x".repeat(121))),
            ("zakat.nisab_source", json!("MARW\n1447")),
            ("zakat.nisab_source", Value::Null),
        ] {
            let err = db
                .write(move |tx| set_value(tx, &Actor::default(), key, bad))
                .unwrap_err();
            assert_eq!(err.code(), "value_invalid", "{key}");
        }
        db.write(|tx| {
            set_value(
                tx,
                &Actor::default(),
                "zakat.nisab_source",
                json!("م".repeat(120)),
            )
        })
        .unwrap();
    }

    /// How the till takes payment (Settings › Payments): asked at the till by default, with every
    /// method allowed; the one method of the "fixed" mode never needs details nor a customer.
    #[test]
    fn the_till_payment_mode_asks_by_default_and_refuses_bad_lists() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            db.read(|c| get::<String>(c, "payments.till_mode")).unwrap(),
            "ask"
        );
        assert_eq!(
            db.read(|c| get::<String>(c, "payments.fixed_method"))
                .unwrap(),
            "cash"
        );
        assert_eq!(
            db.read(|c| get::<Vec<String>>(c, "payments.allowed_methods"))
                .unwrap(),
            [
                "cash",
                "card_cib",
                "edahabia",
                "baridimob",
                "cheque",
                "transfer",
                "credit",
                "voucher"
            ]
        );
        for key in [
            "payments.till_mode",
            "payments.fixed_method",
            "payments.allowed_methods",
        ] {
            assert_eq!(
                def(key).unwrap().permission,
                Some(Permission::SettingsCompany),
                "{key}"
            );
        }
        for (key, good) in [
            ("payments.till_mode", json!("fixed")),
            ("payments.fixed_method", json!("edahabia")),
            (
                "payments.fixed_method",
                json!("type:0192f0c1-7a2b-7c3d-8e4f-123456789abc"),
            ),
            ("payments.allowed_methods", json!(["card_cib"])),
            (
                "payments.allowed_methods",
                json!(["credit", "cash", "cheque"]),
            ),
        ] {
            db.write(move |tx| set_value(tx, &Actor::default(), key, good))
                .unwrap();
        }
        assert_eq!(
            db.read(|c| get::<Vec<String>>(c, "payments.allowed_methods"))
                .unwrap(),
            ["credit", "cash", "cheque"]
        );
        for (key, bad) in [
            ("payments.till_mode", json!("auto")),
            ("payments.till_mode", Value::Null),
            // cheque, transfer, credit and voucher need details or a customer
            ("payments.fixed_method", json!("cheque")),
            ("payments.fixed_method", json!("transfer")),
            ("payments.fixed_method", json!("credit")),
            ("payments.fixed_method", json!("voucher")),
            ("payments.fixed_method", json!("change")),
            ("payments.fixed_method", json!("type:")),
            ("payments.fixed_method", json!("type:a b")),
            (
                "payments.fixed_method",
                json!(format!("type:{}", "a".repeat(65))),
            ),
            ("payments.fixed_method", json!(1)),
            ("payments.allowed_methods", json!([])),
            ("payments.allowed_methods", json!(["cash", "cash"])),
            ("payments.allowed_methods", json!(["change"])),
            ("payments.allowed_methods", json!(["cash", "bitcoin"])),
            ("payments.allowed_methods", json!(["cash", 1])),
            ("payments.allowed_methods", json!("cash")),
            ("payments.allowed_methods", Value::Null),
        ] {
            let err = db
                .write(move |tx| set_value(tx, &Actor::default(), key, bad))
                .unwrap_err();
            assert_eq!(err.code(), "value_invalid", "{key}");
        }
    }

    #[test]
    fn m1_settings_have_safe_defaults() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            db.read(|c| get::<StampMode>(c, "stamp_duty.mode")).unwrap(),
            StampMode::ChargeCustomer
        );
        assert_eq!(
            db.read(|c| get::<String>(c, "stock.negative_policy"))
                .unwrap(),
            "warn"
        );
        assert_eq!(
            db.read(|c| get::<i64>(c, "sales.cash_rounding")).unwrap(),
            0
        );
        assert_eq!(
            db.read(|c| get::<i64>(c, "cash.variance_threshold"))
                .unwrap(),
            50_000
        );
        assert!(!db.read(|c| get::<bool>(c, "cash.blind_close")).unwrap());
        for (key, bad) in [
            ("stamp_duty.mode", json!("sometimes")),
            ("stock.negative_policy", json!("never")),
            ("sales.cash_rounding", json!(300)),
            ("cash.variance_threshold", json!(-1)),
        ] {
            let err = db
                .write(move |tx| set_value(tx, &Actor::default(), key, bad))
                .unwrap_err();
            assert_eq!(err.code(), "value_invalid", "{key}");
        }
    }

    #[test]
    fn m2_settings_have_safe_defaults_and_validate() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.read(|c| get::<i64>(c, "sales.max_change")).unwrap(), 0);
        assert!(
            !db.read(|c| get::<bool>(c, "payments.cheque_requires_customer"))
                .unwrap()
        );
        assert_eq!(
            db.read(|c| get::<String>(c, "stock.expired_lots")).unwrap(),
            "warn"
        );
        assert_eq!(
            db.read(|c| get::<i64>(c, "stock.expiry_alert_days"))
                .unwrap(),
            30
        );
        assert_eq!(
            db.read(|c| get_value(c, "promotions.soldes_windows"))
                .unwrap(),
            json!([])
        );
        assert_eq!(
            db.read(|c| get::<String>(c, "products.internal_ean_prefix"))
                .unwrap(),
            "200"
        );
        assert_eq!(db.read(|c| get::<String>(c, "company.nin")).unwrap(), "");
        let set = |key: &'static str, value: Value| {
            db.write(move |tx| set_value(tx, &Actor::default(), key, value))
                .map(drop)
        };
        set(
            "promotions.soldes_windows",
            json!([{ "from": "2027-01-10", "to": "2027-02-20" }]),
        )
        .unwrap();
        assert!(
            set(
                "promotions.soldes_windows",
                json!([{ "from": "2027-02-20", "to": "2027-01-10" }])
            )
            .is_err()
        );
        assert!(set("stock.expired_lots", json!("maybe")).is_err());
        assert!(set("products.internal_ean_prefix", json!("613")).is_err());
        set("company.nin", json!("109876543210987654")).unwrap();
        assert!(set("company.nin", json!("12345")).is_err());
        set(
            "company.social",
            json!({ "facebook": "elamel.dz", "website": "https://elamel.dz" }),
        )
        .unwrap();
        assert!(set("company.social", json!({ "myspace": "x" })).is_err());
        assert!(set("sales.max_change", json!(-1)).is_err());
    }

    #[test]
    fn set_value_audits_before_and_after() {
        let db = Db::open_memory().unwrap();
        for minutes in [10, 20] {
            db.write(move |tx| {
                set_value(
                    tx,
                    &Actor::default(),
                    "security.auto_lock_minutes",
                    json!(minutes),
                )
            })
            .unwrap();
        }
        let rows = db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT entity_type, entity_id, payload FROM audit_log ORDER BY seq",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        let rows: Vec<(String, String, Value)> = rows
            .into_iter()
            .map(|(kind, key, payload)| (kind, key, serde_json::from_str(&payload).unwrap()))
            .collect();
        let key = "security.auto_lock_minutes".to_owned();
        assert_eq!(
            rows,
            [
                (
                    "setting".into(),
                    key.clone(),
                    json!({ "before": 0, "after": 10 })
                ),
                ("setting".into(), key, json!({ "before": 10, "after": 20 })),
            ]
        );
    }

    #[test]
    fn dzbuild_settings_are_private_and_quiet_by_default() {
        let db = Db::open_memory().unwrap();
        let get = |key: &'static str| db.read(move |c| get_value(c, key)).unwrap();
        assert_eq!(get("cloud.sync.enabled"), json!(true));
        assert_eq!(
            get("cloud.publish.mode"),
            json!("selected"),
            "nothing is published unasked"
        );
        assert_eq!(
            get("cloud.customers.sync"),
            json!(false),
            "loi 18-07: personal data stays unless the owner says so"
        );
        assert_eq!(get("cloud.receipt_qr"), json!(false));
        assert_eq!(
            get("cloud.prices.pull"),
            json!(false),
            "the shop's prices stay the shop's"
        );
        assert_eq!(get("cloud.orders.document"), json!("ticket"));
        assert_eq!(get("cloud.orders.retention_days"), json!(90));
        for key in [
            "cloud.sync.enabled",
            "cloud.publish.mode",
            "cloud.images.push",
            "cloud.stock.push",
            "cloud.prices.pull",
            "cloud.stock.pull",
            "cloud.sales.push",
            "cloud.orders.intake",
            "cloud.orders.document",
            "cloud.orders.retention_days",
            "cloud.customers.sync",
            "cloud.customers.flags",
            "cloud.receipt_qr",
        ] {
            assert_eq!(
                def(key).unwrap().permission,
                Some(Permission::SettingsDzbuild),
                "{key}"
            );
        }
        // the online backup of 0.7–0.9 is gone (1.0.0, free edition): its setting with it, and a
        // value an older version stored is left alone, unread
        assert_eq!(
            def("cloud.backup.auto").unwrap_err().code(),
            "setting_unknown"
        );
        for (key, bad) in [
            ("cloud.publish.mode", json!("everything")),
            ("cloud.orders.document", json!("invoice")),
            ("cloud.orders.retention_days", json!(10)),
            ("cloud.orders.retention_days", json!(400)),
            ("cloud.customers.sync", json!("yes")),
        ] {
            let err = db
                .write(move |tx| set_value(tx, &Actor::default(), key, bad))
                .unwrap_err();
            assert_eq!(err.code(), "value_invalid", "{key}");
        }
    }

    #[test]
    fn a_stored_value_no_longer_valid_reads_as_the_default() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('ui.theme', '\"blue\"', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.read(|c| get_value(c, "ui.theme")).unwrap(),
            json!("system")
        );
        assert_eq!(db.read(|c| get::<String>(c, "ui.theme")).unwrap(), "system");
        let unknown = db.read(|c| get_value(c, "ui.colour")).unwrap_err();
        assert_eq!(
            (unknown.code().as_str(), unknown.payload().field.as_deref()),
            ("setting_unknown", Some("key"))
        );
    }

    #[test]
    fn backup_settings_have_defaults_and_bounds() {
        let db = Db::open_memory().unwrap();
        let retention = db.read(crate::db::Retention::from_settings).unwrap();
        assert_eq!(
            retention,
            crate::db::Retention {
                daily: 14,
                weekly: 8,
                monthly: 12
            }
        );
        let actor = Actor::system(None);
        let refused = db.write(move |tx| set_value(tx, &actor, "backup.keep_daily", json!(3)));
        assert_eq!(refused.unwrap_err().code(), "value_invalid");
        let actor = Actor::system(None);
        db.write(move |tx| set_value(tx, &actor, "backup.keep_monthly", json!(0)))
            .unwrap();
        assert_eq!(
            db.read(crate::db::Retention::from_settings)
                .unwrap()
                .monthly,
            0
        );
        assert_eq!(
            db.read(|c| get_value(c, "backup.on_close")).unwrap(),
            json!(true)
        );
        assert_eq!(
            def("backup.keep_weekly").unwrap().permission,
            Some(Permission::SettingsData)
        );
    }

    #[test]
    fn update_settings_accept_the_two_channels_only() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            db.read(|c| get_value(c, "updates.channel")).unwrap(),
            json!("stable")
        );
        assert_eq!(
            db.read(|c| get_value(c, "updates.auto_check")).unwrap(),
            json!(true)
        );
        let actor = Actor::system(None);
        let refused =
            db.write(move |tx| set_value(tx, &actor, "updates.channel", json!("nightly")));
        assert_eq!(refused.unwrap_err().code(), "value_invalid");
        let actor = Actor::system(None);
        db.write(move |tx| set_value(tx, &actor, "updates.channel", json!("beta")))
            .unwrap();
        assert_eq!(
            def("updates.auto_download").unwrap().permission,
            Some(Permission::SettingsData)
        );
    }

    #[test]
    fn the_support_contact_is_checked() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            db.read(|c| get_value(c, "support.contact")).unwrap(),
            json!({ "url": "https://dzbuild.com/pos/support" })
        );
        for bad in [
            json!({ "whatsapp": "05501" }),
            json!({ "whatsapp": "0550 12 34 56" }),
            json!({ "url": "http://example.dz" }),
            json!({ "url": "https://a b" }),
            json!({ "phone": "0550123456" }),
            json!("0550123456"),
        ] {
            let actor = Actor::system(None);
            let value = bad.clone();
            let refused = db.write(move |tx| set_value(tx, &actor, "support.contact", value));
            assert_eq!(refused.unwrap_err().code(), "value_invalid", "{bad}");
        }
        let actor = Actor::system(None);
        db.write(move |tx| {
            set_value(
                tx,
                &actor,
                "support.contact",
                json!({ "whatsapp": "213550123456", "label": "Informatique Benali" }),
            )
        })
        .unwrap();
        assert_eq!(
            def("support.contact").unwrap().permission,
            Some(Permission::SettingsCompany)
        );
    }
}

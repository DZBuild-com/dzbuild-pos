//! The bodies the POS sends DZBuild (Backend Brief; the open shapes fixed in the Backend contract
//! of the plan): products, in-store sales, refunds, Z closures. Amounts in the API's unit through
//! `ApiMoney`, rates in percent, quantities in units, times in Algiers RFC 3339. Nothing personal
//! leaves unless the owner turned `cloud.customers.sync` on; costs never leave.

use super::derive::OnlineStatus;
use super::store::{CloudLink, CloudSettings};
use super::wire::{ApiMoney, ApiQty, ApiRate, ApiTime};
use crate::cash::closing::ZReport;
use crate::catalog::products::Product;
use crate::company::{self, PriceMode, TaxRegime};
use crate::documents::{DocType, Document};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::tax::tva::{Discount, LineInput, compute_line};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadContext {
    pub store_language: Lang,
    pub price_mode: PriceMode,
    pub tax_regime: TaxRegime,
    pub location_id: Option<String>,
    pub customers_sync: bool,
}

impl PayloadContext {
    pub fn load(
        conn: &Connection,
        link: &CloudLink,
        settings: &CloudSettings,
    ) -> Result<PayloadContext, AppError> {
        let company = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
        Ok(PayloadContext {
            store_language: link.store_language,
            price_mode: company.price_mode,
            tax_regime: company.tax_regime,
            location_id: link.location_id.clone(),
            customers_sync: settings.customers_sync,
        })
    }
}

/// SHA-256 hex of the serialized body: the batch's idempotency key and the "unchanged" check.
pub fn body_hash<T: Serialize>(value: &T) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(hex::encode(Sha256::digest(&bytes).as_slice()))
}

/// The product's name in the store's language, the first non-blank of the others otherwise.
pub(crate) fn name_for(product: &Product, lang: Lang) -> String {
    let preferred = match lang {
        Lang::Ar => &product.name_ar,
        _ => &product.name_fr,
    };
    [
        preferred,
        &product.name_fr,
        &product.name_ar,
        &product.name_en,
    ]
    .into_iter()
    .find(|n| !n.trim().is_empty())
    .cloned()
    .unwrap_or_default()
}

/// The price a customer pays, whatever the shop's price mode.
pub(crate) fn consumer_price(product: &Product, mode: PriceMode) -> Result<Money, AppError> {
    match mode {
        PriceMode::Ttc => Ok(product.sell_price),
        PriceMode::Ht => Ok(compute_line(
            PriceMode::Ht,
            &LineInput {
                unit_price: product.sell_price,
                qty: Qty::ONE,
                discount: Discount::None,
                rate: product.tax_rate_bp,
            },
        )?
        .total_ttc),
    }
}

fn unit_barcode(product: &Product) -> Option<String> {
    product
        .barcodes
        .iter()
        .find(|b| b.pack_qty == Qty::ONE)
        .map(|b| b.barcode.clone())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pricing {
    pub price: ApiMoney,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inventory {
    pub track_stock: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryRef {
    pub external_id: String,
    pub name: String,
}

/// One item of `POST /v1/products/batch` (upsert by `external_id`, DZBuild falls back on `sku`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductItem {
    pub external_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub pricing: Pricing,
    pub inventory: Inventory,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<CategoryRef>,
}

/// The fields the owner may also edit online, as last pushed (compared by the pull, Task 11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ProductSnapshot {
    pub name: String,
    pub price: Money,
    pub status: String,
}

impl ProductItem {
    pub fn snapshot(&self) -> ProductSnapshot {
        ProductSnapshot {
            name: self.name.clone(),
            price: self.pricing.price.0,
            status: self.status.to_owned(),
        }
    }
}

pub fn product_item(
    conn: &Connection,
    product: &Product,
    status: OnlineStatus,
    ctx: &PayloadContext,
) -> Result<ProductItem, AppError> {
    let category = match product.category_id.as_deref() {
        Some(id) => crate::catalog::categories::get(conn, id)
            .ok()
            .map(|c| CategoryRef {
                external_id: c.id.clone(),
                name: [c.name_ar.clone(), c.name_fr.clone(), c.name_en.clone()]
                    .into_iter()
                    .find(|n| !n.trim().is_empty())
                    .unwrap_or_default(),
            }),
        None => None,
    };
    let online_stock = super::stock::online_pieces(conn, product)?.is_some();
    Ok(ProductItem {
        external_id: product.id.clone(),
        name: name_for(product, ctx.store_language),
        sku: product.reference.clone(),
        barcode: unit_barcode(product),
        pricing: Pricing {
            price: ApiMoney(consumer_price(product, ctx.price_mode)?),
        },
        inventory: Inventory {
            track_stock: online_stock,
            sku: product.reference.clone(),
            barcode: unit_barcode(product),
        },
        status: status.as_str(),
        category,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleCustomer {
    pub external_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_external_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub description: String,
    pub quantity: ApiQty,
    pub unit_price: ApiMoney,
    pub discount: ApiMoney,
    pub tax_rate: ApiRate,
    pub total: ApiMoney,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleTotals {
    pub subtotal: ApiMoney,
    pub discount: ApiMoney,
    pub tax: ApiMoney,
    pub stamp_duty: ApiMoney,
    pub rounding: ApiMoney,
    pub total: ApiMoney,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalePayment {
    pub method: &'static str,
    pub amount: ApiMoney,
}

/// `POST /v1/pos/sales`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalePayload {
    pub external_id: String,
    pub number: String,
    pub document_type: &'static str,
    pub channel: &'static str,
    pub apply_stock: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
    pub occurred_at: ApiTime,
    pub business_date: String,
    pub terminal: String,
    pub cashier_ref: String,
    pub price_mode: &'static str,
    pub tax_regime: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<SaleCustomer>,
    pub lines: Vec<SaleLine>,
    pub totals: SaleTotals,
    pub payments: Vec<SalePayment>,
}

fn cashier_ref(conn: &Connection, user_id: &str, terminal_code: &str) -> String {
    let username = crate::auth::users::get(conn, user_id)
        .map(|u| u.username)
        .unwrap_or_default();
    format!("{terminal_code}/{username}")
}

fn terminal_code(conn: &Connection, terminal_id: &str) -> Result<String, AppError> {
    Ok(crate::terminal::get(conn, terminal_id)?
        .map(|t| t.code)
        .unwrap_or_default())
}

fn doc_type_name(doc_type: DocType) -> &'static str {
    match doc_type {
        DocType::Invoice => "invoice",
        DocType::DeliveryNote => "delivery_note",
        DocType::CreditNote => "credit_note",
        DocType::ReturnNote => "return_note",
        _ => "ticket",
    }
}

fn abs(m: Money) -> Money {
    m.checked_abs().unwrap_or(m)
}

fn lines_of(conn: &Connection, doc: &Document) -> Result<Vec<SaleLine>, AppError> {
    let mut out = Vec::new();
    for line in doc.lines.iter().filter(|l| l.parent_line_id.is_none()) {
        let (sku, barcode) = match line.product_id.as_deref() {
            Some(id) => match crate::catalog::products::get(conn, id) {
                Ok(p) => (p.reference.clone(), unit_barcode(&p)),
                Err(_) => (None, None),
            },
            None => (None, None),
        };
        out.push(SaleLine {
            product_external_id: line.product_id.clone(),
            sku,
            barcode,
            description: line.description.clone(),
            quantity: ApiQty(Qty(line.qty.0.abs())),
            unit_price: ApiMoney(line.unit_price),
            discount: ApiMoney(abs(line.discount_amount)),
            tax_rate: ApiRate(Rate(line.tax_rate_bp.0)),
            total: ApiMoney(abs(line.total_ttc)),
        });
    }
    Ok(out)
}

fn customer_of(
    conn: &Connection,
    doc: &Document,
    ctx: &PayloadContext,
) -> Result<Option<SaleCustomer>, AppError> {
    let (true, Some(id)) = (ctx.customers_sync, doc.customer_id.as_deref()) else {
        return Ok(None);
    };
    let phone: Option<String> = conn
        .query_row("SELECT phone FROM customers WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    Ok(Some(SaleCustomer {
        external_id: id.to_owned(),
        phone,
    }))
}

pub fn sale_payload(
    conn: &Connection,
    doc: &Document,
    ctx: &PayloadContext,
) -> Result<SalePayload, AppError> {
    let terminal = terminal_code(conn, &doc.terminal_id)?;
    // The lines' gross before discount in the document's price mode (`tva::Totals::gross_total`):
    // the discount is HT in an HT shop, so the gross is too.
    let net = match doc.price_mode {
        PriceMode::Ttc => doc.total_ttc,
        PriceMode::Ht => doc.subtotal_ht,
    };
    let gross = net
        .checked_add(doc.discount_total)
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    Ok(SalePayload {
        external_id: doc.id.clone(),
        number: doc.number.clone().unwrap_or_default(),
        document_type: doc_type_name(doc.doc_type),
        channel: "pos",
        apply_stock: false,
        location_id: ctx.location_id.clone(),
        occurred_at: ApiTime(doc.validated_at.unwrap_or(doc.created_at)),
        business_date: doc.business_date.clone(),
        cashier_ref: cashier_ref(conn, &doc.user_id, &terminal),
        terminal,
        price_mode: if doc.price_mode == PriceMode::Ht {
            "ht"
        } else {
            "ttc"
        },
        tax_regime: if doc.tax_regime == TaxRegime::Ifu {
            "ifu"
        } else {
            "reel"
        },
        customer: customer_of(conn, doc, ctx)?,
        lines: lines_of(conn, doc)?,
        totals: SaleTotals {
            subtotal: ApiMoney(gross),
            discount: ApiMoney(doc.discount_total),
            tax: ApiMoney(doc.tax_total),
            stamp_duty: ApiMoney(doc.stamp_duty),
            rounding: ApiMoney(doc.rounding),
            total: ApiMoney(doc.total_due),
        },
        payments: doc
            .payments
            .iter()
            .map(|p| SalePayment {
                method: p.method.as_str(),
                amount: ApiMoney(p.amount),
            })
            .collect(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefundLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_external_id: Option<String>,
    pub description: String,
    pub quantity: ApiQty,
    pub amount: ApiMoney,
    /// Always `false`: the return's own `return` movement already reaches DZBuild as a
    /// `pos_return` delta of the adjustments batch, so a restock here would count it twice.
    pub restock: bool,
    /// Where the goods went, for DZBuild's reports only (it moves no stock): `shelf` or
    /// `damaged`; absent for a line without stock (an open item).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_to: Option<&'static str>,
}

/// `POST /v1/pos/sales/{id}/refunds`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefundPayload {
    pub external_id: String,
    pub number: String,
    pub document_type: &'static str,
    /// Always `false`, as for a sale: stock travels through the adjustments batch only.
    pub apply_stock: bool,
    pub occurred_at: ApiTime,
    pub terminal: String,
    pub lines: Vec<RefundLine>,
    pub total: ApiMoney,
    pub payments: Vec<SalePayment>,
}

pub fn refund_payload(
    conn: &Connection,
    doc: &Document,
    _ctx: &PayloadContext,
) -> Result<RefundPayload, AppError> {
    let damaged = crate::stock::DAMAGED_LOCATION_ID;
    Ok(RefundPayload {
        external_id: doc.id.clone(),
        number: doc.number.clone().unwrap_or_default(),
        document_type: doc_type_name(doc.doc_type),
        apply_stock: false,
        occurred_at: ApiTime(doc.validated_at.unwrap_or(doc.created_at)),
        terminal: terminal_code(conn, &doc.terminal_id)?,
        lines: doc
            .lines
            .iter()
            .filter(|l| l.parent_line_id.is_none())
            .map(|l| RefundLine {
                product_external_id: l.product_id.clone(),
                description: l.description.clone(),
                quantity: ApiQty(Qty(l.qty.0.abs())),
                amount: ApiMoney(abs(l.total_ttc)),
                restock: false,
                returned_to: l
                    .restock_location_id
                    .as_deref()
                    .map(|loc| if loc == damaged { "damaged" } else { "shelf" }),
            })
            .collect(),
        total: ApiMoney(abs(doc.total_due)),
        payments: doc
            .payments
            .iter()
            .map(|p| SalePayment {
                method: p.method.as_str(),
                amount: ApiMoney(abs(p.amount)),
            })
            .collect(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditAnchor {
    pub last_seq: i64,
    pub last_hash: String,
    pub intact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentAnchor {
    pub document_type: &'static str,
    pub series: String,
    pub last_seq: i64,
    pub last_hash: String,
    pub intact: bool,
}

/// The external tamper-evidence anchor (AUD-004): DZBuild keeps, outside the shop's PC, where the
/// audit journal and each document chain stood at this Z, and the Z's own hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureAnchors {
    pub audit: AuditAnchor,
    pub documents: Vec<DocumentAnchor>,
    pub closure_hash: String,
    pub closure_prev_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureTotals {
    pub sales: ApiMoney,
    pub returns: ApiMoney,
    pub net: ApiMoney,
    pub discount: ApiMoney,
    pub stamp_charged: ApiMoney,
    pub stamp_absorbed: ApiMoney,
    pub rounding: ApiMoney,
    pub grand_total: ApiMoney,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureMethod {
    pub method: &'static str,
    pub received: ApiMoney,
    pub paid_back: ApiMoney,
    pub net: ApiMoney,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureRate {
    pub tax_rate: ApiRate,
    pub base: ApiMoney,
    pub tax: ApiMoney,
    pub total: ApiMoney,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureCash {
    pub opening_float: ApiMoney,
    pub cash_sales: ApiMoney,
    pub cash_refunds: ApiMoney,
    pub paid_in: ApiMoney,
    pub paid_out: ApiMoney,
    pub drops: ApiMoney,
    pub expenses: ApiMoney,
    pub expected: ApiMoney,
    pub counted: ApiMoney,
    pub variance: ApiMoney,
}

/// `POST /v1/pos/closures`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosurePayload {
    pub external_id: String,
    pub z_number: String,
    pub terminal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
    pub business_date: String,
    pub opened_at: ApiTime,
    pub closed_at: ApiTime,
    pub cashier_ref: String,
    pub tickets: i64,
    pub returns: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_ticket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ticket: Option<String>,
    pub totals: ClosureTotals,
    pub by_method: Vec<ClosureMethod>,
    pub per_rate: Vec<ClosureRate>,
    pub cash: ClosureCash,
    pub anchors: ClosureAnchors,
}

pub fn closure_payload(
    conn: &Connection,
    z: &ZReport,
    ctx: &PayloadContext,
) -> Result<ClosurePayload, AppError> {
    let s = &z.summary;
    Ok(ClosurePayload {
        external_id: s.session.id.clone(),
        z_number: z.z_number.clone(),
        terminal: s.terminal_code.clone(),
        location_id: ctx.location_id.clone(),
        business_date: s.session.business_date.clone(),
        opened_at: ApiTime(s.session.opened_at),
        closed_at: ApiTime(z.closed_at),
        cashier_ref: cashier_ref(conn, &s.session.user_id, &s.terminal_code),
        tickets: s.tickets,
        returns: s.returns,
        first_ticket: s.first_ticket.clone(),
        last_ticket: s.last_ticket.clone(),
        totals: ClosureTotals {
            sales: ApiMoney(s.sales_ttc),
            returns: ApiMoney(s.returns_ttc),
            net: ApiMoney(s.net_ttc),
            discount: ApiMoney(s.discount_total),
            stamp_charged: ApiMoney(s.stamp_charged),
            stamp_absorbed: ApiMoney(s.stamp_absorbed),
            rounding: ApiMoney(s.rounding),
            grand_total: ApiMoney(z.grand_total),
        },
        by_method: s
            .by_method
            .iter()
            .map(|m| ClosureMethod {
                method: m.method.as_str(),
                received: ApiMoney(m.received),
                paid_back: ApiMoney(m.paid_back),
                net: ApiMoney(m.net),
                count: m.count,
            })
            .collect(),
        per_rate: s
            .per_rate
            .iter()
            .map(|r| ClosureRate {
                tax_rate: ApiRate(r.rate_bp),
                base: ApiMoney(r.base_ht),
                tax: ApiMoney(r.tax),
                total: ApiMoney(r.total_ttc),
            })
            .collect(),
        cash: ClosureCash {
            opening_float: ApiMoney(s.opening_float),
            cash_sales: ApiMoney(s.cash_sales),
            cash_refunds: ApiMoney(s.cash_refunds),
            paid_in: ApiMoney(s.paid_in),
            paid_out: ApiMoney(s.paid_out),
            drops: ApiMoney(s.drops),
            expenses: ApiMoney(s.expenses_total),
            expected: ApiMoney(s.expected_cash.unwrap_or(Money::ZERO)),
            counted: ApiMoney(z.counted_cash),
            variance: ApiMoney(z.variance),
        },
        anchors: ClosureAnchors {
            audit: AuditAnchor {
                last_seq: z.audit_anchor.last_seq,
                last_hash: z.audit_anchor.last_hash.clone(),
                intact: z.audit_anchor.intact,
            },
            documents: z
                .document_anchors
                .iter()
                .map(|a| DocumentAnchor {
                    document_type: doc_type_name(a.doc_type),
                    series: a.series.clone(),
                    last_seq: a.last_seq,
                    last_hash: a.last_hash.clone(),
                    intact: a.intact,
                })
                .collect(),
            closure_hash: z.hash.clone(),
            closure_prev_hash: z.prev_hash.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cash::closing::{CloseInput, CountLine, close_session};
    use crate::catalog::products::{Barcode, ProductInput};
    use crate::cloud::derive::OnlineStatus;
    use crate::cloud::store::cloud_settings;
    use crate::cloud::testkit::linked_shop;
    use crate::money::{Money, Qty};
    use crate::payments::PaymentInput;
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::tax::tva::Discount;
    use serde_json::{Value, json};

    /// Ids and times vary per run: the snapshot keeps their place, not their value.
    fn stable(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| {
                        let fixed = matches!(
                            k.as_str(),
                            "external_id"
                                | "product_external_id"
                                | "occurred_at"
                                | "opened_at"
                                | "closed_at"
                                | "last_hash"
                                | "closure_hash"
                                | "closure_prev_hash"
                                | "cashier_ref"
                        ) && !v.is_null();
                        (k, if fixed { json!("[stable]") } else { stable(v) })
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(stable).collect()),
            other => other,
        }
    }

    fn context(
        shop: &crate::testkit::Shop,
        link: &crate::cloud::store::CloudLink,
    ) -> PayloadContext {
        let link = link.clone();
        shop.db
            .read(move |c| {
                let settings = cloud_settings(c)?;
                PayloadContext::load(c, &link, &settings)
            })
            .unwrap()
    }

    #[test]
    fn a_product_is_pushed_at_its_ttc_price_with_its_first_unit_barcode() {
        let (shop, link) = linked_shop();
        let mut input = ProductInput::simple(
            "Sac cuir noir",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(450_000),
        );
        input.name_ar = "حقيبة جلد سوداء".into();
        input.reference = Some("SAC-CUIR-NOIR".into());
        input.barcodes = vec![
            Barcode {
                barcode: "6130000012345".into(),
                pack_qty: Qty(1_000),
                label: None,
            },
            Barcode {
                barcode: "6130000099999".into(),
                pack_qty: Qty(6_000),
                label: Some("carton".into()),
            },
        ];
        let bag = shop.product_with(input);
        let ctx = context(&shop, &link);
        let item = shop
            .db
            .read(move |c| product_item(c, &bag, OnlineStatus::Active, &ctx))
            .unwrap();
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json["pricing"]["price"],
            json!("4500.00"),
            "dinars, per API_MONEY_UNIT"
        );
        assert_eq!(
            json["name"], "حقيبة جلد سوداء",
            "the store's language (ar), fr as the fallback"
        );
        assert_eq!(
            json["barcode"], "6130000012345",
            "the one-piece barcode, not the carton"
        );
        assert_eq!(json["sku"], "SAC-CUIR-NOIR");
        assert_eq!(json["status"], "active");
        assert!(
            json.get("cost_price").is_none() && json["pricing"].get("cost_price").is_none(),
            "costs stay home"
        );
        insta::assert_json_snapshot!("product_item", stable(json));
    }

    #[test]
    fn an_ht_shop_pushes_the_consumer_price() {
        let (shop, link) = linked_shop();
        shop.db
            .write(|tx| {
                tx.execute("UPDATE company SET price_mode = 'ht'", [])?;
                Ok(())
            })
            .unwrap();
        let p = shop.product("Ciment", Money(100_000), &shop.tva19);
        let ctx = context(&shop, &link);
        let item = shop
            .db
            .read(move |c| product_item(c, &p, OnlineStatus::Active, &ctx))
            .unwrap();
        assert_eq!(
            item.pricing.price,
            crate::cloud::wire::ApiMoney(Money(119_000)),
            "1 000 DA HT + 19 %"
        );
    }

    #[test]
    fn a_sale_carries_lines_totals_and_the_change_as_a_negative_payment() {
        let (shop, link) = linked_shop();
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        shop.stock_in(&bag.id, 5_000);
        shop.open_till(0);
        let ctx_till = crate::till::TillContext::new(
            shop.owner.clone(),
            shop.pack.clone(),
            crate::testkit::NOON,
        );
        let mut line = CartLine::product(&bag.id, Qty(1_000));
        line.discount = Discount::Amount(Money(50_000));
        let input = SaleInput {
            cart: Cart::new(crate::lang::Lang::Fr, vec![line]),
            payments: vec![PaymentInput::cash(Money(500_000))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &ctx_till, input))
            .unwrap();
        let ctx = context(&shop, &link);
        let doc = sale.document.clone();
        let payload = shop.db.read(move |c| sale_payload(c, &doc, &ctx)).unwrap();
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["external_id"], sale.document.id);
        assert_eq!(json["channel"], "pos");
        assert_eq!(
            json["apply_stock"], false,
            "stock travels through the adjustments batch only"
        );
        assert_eq!(json["lines"][0]["unit_price"], json!("4500.00"));
        assert_eq!(json["lines"][0]["discount"], json!("500.00"));
        assert_eq!(json["lines"][0]["tax_rate"], json!(19));
        assert_eq!(json["totals"]["subtotal"], json!("4500.00"));
        assert_eq!(
            json["totals"]["total"],
            json!("4040.00"),
            "4 000 + 40 DA droit de timbre on the cash"
        );
        let methods: Vec<(String, Value)> = payload
            .payments
            .iter()
            .map(|p| (p.method.to_owned(), json!(p.amount)))
            .collect();
        assert!(
            methods.contains(&("change".to_owned(), json!("-960.00"))),
            "{methods:?}"
        );
        assert!(
            json.get("customer").is_none(),
            "an anonymous sale names no customer"
        );
        insta::assert_json_snapshot!("sale_payload", stable(json));
    }

    #[test]
    fn a_sale_names_its_customer_only_when_the_owner_allows_it() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 5_000);
        shop.open_till(0);
        let mut known = crate::customers::CustomerInput::named("Amel");
        known.phone = Some("0555123456".into());
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let amel = shop
            .db
            .write(move |tx| crate::customers::create(tx, &actor, &pack, known))
            .unwrap();
        let till = shop.till();
        let mut cart = Cart::new(
            crate::lang::Lang::Fr,
            vec![CartLine::product(&milk.id, Qty(1_000))],
        );
        cart.customer_id = Some(amel.id.clone());
        let input = SaleInput {
            cart,
            payments: vec![PaymentInput::cash(Money(10_000))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &till, input))
            .unwrap();
        assert_eq!(sale.document.customer_id.as_deref(), Some(amel.id.as_str()));
        let body = |ctx: PayloadContext| {
            let doc = sale.document.clone();
            let payload = shop.db.read(move |c| sale_payload(c, &doc, &ctx)).unwrap();
            serde_json::to_value(&payload).unwrap()
        };

        let ctx = context(&shop, &link);
        assert!(
            !ctx.customers_sync,
            "loi 18-07: off unless the owner turns it on"
        );
        let json = body(ctx);
        assert!(
            json.get("customer").is_none(),
            "no personal data leaves the PC by default: {json}"
        );

        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                crate::settings::set_value(tx, &actor, "cloud.customers.sync", json!(true))
                    .map(drop)
            })
            .unwrap();
        let ctx = context(&shop, &link);
        assert!(ctx.customers_sync);
        assert_eq!(
            body(ctx)["customer"],
            json!({ "external_id": amel.id, "phone": "0555123456" }),
            "the owner allowed it: the customer's id and phone"
        );
    }

    #[test]
    fn an_ht_shop_sale_sends_its_ht_gross_as_the_subtotal() {
        let (shop, link) = linked_shop();
        shop.db
            .write(|tx| {
                tx.execute("UPDATE company SET price_mode = 'ht'", [])?;
                Ok(())
            })
            .unwrap();
        let cement = shop.product("Ciment", Money(20_000), &shop.tva19);
        shop.stock_in(&cement.id, 5_000);
        shop.open_till(0);
        let till = shop.owner_till();
        let mut line = CartLine::product(&cement.id, Qty(1_000));
        line.discount = Discount::Percent(crate::money::Rate(1_000));
        let input = SaleInput {
            cart: Cart::new(crate::lang::Lang::Fr, vec![line]),
            payments: vec![PaymentInput::cash(Money(21_420))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &till, input))
            .unwrap();
        let ctx = context(&shop, &link);
        let doc = sale.document.clone();
        let payload = shop.db.read(move |c| sale_payload(c, &doc, &ctx)).unwrap();
        assert_eq!(payload.price_mode, "ht");
        assert_eq!(
            (payload.lines[0].unit_price, payload.lines[0].discount),
            (ApiMoney(Money(20_000)), ApiMoney(Money(2_000))),
            "200 DA HT, 10 % off, in the price mode"
        );
        let t = &payload.totals;
        assert_eq!(
            (t.subtotal, t.discount, t.tax, t.stamp_duty, t.total),
            (
                ApiMoney(Money(20_000)),
                ApiMoney(Money(2_000)),
                ApiMoney(Money(3_420)),
                ApiMoney(Money::ZERO),
                ApiMoney(Money(21_420))
            ),
            "HT gross 200 − 20 = 180 HT, + 34.20 TVA = 214.20 (no timbre up to 300 DA)"
        );
    }

    #[test]
    fn a_refund_goes_out_in_positive_amounts_without_moving_stock_and_with_a_stable_hash() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let till = shop.till();
        let sell = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(&milk.id, Qty(2_000))],
            ),
            payments: vec![PaymentInput::cash(Money(20_000))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &till, sell))
            .unwrap();
        let refund = crate::cloud::testkit::return_all(&shop, &sale.document.id);
        let ctx = context(&shop, &link);
        let doc = refund.clone();
        let payload = shop
            .db
            .read(move |c| refund_payload(c, &doc, &ctx))
            .unwrap();
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["external_id"], refund.id);
        assert_eq!(json["document_type"], "return_note");
        assert_eq!(
            json["apply_stock"], false,
            "the return's stock travels as a pos_return delta in the adjustments batch only"
        );
        assert_eq!(
            json["lines"][0]["quantity"],
            json!(2),
            "a return note's negative quantity goes out positive"
        );
        assert_eq!(json["lines"][0]["amount"], json!("200.00"));
        assert_eq!(
            json["lines"][0]["restock"], false,
            "DZBuild never restocks what the adjustments batch already moved"
        );
        assert_eq!(
            json["lines"][0]["returned_to"], "shelf",
            "where the goods went, for DZBuild's reports only"
        );
        assert_eq!(json["total"], json!("200.00"));
        assert_eq!(
            json["payments"],
            json!([{ "method": "cash", "amount": "200.00" }])
        );
        let hash = body_hash(&payload).unwrap();
        assert!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "{hash}"
        );
        assert_eq!(
            hash,
            body_hash(&payload.clone()).unwrap(),
            "the same body, the same key"
        );
        let mut other = payload;
        other.lines[0].returned_to = Some("damaged");
        assert_ne!(hash, body_hash(&other).unwrap());
        insta::assert_json_snapshot!("refund_payload", stable(json));
    }

    #[test]
    fn a_refund_to_the_damaged_goods_location_moves_no_online_stock_either() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let till = shop.till();
        let sell = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(&milk.id, Qty(1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(10_000))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &till, sell))
            .unwrap();
        let refund = crate::cloud::testkit::return_all_to(&shop, &sale.document.id, false);
        assert_eq!(
            refund.lines[0].restock_location_id.as_deref(),
            Some(crate::stock::DAMAGED_LOCATION_ID)
        );
        let ctx = context(&shop, &link);
        let payload = shop
            .db
            .read(move |c| refund_payload(c, &refund, &ctx))
            .unwrap();
        assert!(!payload.apply_stock);
        assert_eq!(
            (payload.lines[0].restock, payload.lines[0].returned_to),
            (false, Some("damaged"))
        );
    }

    #[test]
    fn the_closure_carries_the_audit_and_document_anchors() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(20_000);
        let till = shop.till();
        let sell = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(&milk.id, Qty(2_000))],
            ),
            payments: vec![PaymentInput::cash(Money(20_000))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &till, sell))
            .unwrap();
        let till = shop.till();
        let z = shop
            .db
            .write(move |tx| {
                close_session(
                    tx,
                    &till,
                    CloseInput {
                        counts: vec![CountLine {
                            denomination: Money(20_000),
                            count: 2,
                        }],
                        notes: None,
                    },
                )
            })
            .unwrap();
        let ctx = context(&shop, &link);
        let zc = z.clone();
        let payload = shop
            .db
            .read(move |c| closure_payload(c, &zc, &ctx))
            .unwrap();
        assert_eq!(payload.anchors.audit.last_seq, z.audit_anchor.last_seq);
        assert_eq!(payload.anchors.audit.last_hash, z.audit_anchor.last_hash);
        assert!(payload.anchors.audit.intact);
        assert_eq!(payload.anchors.documents.len(), z.document_anchors.len());
        assert_eq!(
            payload.anchors.documents[0].last_hash,
            z.document_anchors[0].last_hash
        );
        assert_eq!(payload.anchors.closure_hash, z.hash);
        assert_eq!(payload.z_number, z.z_number);
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["totals"]["net"], json!("200.00"));
        assert_eq!(json["cash"]["counted"], json!("400.00"));
        insta::assert_json_snapshot!("closure_payload", stable(json));
    }

    #[test]
    #[ignore = "writes the Backend contract examples for DZBuild: cargo test -p pos-core write_contract_fixtures -- --ignored"]
    fn write_contract_fixtures() {
        // Copies the `stable` JSON of the four snapshots above to tests/fixtures/cloud/
        // {product_item,sale_payload,refund_payload,closure_payload}.json for the owner to forward.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cloud");
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "product_item",
            "sale_payload",
            "refund_payload",
            "closure_payload",
        ] {
            let snap =
                std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
                    format!("src/cloud/snapshots/pos_core__cloud__payloads__tests__{name}.snap"),
                ))
                .unwrap();
            let body = snap.splitn(3, "---").nth(2).unwrap_or_default().trim();
            std::fs::write(dir.join(format!("{name}.json")), format!("{body}\n")).unwrap();
        }
    }
}

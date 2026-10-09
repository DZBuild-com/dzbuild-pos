//! Personal data (loi 18-07 as amended by loi 25-11; master spec §6 PDP): marketing consent per
//! channel, logged and revocable; every opening, export or print of a customer file journaled
//! (without the personal fields); a customer's data exported on request; a customer anonymised
//! on request once nothing is open — the validated documents keep their frozen buyer (fiscal
//! retention) and the legal registers (pharmacy, jeweller) keep their entries; the DPO contact
//! and the processing register printed from the modules in use. The tamper-evident journal is
//! never rewritten: what M2 journaled when the customer was created stays there, which the
//! anonymisation screen says (Task 30).

use crate::appointments::{self, Appointment};
use crate::audit::{self, Actor};
use crate::customers::{self, Customer};
use crate::documents::DocumentSummary;
use crate::error::AppError;
use crate::gift_vouchers::{self, Voucher};
use crate::ids::new_id;
use crate::instalments::{self, Plan, PlanFilter};
use crate::ledger::{EntryQuery, LedgerEntry};
use crate::loyalty::{self, Account, PointsEntry};
use crate::orders::{self, CustomerOrderFilter, OrderSummary};
use crate::packs::{self, Module};
use crate::service_tickets::{self, ServiceTicket};
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use rusqlite::{Connection, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// A channel the shop may use to reach the customer with offers (PDP-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConsentChannel {
    Sms,
    Whatsapp,
    Loyalty,
}

/// Where the customer's answer was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConsentSource {
    Counter,
    Form,
    Import,
    CustomerRequest,
}

/// Why a customer file was opened (PDP-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AccessPurpose {
    View,
    Export,
    Print,
}

impl ConsentChannel {
    pub const ALL: [ConsentChannel; 3] = [
        ConsentChannel::Sms,
        ConsentChannel::Whatsapp,
        ConsentChannel::Loyalty,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ConsentChannel::Sms => "sms",
            ConsentChannel::Whatsapp => "whatsapp",
            ConsentChannel::Loyalty => "loyalty",
        }
    }

    pub fn parse(s: &str) -> Option<ConsentChannel> {
        ConsentChannel::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

impl ConsentSource {
    pub const ALL: [ConsentSource; 4] = [
        ConsentSource::Counter,
        ConsentSource::Form,
        ConsentSource::Import,
        ConsentSource::CustomerRequest,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ConsentSource::Counter => "counter",
            ConsentSource::Form => "form",
            ConsentSource::Import => "import",
            ConsentSource::CustomerRequest => "customer_request",
        }
    }

    pub fn parse(s: &str) -> Option<ConsentSource> {
        ConsentSource::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

impl AccessPurpose {
    pub const ALL: [AccessPurpose; 3] = [
        AccessPurpose::View,
        AccessPurpose::Export,
        AccessPurpose::Print,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            AccessPurpose::View => "view",
            AccessPurpose::Export => "export",
            AccessPurpose::Print => "print",
        }
    }

    pub fn parse(s: &str) -> Option<AccessPurpose> {
        AccessPurpose::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

sql_text_enum!(ConsentChannel);
sql_text_enum!(ConsentSource);
sql_text_enum!(AccessPurpose);

/// One answer of the customer about a channel, with who recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Consent {
    pub channel: ConsentChannel,
    pub granted: bool,
    pub source: ConsentSource,
    pub note: Option<String>,
    pub created_at: i64,
    pub user_name: String,
}

/// The customer's loyalty account and points ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerLoyalty {
    pub account: Account,
    pub entries: Vec<PointsEntry>,
}

/// Everything the shop keeps about a customer (PDP-004, the right of access and portability),
/// each list in its module's own shape.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerDataExport {
    pub customer: Customer,
    /// Every answer, newest first.
    pub consents: Vec<Consent>,
    /// The kridi ledger, newest first.
    pub ledger: Vec<LedgerEntry>,
    /// The validated documents naming the customer, newest first.
    pub documents: Vec<DocumentSummary>,
    pub loyalty: CustomerLoyalty,
    pub vouchers: Vec<Voucher>,
    pub orders: Vec<OrderSummary>,
    pub appointments: Vec<Appointment>,
    pub service_tickets: Vec<ServiceTicket>,
    pub instalments: Vec<Plan>,
    pub exported_at: i64,
}

/// What an anonymisation left in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AnonymiseReport {
    pub customer_id: String,
    pub code: String,
    /// Validated documents that keep their buyer for the fiscal retention.
    pub kept_documents: i64,
    /// Entries of the legal registers (pharmacy, jeweller) that stay as written.
    pub kept_register_entries: i64,
}

/// The shop's data protection officer (PDP-005), printed on the processing register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Dpo {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
}

/// A processing activity of the register (PDP-005). The words are catalogue keys of the
/// documents (`a4.privacy.activity.<key>.*`), translated when printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub key: String,
    pub legal_basis: String,
    pub data: String,
    pub retention: String,
}

impl Activity {
    /// The activity `key` with its catalogue words.
    pub fn of(key: &str) -> Activity {
        let word = |part: &str| format!("a4.privacy.activity.{key}.{part}");
        Activity {
            key: key.to_owned(),
            legal_basis: word("basis"),
            data: word("data"),
            retention: word("retention"),
        }
    }
}

/// Records the customer's answer about a channel (PDP-002) and keeps M2's single
/// `marketing_consent` flag in step with the latest SMS and WhatsApp answers.
pub fn set_consent(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    channel: ConsentChannel,
    granted: bool,
    source: ConsentSource,
    note: Option<String>,
) -> Result<Consent, AppError> {
    customers::get(tx, customer_id)?;
    let note = crate::catalog::clean_opt(note.as_deref(), "note", 200)?;
    tx.execute(
        "INSERT INTO customer_consents (id, customer_id, channel, granted, source, note, created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_id(),
            customer_id,
            channel,
            granted,
            source,
            note,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    let marketing: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM customer_consents c
                        WHERE c.customer_id = ?1 AND c.channel IN ('sms', 'whatsapp') AND c.granted = 1
                          AND c.rowid = (SELECT x.rowid FROM customer_consents x
                                         WHERE x.customer_id = c.customer_id AND x.channel = c.channel
                                         ORDER BY x.created_at DESC, x.rowid DESC LIMIT 1))",
        [customer_id],
        |r| r.get(0),
    )?;
    tx.execute(
        "UPDATE customers SET marketing_consent = ?2, updated_at = ?3 WHERE id = ?1",
        params![customer_id, marketing, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "privacy.consent_set",
            "customer",
            Some(customer_id),
            json!({ "channel": channel, "granted": granted, "source": source }),
        ),
    )?;
    consents(tx, customer_id)?
        .into_iter()
        .find(|c| c.channel == channel)
        .ok_or_else(|| AppError::Internal("the consent just recorded is missing".into()))
}

fn consent_of(r: &Row<'_>) -> rusqlite::Result<Consent> {
    Ok(Consent {
        channel: r.get(0)?,
        granted: r.get(1)?,
        source: r.get(2)?,
        note: r.get(3)?,
        created_at: r.get(4)?,
        user_name: r.get(5)?,
    })
}

/// The answer in force per channel: the newest of each.
pub fn consents(conn: &Connection, customer_id: &str) -> Result<Vec<Consent>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.channel, c.granted, c.source, c.note, c.created_at, COALESCE(u.display_name, c.user_id)
         FROM (SELECT x.*, ROW_NUMBER() OVER (PARTITION BY x.channel
                                              ORDER BY x.created_at DESC, x.rowid DESC) AS n
               FROM customer_consents x WHERE x.customer_id = ?1) c
         LEFT JOIN users u ON u.id = c.user_id
         WHERE c.n = 1
         ORDER BY CASE c.channel WHEN 'sms' THEN 0 WHEN 'whatsapp' THEN 1 ELSE 2 END",
    )?;
    let rows = stmt
        .query_map([customer_id], consent_of)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every answer of the customer, newest first.
pub fn consent_history(conn: &Connection, customer_id: &str) -> Result<Vec<Consent>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.channel, c.granted, c.source, c.note, c.created_at, COALESCE(u.display_name, c.user_id)
         FROM customer_consents c LEFT JOIN users u ON u.id = c.user_id
         WHERE c.customer_id = ?1
         ORDER BY c.created_at DESC, c.rowid DESC",
    )?;
    let rows = stmt
        .query_map([customer_id], consent_of)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Journals that a customer file was opened, exported or printed (PDP-003): the customer's id
/// and the purpose, never a personal field.
pub fn log_access(
    tx: &Transaction<'_>,
    actor: &Actor,
    customer_id: &str,
    purpose: AccessPurpose,
) -> Result<(), AppError> {
    customers::get(tx, customer_id)?;
    audit::append(
        tx,
        &actor.event(
            "privacy.accessed",
            "customer",
            Some(customer_id),
            json!({ "purpose": purpose }),
        ),
    )
    .map(drop)
}

/// Every validated document naming the customer, newest first.
fn documents_of(conn: &Connection, customer_id: &str) -> Result<Vec<DocumentSummary>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, doc_type, number, status, business_date, validated_at, total_due, user_id,
                terminal_id, parent_id
         FROM documents WHERE customer_id = ?1 AND status = 'validated'
         ORDER BY validated_at DESC, rowid DESC",
    )?;
    let rows = stmt
        .query_map([customer_id], |r| {
            Ok(DocumentSummary {
                id: r.get(0)?,
                doc_type: r.get(1)?,
                number: r.get(2)?,
                status: r.get(3)?,
                business_date: r.get(4)?,
                validated_at: r.get(5)?,
                total_due: r.get(6)?,
                user_id: r.get(7)?,
                terminal_id: r.get(8)?,
                parent_id: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The ids `sql` lists for the customer.
fn ids_of(conn: &Connection, sql: &str, customer_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt
        .query_map([customer_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The whole kridi ledger, page after page.
fn ledger_of(conn: &Connection, customer_id: &str) -> Result<Vec<LedgerEntry>, AppError> {
    let mut entries = Vec::new();
    let mut before = None;
    loop {
        let page = customers::credit::entries(
            conn,
            customer_id,
            &EntryQuery {
                from: None,
                to: None,
                limit: Some(5_000),
                before: before.take(),
            },
        )?;
        let truncated = page.truncated;
        before = page.entries.last().map(|e| e.id.clone());
        entries.extend(page.entries);
        if !truncated || before.is_none() {
            return Ok(entries);
        }
    }
}

/// Everything the shop keeps about the customer (PDP-004). The command writes it as JSON beside
/// an xlsx of the documents and journals the export.
pub fn export_customer(
    conn: &Connection,
    customer_id: &str,
) -> Result<CustomerDataExport, AppError> {
    let customer = customers::get(conn, customer_id)?;
    let now = crate::time::now_ms();
    let today = crate::time::business_date(now)
        .format("%Y-%m-%d")
        .to_string();
    let vouchers = ids_of(
        conn,
        "SELECT code FROM gift_vouchers WHERE customer_id = ?1 ORDER BY created_at DESC, rowid DESC",
        customer_id,
    )?
    .iter()
    .map(|code| gift_vouchers::get(conn, code))
    .collect::<Result<Vec<_>, _>>()?;
    let appointments = ids_of(
        conn,
        "SELECT id FROM appointments WHERE customer_id = ?1 ORDER BY starts_at DESC, rowid DESC",
        customer_id,
    )?
    .iter()
    .map(|id| appointments::get(conn, id))
    .collect::<Result<Vec<_>, _>>()?;
    let service_tickets = ids_of(
        conn,
        "SELECT id FROM service_tickets WHERE customer_id = ?1 ORDER BY created_at DESC, rowid DESC",
        customer_id,
    )?
    .iter()
    .map(|id| service_tickets::get(conn, id))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(CustomerDataExport {
        consents: consent_history(conn, customer_id)?,
        ledger: ledger_of(conn, customer_id)?,
        documents: documents_of(conn, customer_id)?,
        loyalty: CustomerLoyalty {
            account: loyalty::account(conn, customer_id, &today)?,
            entries: loyalty::entries(conn, customer_id)?,
        },
        vouchers,
        orders: orders::list(
            conn,
            &CustomerOrderFilter {
                customer_id: Some(customer_id.to_owned()),
                ..CustomerOrderFilter::default()
            },
        )?,
        appointments,
        service_tickets,
        instalments: instalments::list(
            conn,
            &PlanFilter {
                customer_id: Some(customer_id.to_owned()),
                ..PlanFilter::default()
            },
            &today,
        )?,
        customer,
        exported_at: now,
    })
}

/// What the customer still has open with the shop: a balance either way, deposits held, an
/// order, a repair, a voucher with money left or a booked appointment. And what keeps personal
/// data the app cannot erase (final review, loi 18-07): the identity numbers and phones of an
/// instalment contract's documents, the buyer's name on a gift voucher — append-only rows, so the
/// customer is not anonymised while they exist (the owner is told which).
fn open_items(conn: &Connection, customer_id: &str) -> Result<Vec<&'static str>, AppError> {
    let checks: [(&str, &str); 8] = [
        (
            "balance",
            "SELECT balance <> 0 FROM customers WHERE id = ?1",
        ),
        (
            "deposits",
            "SELECT COALESCE(SUM(amount), 0) <> 0 FROM deposits WHERE customer_id = ?1",
        ),
        (
            "orders",
            "SELECT EXISTS (SELECT 1 FROM customer_orders o JOIN documents d ON d.id = o.document_id
                            WHERE d.customer_id = ?1 AND o.status IN ('open', 'ready'))",
        ),
        (
            "serviceTickets",
            "SELECT EXISTS (SELECT 1 FROM service_tickets WHERE customer_id = ?1
                            AND status NOT IN ('delivered', 'returned_unrepaired', 'cancelled'))",
        ),
        (
            "vouchers",
            "SELECT EXISTS (SELECT 1 FROM gift_vouchers v WHERE v.customer_id = ?1
                            AND (SELECT COALESCE(SUM(amount), 0) FROM gift_voucher_ledger l
                                 WHERE l.voucher_id = v.id) > 0)",
        ),
        (
            "appointments",
            "SELECT EXISTS (SELECT 1 FROM appointments WHERE customer_id = ?1 AND status = 'booked')",
        ),
        (
            "instalmentGuarantees",
            "SELECT EXISTS (SELECT 1 FROM instalment_guarantees g
                            JOIN instalment_plans p ON p.id = g.plan_id
                            WHERE p.customer_id = ?1
                              AND (g.holder_name IS NOT NULL OR g.reference IS NOT NULL
                                   OR g.phone IS NOT NULL OR g.note IS NOT NULL))",
        ),
        (
            "voucherBuyer",
            "SELECT EXISTS (SELECT 1 FROM gift_vouchers WHERE customer_id = ?1 AND buyer_name IS NOT NULL)",
        ),
    ];
    let mut open = Vec::new();
    for (name, sql) in checks {
        if conn.query_row(sql, [customer_id], |r| r.get::<_, bool>(0))? {
            open.push(name);
        }
    }
    Ok(open)
}

/// Anonymises a customer on request (PDP-004), refused while anything is open
/// (`customer_has_open_items`, `details.open`): the name becomes "Client anonymisé <code>", the
/// contact, identity (NIN, NIF, NIS, RC, AI, the VAT exemption's reference, the legal form and
/// activity, the wilaya), tags and notes are erased, the appointments lose the name and phone, a live
/// loyalty card is blocked, every channel is refused, and the file is archived. The validated
/// documents keep their frozen buyer and the legal registers their entries. One journal row,
/// without the erased fields.
pub fn anonymise(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    note: Option<String>,
) -> Result<AnonymiseReport, AppError> {
    let customer = customers::get(tx, customer_id)?;
    let note = crate::catalog::clean_opt(note.as_deref(), "note", 200)?;
    let open = open_items(tx, customer_id)?;
    if !open.is_empty() {
        return Err(AppError::rejected(
            json!({ "open": open }),
            "customer_has_open_items",
        ));
    }
    let name = format!("Client anonymisé {}", customer.code);
    let search = crate::catalog::normalize::search_text([name.as_str(), customer.code.as_str()]);
    tx.execute(
        "UPDATE customers SET name = ?2, search_text = ?3, phone = NULL, phone2 = NULL, email = NULL,
                address = NULL, wilaya = NULL, nin = NULL, nif = NULL, nis = NULL, rc = NULL, ai = NULL,
                vat_exempt_ref = NULL, legal_form = NULL, activity = NULL, tags = NULL, notes = NULL,
                ids_verified_at = NULL, marketing_consent = 0,
                archived_at = COALESCE(archived_at, ?4), updated_at = ?4
         WHERE id = ?1",
        params![customer_id, name, search, ctx.now],
    )?;
    tx.execute(
        "UPDATE appointments SET customer_name = ?2, phone = NULL, updated_at = ?3 WHERE customer_id = ?1",
        params![customer_id, name, ctx.now],
    )?;
    tx.execute(
        "UPDATE loyalty_cards SET blocked_at = ?2 WHERE customer_id = ?1 AND blocked_at IS NULL",
        params![customer_id, ctx.now],
    )?;
    for channel in ConsentChannel::ALL {
        tx.execute(
            "INSERT INTO customer_consents (id, customer_id, channel, granted, source, created_at, user_id)
             VALUES (?1, ?2, ?3, 0, 'customer_request', ?4, ?5)",
            params![new_id(), customer_id, channel, ctx.now, ctx.session.user_id],
        )?;
    }
    let kept_documents: i64 = tx.query_row(
        "SELECT COUNT(*) FROM documents WHERE customer_id = ?1 AND status = 'validated'",
        [customer_id],
        |r| r.get(0),
    )?;
    let kept_register_entries: i64 = tx.query_row(
        "SELECT (SELECT COUNT(*) FROM controlled_register r JOIN documents d ON d.id = r.document_id
                 WHERE d.customer_id = ?1)
              + (SELECT COUNT(*) FROM jeweller_register r JOIN documents d ON d.id = r.document_id
                 WHERE d.customer_id = ?1)",
        [customer_id],
        |r| r.get(0),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "privacy.anonymised",
            "customer",
            Some(customer_id),
            json!({
                "code": customer.code,
                "keptDocuments": kept_documents,
                "keptRegisterEntries": kept_register_entries,
                "note": note,
                "approvals": ctx.approvers(),
            }),
        ),
    )?;
    Ok(AnonymiseReport {
        customer_id: customer_id.to_owned(),
        code: customer.code,
        kept_documents,
        kept_register_entries,
    })
}

/// The shop's DPO (`privacy.dpo`), if one is named.
pub fn dpo(conn: &Connection) -> Result<Option<Dpo>, AppError> {
    let value = crate::settings::get_value(conn, "privacy.dpo")?;
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(value)?))
}

/// The shop's processing activities (PDP-005): the customer file and kridi always, then each
/// module in use that keeps personal data.
pub fn activities(conn: &Connection) -> Result<Vec<Activity>, AppError> {
    let on = packs::state(conn)?.modules;
    let mut out = vec![Activity::of("customers")];
    for (module, key) in [
        (Module::Loyalty, "loyalty"),
        (Module::Pharmacy, "pharmacy_register"),
        (Module::Jewellery, "jeweller_register"),
        (Module::Staff, "staff"),
        (Module::Instalments, "instalments"),
        (Module::Appointments, "appointments"),
    ] {
        if on.contains(&module) {
            out.push(Activity::of(key));
        }
    }
    Ok(out)
}

//! Receipts after the sale (spec §7.1): the sale commits, a queue thread prints it, and a
//! failure never touches the sale — it reaches the window as a `print-failed` event whose toast
//! offers a retry (the original, once) or a reprint (DUPLICATA). Versement receipts (M2,
//! CRD-004) go through the same queue after the versement commits, and so do the pack slips (M8):
//! kitchen tickets on their station's printer (marked printed once on paper), the addition, the
//! repair intake slip, the deposit receipt and the vouchers.

use pos_core::company;
use pos_core::customers::{self, Customer};
use pos_core::db::Db;
use pos_core::documents::{self, DocType, Document};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::ledger::{self, EntryQuery, LedgerEntry, Side};
use pos_core::money::Money;
use pos_core::restaurant::{bill, kitchen};
use pos_docs::receipt::{self, Receipt, ReceiptCustomer, ReceiptLoyalty, ReceiptSource};
use pos_docs::settlement::{
    SettledLine, SettlementReceipt, SettlementSource, build_settlement, kind_label,
};
use pos_hw::layout::Layout;
use pos_hw::{HwError, PrintOutcome, Registry};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// A receipt that did not print; `code` is translated by the UI (`errors:<code>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct PrintFailed {
    pub document_id: String,
    pub number: Option<String>,
    pub code: String,
    /// `document_id` is a versement's ledger entry (`RV…`): its DUPLICATA is
    /// `credit_reprint_receipt`, not `documents_reprint`.
    pub versement: bool,
    /// HW-002, `code` `printer_driver_only`: the Windows printer that only takes pages and its
    /// roll width; the window then prints the receipt as a page (`documents_print_receipt_roll`).
    #[serde(default)]
    #[specta(optional)]
    pub printer_name: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub paper_width_mm: Option<u16>,
    /// A slip, not a receipt, and which one. A voucher's `document_id` is its code, which
    /// `vouchers_reprint` takes (never pending a retry, no DUPLICATA); a repair intake slip or a
    /// deposit receipt is named by its own number and printed again once by
    /// `hardware_retry_print` (the queue keeps its layout); a kitchen ticket (`number` none) is
    /// sent again by `kitchen_reprint`, an addition (`document_id` its table ticket, `number`
    /// none) by `dining_ticket_prebill`.
    #[serde(default)]
    #[specta(optional)]
    pub slip: Option<PackSlip>,
}

/// The kind of a slip printed through the queue, named in its `print-failed` event so the
/// window reprints it with the right command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PackSlip {
    /// A kitchen or cancellation ticket (RST-006), laid out by the print thread for its
    /// station's printer: `kitchen_reprint(id)`.
    Kitchen,
    /// The addition of a table ticket (RST-009): `dining_ticket_prebill(ticket id)`.
    PreBill,
    /// A gift, exchange or loyalty voucher: `vouchers_reprint(code)`.
    Voucher,
    /// A repair's intake slip (`RP…`, ORD-005): `hardware_retry_print(number)`.
    Intake,
    /// A deposit's receipt (`AC…`, CRD-007): `hardware_retry_print(number)`.
    Deposit,
}

impl PackSlip {
    /// The queue keeps the slip's layout after a failure, for one `hardware_retry_print`; the
    /// other kinds have a reprint command of their own.
    fn kept_for_retry(self) -> bool {
        matches!(self, PackSlip::Intake | PackSlip::Deposit)
    }
}

pub type Notifier = Arc<dyn Fn(PrintFailed) + Send + Sync>;

/// Drops every event (tests, and the start before the window exists).
pub fn silent() -> Notifier {
    Arc::new(|_| {})
}

#[derive(Debug, Clone)]
pub struct PrintRequest {
    /// The validated document, or the customer ledger entry of a versement.
    pub document_id: String,
    /// Print the receipt; `false` with `cash_moved` opens the drawer only ("no receipt").
    pub receipt: bool,
    /// Cash went in or out: kick the drawer when the receipt options say so.
    pub cash_moved: bool,
    /// `document_id` is a versement (a `payment` of `customer_ledger`), printed as its `RV…`
    /// receipt.
    pub versement: bool,
    /// A pack slip, not a receipt (M8): which one, for the window to name and reprint it. With
    /// `Kitchen` and no `layout`, `document_id` is the kitchen ticket the print thread lays out
    /// and then marks printed.
    pub slip: Option<PackSlip>,
    /// A slip the command already laid out (the addition, a repair's intake slip, a deposit's
    /// `AC…` receipt, a voucher), printed on the receipt printer; `document_id` is then the
    /// slip's own number (the table ticket for an addition), which a failure names.
    pub layout: Option<Layout>,
}

impl PrintRequest {
    /// The receipt of the validated document `document_id`, without a drawer kick.
    pub fn receipt(document_id: &str) -> PrintRequest {
        PrintRequest {
            document_id: document_id.to_owned(),
            receipt: true,
            cash_moved: false,
            versement: false,
            slip: None,
            layout: None,
        }
    }

    /// The receipt of the versement `entry_id` and/or the drawer for its cash.
    pub fn versement(entry_id: &str, receipt: bool, cash_moved: bool) -> PrintRequest {
        PrintRequest {
            document_id: entry_id.to_owned(),
            receipt,
            cash_moved,
            versement: true,
            slip: None,
            layout: None,
        }
    }

    /// The pack slip `kind` of `id`, laid out by the print thread (a kitchen ticket).
    pub fn slip(kind: PackSlip, id: &str) -> PrintRequest {
        PrintRequest {
            document_id: id.to_owned(),
            receipt: false,
            cash_moved: false,
            versement: false,
            slip: Some(kind),
            layout: None,
        }
    }

    /// The pack slip `kind` laid out by the command, on the receipt printer, named `id` (its own
    /// number: a failure toast names it and a retry reprints it). No drawer: set `cash_moved`
    /// for money taken in cash.
    pub fn layout(kind: PackSlip, id: &str, layout: Layout) -> PrintRequest {
        PrintRequest {
            document_id: id.to_owned(),
            receipt: true,
            cash_moved: false,
            versement: false,
            slip: Some(kind),
            layout: Some(layout),
        }
    }
}

/// A hardware failure as the webview receives it: a settings field at fault is a validation
/// error on that field, anything else `AppError::Hardware` with the `pos_hw` code.
pub fn hw_error(error: HwError) -> AppError {
    match error.field() {
        Some(field) => AppError::validation(field, error.code()),
        None => AppError::Hardware {
            code: error.code().to_owned(),
        },
    }
}

/// The ticket's customer (DOC-015) and his balance after it: the `balance_after` of the entry
/// the document posted to his account among `entries` (his entries of that day), `None` when it
/// moved nothing (a cash ticket).
fn receipt_customer(
    customer: Customer,
    entries: &[LedgerEntry],
    document: &Document,
) -> (ReceiptCustomer, Option<Money>) {
    // Newest first: the latest entry of the document carries the balance it left.
    let balance = entries
        .iter()
        .find(|e| e.ref_id.as_deref() == Some(document.id.as_str()))
        .map(|e| e.balance_after);
    let printed = ReceiptCustomer {
        name: customer.name,
        code: customer.code,
        phone: customer.phone,
        address: customer.address,
    };
    (printed, balance)
}

/// The receipt of a validated document and the shop logo, in the document's language or `lang`
/// (UX-004: another language at reprint).
pub fn load_receipt(
    db: &Db,
    id: &str,
    duplicate: bool,
    lang: Option<Lang>,
) -> Result<(Receipt, Option<Vec<u8>>), AppError> {
    let id = id.to_owned();
    db.read(move |conn| {
        let mut document = documents::get(conn, &id)?;
        if let Some(lang) = lang {
            document.lang = lang;
        }
        let company = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
        let cashier = pos_core::auth::users::get(conn, &document.user_id)
            .map(|u| u.display_name)
            .unwrap_or_default();
        let terminal_code = pos_core::terminal::get(conn, &document.terminal_id)?
            .map(|t| t.code)
            .unwrap_or_default();
        let original = match (document.doc_type, &document.parent_id) {
            (DocType::ReturnNote, Some(parent)) => documents::get(conn, parent)?.number,
            _ => None,
        };
        let mut receipt = receipt::build(&ReceiptSource {
            document: &document,
            company: &company,
            cashier_name: &cashier,
            terminal_code: &terminal_code,
            original_number: original.as_deref(),
            duplicate,
        })?;
        receipt.store_url = pos_core::cloud::store::receipt_url(conn)?;
        // M8: a restaurant ticket names its table or takeaway number (RST-009), and a ticket
        // that earned points says so with the customer's balance (LOY-002).
        receipt.dining =
            bill::dining_of_document(conn, &id)?.map(|d| receipt::dining(&d, document.lang));
        if let Some(points) = pos_core::loyalty::points_of_document(conn, &id)? {
            receipt = receipt.with_loyalty(ReceiptLoyalty {
                earned: points.earned,
                balance: points.balance,
                card: points.card.unwrap_or_default(),
            });
        }
        if let Some(customer_id) = &document.customer_id {
            let customer = customers::get(conn, customer_id)?;
            let that_day = EntryQuery {
                from: Some(document.business_date.clone()),
                to: Some(document.business_date.clone()),
                limit: None,
                before: None,
            };
            let entries = ledger::entries(conn, Side::Customer, customer_id, &that_day)?.entries;
            let (printed, balance) = receipt_customer(customer, &entries, &document);
            receipt = receipt.with_customer(printed, balance);
        }
        Ok((receipt, company::logo(conn)?))
    })
}

/// The receipt of the versement `entry_id` (CRD-004): in `lang`, else the customer's language,
/// else the shop's document language, with the debts it settled and their numbers.
pub fn load_settlement(
    db: &Db,
    entry_id: &str,
    duplicate: bool,
    lang: Option<Lang>,
) -> Result<SettlementReceipt, AppError> {
    let id = entry_id.to_owned();
    db.read(move |conn| {
        let entry = ledger::get(conn, Side::Customer, &id)?;
        let customer = customers::get(conn, &entry.party_id)?;
        let company = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
        let cashier = match &entry.user_id {
            Some(user) => pos_core::auth::users::get(conn, user)
                .map(|u| u.display_name)
                .unwrap_or_default(),
            None => String::new(),
        };
        let terminal_code = match &entry.terminal_id {
            Some(terminal) => pos_core::terminal::get(conn, terminal)?
                .map(|t| t.code)
                .unwrap_or_default(),
            None => String::new(),
        };
        let lang = lang.or(customer.lang).unwrap_or(company.default_doc_lang);
        // A debt without a document (the opening balance, an adjustment) is named by its kind.
        let settled = ledger::allocations_of(conn, Side::Customer, &entry.id)?
            .into_iter()
            .filter(|a| a.credit_id == entry.id)
            .map(|a| {
                let number = match a.debit_number {
                    Some(number) => number,
                    None => kind_label(lang, ledger::get(conn, Side::Customer, &a.debit_id)?.kind),
                };
                Ok(SettledLine {
                    number,
                    amount: a.amount,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        build_settlement(&SettlementSource {
            company: &company,
            customer_name: &customer.name,
            customer_phone: customer.phone.as_deref(),
            entry: &entry,
            settled: &settled,
            cashier_name: &cashier,
            terminal_code: &terminal_code,
            lang,
            duplicate,
        })
    })
}

/// A versement receipt on the receipt printer, without a drawer kick (see [`run_versement`]).
pub fn print_settlement(
    hw: &Registry,
    receipt: &SettlementReceipt,
) -> Result<PrintOutcome, AppError> {
    hw.print_layout(&crate::slips::settlement_layout(receipt))
        .map_err(hw_error)
}

/// The shop's name in `lang`, falling back fr → ar → en (spec §5).
pub fn shop_name(db: &Db, lang: Lang) -> Result<String, AppError> {
    let company = db
        .read(company::get)?
        .ok_or_else(|| AppError::conflict("setup_required"))?;
    let preferred = match lang {
        Lang::Ar => &company.name_ar,
        Lang::Fr => &company.name_fr,
        Lang::En => &company.name_en,
    };
    Ok([
        preferred,
        &company.name_fr,
        &company.name_ar,
        &company.name_en,
    ]
    .into_iter()
    .find(|n| !n.trim().is_empty())
    .cloned()
    .unwrap_or_default())
}

/// The shop as a slip that leaves the counter prints it (a voucher), in `lang`, and whether this
/// is the demo shop.
pub fn slip_shop(db: &Db, lang: Lang) -> Result<(pos_docs::slips::SlipShop, bool), AppError> {
    let company = db
        .read(company::get)?
        .ok_or_else(|| AppError::conflict("setup_required"))?;
    let shop = pos_docs::slips::SlipShop {
        name: shop_name(db, lang)?,
        address: company.address.clone().filter(|a| !a.trim().is_empty()),
        phone: company.phone.clone().filter(|p| !p.trim().is_empty()),
    };
    Ok((shop, company.demo))
}

/// The thermal slip of `voucher` in `lang` (the demo shop's carries "DÉMO – sans valeur
/// fiscale" first, like its receipts).
pub fn voucher_slip_layout(
    db: &Db,
    voucher: &pos_core::gift_vouchers::Voucher,
    lang: Lang,
) -> Result<pos_hw::layout::Layout, AppError> {
    let (shop, demo) = slip_shop(db, lang)?;
    Ok(pos_hw::slips::with_demo(
        pos_hw::slips::voucher_layout(&pos_docs::slips::voucher_slip(voucher, &shop, lang)),
        lang,
        demo,
    ))
}

/// Documents whose original receipt failed to print and was not retried yet, with `true` for a
/// versement.
type Failed = Arc<Mutex<HashMap<String, bool>>>;

/// Laid-out slips that failed to print and were not retried yet, by job id, with their kind.
type FailedSlips = Arc<Mutex<HashMap<String, (Option<PackSlip>, Layout)>>>;

pub struct PrintQueue {
    /// `None` only while the queue is dropped.
    sender: Option<crossbeam_channel::Sender<PrintRequest>>,
    worker: Option<JoinHandle<()>>,
    failed: Failed,
    slips: FailedSlips,
}

impl std::fmt::Debug for PrintQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintQueue").finish_non_exhaustive()
    }
}

impl PrintQueue {
    /// One thread prints the jobs in order; it ends when the queue is dropped.
    pub fn start(db: Db, hw: Arc<Registry>, notify: Notifier) -> PrintQueue {
        let (sender, receiver) = crossbeam_channel::unbounded::<PrintRequest>();
        let failed: Failed = Arc::new(Mutex::new(HashMap::new()));
        let slips: FailedSlips = Arc::new(Mutex::new(HashMap::new()));
        let (seen, seen_slips) = (failed.clone(), slips.clone());
        let spawned = std::thread::Builder::new()
            .name("print-queue".into())
            .spawn(move || {
                for request in receiver {
                    run(&db, &hw, &notify, (&seen, &seen_slips), request);
                }
            });
        let worker = match spawned {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::error!(%error, "could not start the print queue; receipts need a reprint");
                None
            }
        };
        PrintQueue {
            sender: Some(sender),
            worker,
            failed,
            slips,
        }
    }

    /// Queues a receipt or a slip; `false` (logged) when the queue thread is gone — the sale (or
    /// the write the slip follows) stands anyway and has its reprint.
    pub fn enqueue(&self, request: PrintRequest) -> bool {
        let Some(sender) = &self.sender else {
            return false;
        };
        match sender.send(request) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "print queue unavailable");
                false
            }
        }
    }

    /// The original receipt of `document_id` failed to print and was not printed since: the
    /// caller may print it another way (a page through the Windows driver, HW-002), then
    /// [`forget`](Self::forget) it.
    pub fn is_pending(&self, document_id: &str) -> Result<bool, AppError> {
        Ok(self
            .failed
            .lock()
            .map_err(|_| AppError::Internal("print queue lock poisoned".into()))?
            .contains_key(document_id))
    }

    /// Prints the original again after its print failed; `print_not_pending` otherwise (use a
    /// DUPLICATA reprint).
    pub fn retry(&self, document_id: &str) -> Result<(), AppError> {
        let slip = self
            .slips
            .lock()
            .map_err(|_| AppError::Internal("print queue lock poisoned".into()))?
            .remove(document_id);
        if let Some((kind, layout)) = slip {
            let request = PrintRequest {
                document_id: document_id.to_owned(),
                receipt: true,
                cash_moved: false,
                versement: false,
                slip: kind,
                layout: Some(layout),
            };
            return if self.enqueue(request) {
                Ok(())
            } else {
                Err(AppError::Internal("print queue unavailable".into()))
            };
        }
        let was_failed = self
            .failed
            .lock()
            .map_err(|_| AppError::Internal("print queue lock poisoned".into()))?
            .remove(document_id);
        let Some(versement) = was_failed else {
            return Err(AppError::conflict("print_not_pending"));
        };
        let request = PrintRequest {
            versement,
            ..PrintRequest::receipt(document_id)
        };
        if self.enqueue(request) {
            Ok(())
        } else {
            Err(AppError::Internal("print queue unavailable".into()))
        }
    }
}

impl PrintQueue {
    /// The document was printed another way (a DUPLICATA): its original is no longer pending,
    /// so `retry` can never print an unmarked copy after it.
    pub fn forget(&self, document_id: &str) -> Result<(), AppError> {
        self.failed
            .lock()
            .map_err(|_| AppError::Internal("print queue lock poisoned".into()))?
            .remove(document_id);
        Ok(())
    }
}

impl Drop for PrintQueue {
    /// Closes the queue and waits for its thread: the thread holds a database handle, which must
    /// go with the state (the last handle checkpoints the WAL and stops the writer), not later.
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("the print queue thread panicked");
        }
    }
}

/// Opens the drawer. No printer, no drawer: a shop without a printer sells without a toast each
/// time.
fn kick(hw: &Registry) -> Result<(), AppError> {
    match hw.open_drawer() {
        Err(HwError::NotConfigured) => Ok(()),
        other => other.map(|_| ()).map_err(hw_error),
    }
}

/// A versement receipt is laid out by the app ([`crate::slips`]), so the drawer kick for its
/// cash is a job of its own, before or after the receipt as the receipt options say (always,
/// like a sale's, when no receipt is printed). A kick that fails once the receipt is out is only
/// logged: the receipt printed, so it must not be pending a retry that would print a second
/// original.
fn run_versement(db: &Db, hw: &Registry, request: &PrintRequest) -> Result<(), AppError> {
    if !request.receipt {
        return if request.cash_moved { kick(hw) } else { Ok(()) };
    }
    let options = hw.config().map_err(hw_error)?.receipt;
    let drawer = request.cash_moved && options.open_drawer_on_cash;
    if drawer && options.drawer_before_print {
        kick(hw)?;
    }
    let receipt = load_settlement(db, &request.document_id, false, None)?;
    print_settlement(hw, &receipt)?;
    if drawer
        && !options.drawer_before_print
        && let Err(error) = kick(hw)
    {
        tracing::warn!(entry = %request.document_id, %error, "drawer not opened after the versement receipt");
    }
    Ok(())
}

/// A laid-out slip and, for money taken in cash, the drawer as the receipt options say (like a
/// versement receipt's). A kick that fails once the slip is out is only logged.
fn run_layout(hw: &Registry, request: &PrintRequest, layout: &Layout) -> Result<(), AppError> {
    let options = hw.config().map_err(hw_error)?.receipt;
    let drawer = request.cash_moved && options.open_drawer_on_cash;
    if drawer && options.drawer_before_print {
        kick(hw)?;
    }
    hw.print_layout(layout).map_err(hw_error)?;
    if drawer
        && !options.drawer_before_print
        && let Err(error) = kick(hw)
    {
        tracing::warn!(job = %request.document_id, %error, "drawer not opened after the slip");
    }
    Ok(())
}

/// RST-006, HW-004: a kitchen ticket in `restaurant.kitchen_lang` on its station's printer
/// (`Registry::print_kitchen`: the station's copies and buzzer, else the kitchen fallback, else
/// the receipt printer), with the demo shop's mention, then marked printed in a write
/// of its own. A failure leaves it unprinted: `kitchen_reprint` sends it again. A station printer
/// that only takes pages is `printer_driver_only` (a kitchen ticket is never printed as a page).
fn run_kitchen(db: &Db, hw: &Registry, kitchen_id: &str) -> Result<(), AppError> {
    let id = kitchen_id.to_owned();
    let (ticket, lang, demo) = db.read(move |c| {
        let lang: Lang = pos_core::settings::get(c, "restaurant.kitchen_lang")?;
        let demo = company::get(c)?.is_some_and(|shop| shop.demo);
        Ok((kitchen::get(c, &id)?, lang, demo))
    })?;
    let layout = pos_hw::slips::with_demo(
        pos_hw::slips::kitchen_layout(&pos_docs::slips::kitchen_slip(&ticket, lang)),
        lang,
        demo,
    );
    hw.print_kitchen(ticket.station_id.as_deref(), &layout)
        .map_err(hw_error)?;
    let (ids, now) = (vec![ticket.id], pos_core::time::now_ms());
    db.write(move |tx| kitchen::mark_printed(tx, &ids, now))
        .map(drop)
}

fn run(
    db: &Db,
    hw: &Registry,
    notify: &Notifier,
    (failed, slips): (&Failed, &FailedSlips),
    request: PrintRequest,
) {
    if let Some(layout) = &request.layout {
        let Err(error) = run_layout(hw, &request, layout) else {
            return;
        };
        tracing::warn!(job = %request.document_id, slip = ?request.slip, %error, "slip not printed");
        if request.slip.is_none_or(PackSlip::kept_for_retry)
            && let Ok(mut set) = slips.lock()
        {
            set.insert(request.document_id.clone(), (request.slip, layout.clone()));
        }
        // A laid-out slip's job id is its own printed number (`RP…`, `AC…`, a voucher's code):
        // the toast names it. An addition's is its table ticket: the toast names the addition.
        let number = (request.slip != Some(PackSlip::PreBill)).then(|| request.document_id.clone());
        notify(PrintFailed {
            number,
            document_id: request.document_id,
            code: error.code(),
            versement: false,
            printer_name: None,
            paper_width_mm: None,
            slip: request.slip,
        });
        return;
    }
    // HW-002: the Windows printer that only takes pages and its roll, for the window to print
    // the receipt through it
    let mut page: Option<(String, u16)> = None;
    if request.slip == Some(PackSlip::Kitchen) {
        let Err(error) = run_kitchen(db, hw, &request.document_id) else {
            return;
        };
        tracing::warn!(kitchen = %request.document_id, %error, "kitchen ticket not printed");
        notify(PrintFailed {
            document_id: request.document_id,
            number: None,
            code: error.code(),
            versement: false,
            printer_name: None,
            paper_width_mm: None,
            slip: Some(PackSlip::Kitchen),
        });
        return;
    }
    let result = if request.versement {
        run_versement(db, hw, &request)
    } else if request.receipt {
        load_receipt(db, &request.document_id, false, None).and_then(|(receipt, logo)| {
            hw.print_receipt(&receipt, logo.as_deref(), request.cash_moved)
                .map(|_| ())
                .map_err(|error| {
                    if let HwError::DriverOnly {
                        printer_name,
                        paper_width_mm,
                    } = &error
                    {
                        page = Some((printer_name.clone(), *paper_width_mm));
                    }
                    hw_error(error)
                })
        })
    } else if request.cash_moved {
        kick(hw)
    } else {
        Ok(())
    };
    let Err(error) = result else {
        return;
    };
    tracing::warn!(document = %request.document_id, %error, "receipt not printed");
    if request.receipt
        && let Ok(mut set) = failed.lock()
    {
        set.insert(request.document_id.clone(), request.versement);
    }
    let id = request.document_id.clone();
    let number = if request.versement {
        db.read(move |c| ledger::get(c, Side::Customer, &id))
            .ok()
            .and_then(|e| e.receipt_number)
    } else {
        db.read(move |c| documents::get(c, &id))
            .ok()
            .and_then(|d| d.number)
    };
    let (printer_name, paper_width_mm) = page.unzip();
    notify(PrintFailed {
        document_id: request.document_id,
        number,
        code: error.code(),
        versement: request.versement,
        printer_name,
        paper_width_mm,
        slip: None,
    });
}

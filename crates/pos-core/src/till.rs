//! Who performs a till operation (sale, return, cash movement, closing), with which manager
//! approvals, under which regulatory pack, and when. Built by the command from the logged-in
//! `Session` (M1b); services read permissions and limits from it.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::auth::session::{OverrideGrant, Session};
use crate::datapack::Pack;
use crate::error::AppError;
use crate::money::Rate;
use crate::time;
use chrono::NaiveDate;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TillContext {
    pub session: Session,
    /// Grants obtained with `auth::session::manager_override` for this operation. The command
    /// keeps them server-side; the webview never forges one.
    pub approvals: Vec<OverrideGrant>,
    pub pack: Arc<Pack>,
    /// Unix ms UTC; the business date and fiscal year derive from it.
    pub now: i64,
    /// The hour the business day turns (`sales.day_cutoff_hour`, CFG-008): 0 = midnight.
    pub day_cutoff_hours: u8,
    /// The till this write runs for on the main PC (an M5 host service: its terminal id). The
    /// money of the write is in that till's drawer: the write books it on the till's open
    /// session, as this PC's copy has it (the till's outbox is drained before every call), and
    /// moves no drawer here; the till records the movement in its own copy from the answer
    /// ([`crate::cash::DrawerShare`], [`crate::cash::record_share`]). `None`: this PC's drawer.
    pub drawer_till: Option<String>,
    /// A till's ticket booked late on the main PC (final review): its business day, which the
    /// ledger rows and a voucher's expiry take instead of `now`'s ([`Self::at_ticket`]).
    pub ticket_day: Option<NaiveDate>,
}

impl TillContext {
    pub fn new(session: Session, pack: Arc<Pack>, now: i64) -> TillContext {
        TillContext {
            session,
            approvals: Vec::new(),
            pack,
            now,
            day_cutoff_hours: 0,
            drawer_till: None,
            ticket_day: None,
        }
    }

    /// The same context as of a till's ticket the main PC books after it reached it (final
    /// review): the ledger rows written with it take the ticket's business day, terminal and
    /// validation time, and a voucher's expiry is checked against the ticket's day, not the
    /// booking's. The audit row of the booking keeps this PC's own context.
    pub fn at_ticket(&self, ticket: &crate::documents::Document) -> Result<TillContext, AppError> {
        let day = NaiveDate::parse_from_str(&ticket.business_date, "%Y-%m-%d")
            .map_err(|_| AppError::validation("businessDate", "value_invalid"))?;
        let mut at = self.clone();
        at.now = ticket.validated_at.unwrap_or(ticket.created_at);
        at.session.terminal_id = ticket.terminal_id.clone();
        at.ticket_day = Some(day);
        Ok(at)
    }

    /// The same context with the shop's business-day cut-off (`day_cutoff`).
    pub fn with_day_cutoff(mut self, hours: u8) -> TillContext {
        self.day_cutoff_hours = hours;
        self
    }

    /// The same operation run on this PC (the main PC) for the till `terminal_id`, whose drawer
    /// holds its money ([`Self::drawer_till`]).
    pub fn for_till(mut self, terminal_id: impl Into<String>) -> TillContext {
        self.drawer_till = Some(terminal_id.into());
        self
    }

    /// The terminal whose open session takes the money of this operation.
    pub fn drawer_terminal(&self) -> &str {
        self.drawer_till
            .as_deref()
            .unwrap_or(&self.session.terminal_id)
    }

    /// This PC records the drawer movements of this operation (it is not a host service run for
    /// a till, which records them itself).
    pub fn moves_drawer(&self) -> bool {
        self.drawer_till.is_none()
    }

    pub fn with_approval(mut self, grant: OverrideGrant) -> TillContext {
        self.approvals.push(grant);
        self
    }

    /// The session holds `permission`, or a manager approved it for this operation.
    pub fn allows(&self, permission: Permission) -> bool {
        self.session.has(permission) || self.approved_by(permission).is_some()
    }

    /// The approver of `permission`, when one was given.
    pub fn approved_by(&self, permission: Permission) -> Option<&str> {
        self.approvals
            .iter()
            .find(|g| g.permission == permission)
            .map(|g| g.approver_id.as_str())
    }

    /// The largest discount this operation may give (PRM-004): the owner is unlimited; else the
    /// larger of the user's own limit and the approving manager's own limit. An approval never
    /// gives more than the approver could give themself.
    pub fn discount_limit(&self) -> Rate {
        if self.session.is_owner {
            return Rate::FULL;
        }
        let approved = self
            .approvals
            .iter()
            .filter(|g| g.permission == Permission::SalesDiscount)
            .map(|g| g.discount_limit_bp)
            .max()
            .unwrap_or(0);
        Rate(
            self.session
                .discount_limit_bp
                .max(approved)
                .clamp(0, 10_000),
        )
    }

    pub fn actor(&self) -> Actor {
        self.session.actor()
    }

    /// The business day of `now`, turning at the shop's cut-off hour.
    pub fn business_date(&self) -> NaiveDate {
        self.ticket_day
            .unwrap_or_else(|| time::business_date_with(self.now, self.day_cutoff_hours))
    }

    /// `YYYY-MM-DD` of [`Self::business_date`].
    pub fn business_day(&self) -> String {
        self.business_date().format("%Y-%m-%d").to_string()
    }

    /// Approver ids, for the audit payload of the operation.
    pub fn approvers(&self) -> Vec<(&str, &str)> {
        self.approvals
            .iter()
            .map(|g| (g.permission.as_str(), g.approver_id.as_str()))
            .collect()
    }
}

/// The shop's business-day cut-off hour (`sales.day_cutoff_hour`, 0–6; CFG-008).
pub fn day_cutoff(conn: &Connection) -> Result<u8, AppError> {
    crate::settings::get(conn, "sales.day_cutoff_hour")
}

/// The till's actions a key can be given (UX-017).
pub const KEY_ACTIONS: [&str; 17] = [
    "help",
    "search",
    "customer",
    "price_check",
    "discount",
    "hold",
    "held",
    "return",
    "drawer",
    "repeat_last",
    "checkout",
    "menu",
    "floor",
    "flexy",
    "gift_voucher",
    "serial",
    "department_keys",
];

/// The till's keys as M1b, M2 and the packs bind them. F11 is M4's scale panel and is not
/// remappable.
pub const DEFAULT_KEYS: [(&str, &str); 17] = [
    ("help", "F1"),
    ("search", "F2"),
    ("customer", "F3"),
    ("price_check", "F4"),
    ("discount", "F5"),
    ("hold", "F6"),
    ("held", "F7"),
    ("return", "F8"),
    ("drawer", "F9"),
    ("repeat_last", "F10"),
    ("checkout", "F12"),
    ("menu", "Escape"),
    ("floor", "Ctrl+1"),
    ("flexy", "Ctrl+2"),
    ("gift_voucher", "Ctrl+3"),
    ("serial", "Ctrl+4"),
    ("department_keys", "Ctrl+5"),
];

/// A key the owner may give an action: F1–F12 but F11, Ctrl+0–Ctrl+9 or Escape.
fn key_allowed(key: &str) -> bool {
    key == "Escape"
        || key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| {
                (1..=12).contains(&n) && n != 11 && key.len() == n.to_string().len() + 1
            })
        || key
            .strip_prefix("Ctrl+")
            .is_some_and(|d| d.len() == 1 && d.as_bytes()[0].is_ascii_digit())
}

/// The keys once `custom` (action → key) is laid over the defaults, in `DEFAULT_KEYS` order.
fn merged(custom: &BTreeMap<String, String>) -> Vec<(&'static str, &str)> {
    DEFAULT_KEYS
        .iter()
        .map(|(action, key)| (*action, custom.get(*action).map_or(*key, String::as_str)))
        .collect()
}

/// The first entry of `custom` that breaks the rule: an unknown action, a key not allowed, or a
/// key two actions would share once merged with the defaults.
fn key_map_fault(custom: &BTreeMap<String, String>) -> Option<(String, String)> {
    if let Some((action, key)) = custom
        .iter()
        .find(|(action, key)| !KEY_ACTIONS.contains(&action.as_str()) || !key_allowed(key))
    {
        return Some((action.clone(), key.clone()));
    }
    let mut seen = BTreeSet::new();
    let taken: Vec<&str> = merged(custom)
        .into_iter()
        .filter(|(_, key)| !seen.insert(*key))
        .map(|(_, key)| key)
        .collect();
    let key = taken.first()?;
    // Name the owner's own choice, not the default it collides with.
    custom
        .iter()
        .find(|(_, k)| k.as_str() == *key)
        .map(|(a, k)| (a.clone(), k.clone()))
}

fn as_map(v: &Value) -> Option<BTreeMap<String, String>> {
    v.as_object()?
        .iter()
        .map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
        .collect()
}

/// The validator of `till.key_map`: known actions, allowed keys, no key twice once merged.
pub(crate) fn key_map_ok(v: &Value) -> bool {
    as_map(v).is_some_and(|map| key_map_fault(&map).is_none())
}

/// The till's keys: the defaults, overridden by `till.key_map` (UX-017).
pub fn key_map(conn: &Connection) -> Result<Vec<(String, String)>, AppError> {
    let custom = as_map(&crate::settings::get_value(conn, "till.key_map")?).unwrap_or_default();
    Ok(merged(&custom)
        .into_iter()
        .map(|(action, key)| (action.to_owned(), key.to_owned()))
        .collect())
}

/// Replaces the owner's key choices (`{}` restores the defaults): refused with
/// `key_map_invalid` and `details: { action, key }` naming the first faulty choice. Journals
/// `till.key_map_set` with the keys before and after.
pub fn set_key_map(
    tx: &Transaction<'_>,
    actor: &Actor,
    map: BTreeMap<String, String>,
) -> Result<Vec<(String, String)>, AppError> {
    if let Some((action, key)) = key_map_fault(&map) {
        return Err(AppError::rejected(
            json!({ "action": action, "key": key }),
            "key_map_invalid",
        ));
    }
    let before = key_map(tx)?;
    let value = json!(map);
    crate::settings::set_quiet(tx, "till.key_map", value.clone())?;
    let after = key_map(tx)?;
    audit::append(
        tx,
        &actor.event(
            "till.key_map_set",
            "setting",
            Some("till.key_map"),
            json!({ "before": before, "after": after, "custom": value }),
        ),
    )?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::money::{Money, Qty};
    use crate::sales::cart::{Cart, CartLine, price_cart};
    use crate::tax::tva::Discount;
    use crate::testkit::{NOON, shop};

    /// A manager allowed 20 % approves a 50 % discount: still refused (PRM-004).
    #[test]
    fn an_approval_gives_no_more_than_the_approvers_own_limit() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let line = |bp: i32| {
            let mut line = CartLine::product(&milk.id, Qty(1_000));
            line.discount = Discount::Percent(Rate(bp));
            Cart::new(Lang::Fr, vec![line])
        };
        let approved = TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON)
            .with_approval(OverrideGrant {
                discount_limit_bp: 2_000,
                ..grant(Permission::SalesDiscount)
            });
        let (ctx, half) = (approved.clone(), line(5_000));
        let err = shop
            .db
            .read(move |c| price_cart(c, &ctx, &half))
            .unwrap_err();
        assert_eq!(err.code(), "discount_above_limit");
        let (ctx, fifth) = (approved, line(2_000));
        let priced = shop.db.read(move |c| price_cart(c, &ctx, &fifth)).unwrap();
        assert_eq!(priced.totals.total_ttc, Money(8_000));
    }

    #[test]
    fn the_key_map_refuses_unknown_actions_and_duplicate_keys() {
        let shop = shop();
        let set = |v: serde_json::Value| {
            let actor = shop.owner.actor();
            let map: std::collections::BTreeMap<String, String> =
                serde_json::from_value(v).unwrap();
            shop.db.write(move |tx| set_key_map(tx, &actor, map))
        };
        set(serde_json::json!({ "floor": "Ctrl+9" })).unwrap();
        let map = shop.db.read(key_map).unwrap();
        assert!(map.contains(&("floor".to_owned(), "Ctrl+9".to_owned())));
        assert!(map.contains(&("checkout".to_owned(), "F12".to_owned())));
        assert_eq!(map.len(), KEY_ACTIONS.len());
        let err = set(serde_json::json!({ "checkout": "F9" })).unwrap_err();
        assert_eq!(err.code(), "key_map_invalid", "F9 is the drawer's");
        assert_eq!(
            err.payload().details,
            Some(serde_json::json!({ "action": "checkout", "key": "F9" }))
        );
        assert_eq!(
            set(serde_json::json!({ "launch_missiles": "F11" }))
                .unwrap_err()
                .code(),
            "key_map_invalid"
        );
        assert_eq!(
            set(serde_json::json!({ "floor": "Alt+F4" }))
                .unwrap_err()
                .code(),
            "key_map_invalid"
        );
        assert_eq!(
            set(serde_json::json!({ "help": "F11" }))
                .unwrap_err()
                .code(),
            "key_map_invalid",
            "F11 stays the scale panel's"
        );
        let journaled: i64 = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'till.key_map_set'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(journaled, 1, "one row for the accepted change");
        shop.set("sales.day_cutoff_hour", serde_json::json!(3));
        let cutoff = shop.db.read(day_cutoff).unwrap();
        let ctx =
            TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON).with_day_cutoff(cutoff);
        assert_eq!(
            (ctx.day_cutoff_hours, ctx.business_day().as_str()),
            (3, "2026-09-28")
        );
    }

    fn grant(permission: Permission) -> OverrideGrant {
        OverrideGrant {
            approver_id: "manager".into(),
            approver_name: "Nadia".into(),
            permission,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        }
    }

    #[test]
    fn permissions_come_from_the_session_or_an_approval() {
        let shop = shop();
        let ctx = TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON);
        assert!(ctx.allows(Permission::SalesCreate));
        assert!(!ctx.allows(Permission::SalesPriceOverride));
        assert_eq!(ctx.discount_limit(), Rate(1_000));
        let ctx = ctx.with_approval(grant(Permission::SalesDiscount));
        assert_eq!(ctx.discount_limit(), Rate::FULL);
        assert_eq!(ctx.approved_by(Permission::SalesDiscount), Some("manager"));
        assert_eq!(ctx.approvers(), [("sales.discount", "manager")]);
        let capped = TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON).with_approval(
            OverrideGrant {
                discount_limit_bp: 2_000,
                ..grant(Permission::SalesDiscount)
            },
        );
        assert_eq!(capped.discount_limit(), Rate(2_000));
        let below = TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON).with_approval(
            OverrideGrant {
                discount_limit_bp: 500,
                ..grant(Permission::SalesDiscount)
            },
        );
        assert_eq!(
            below.discount_limit(),
            Rate(1_000),
            "never below the user's own"
        );
        let owner = TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        assert_eq!(owner.discount_limit(), Rate::FULL);
        assert_eq!(owner.business_day(), "2026-09-28");
        assert_eq!(
            owner.actor().user_id.as_deref(),
            Some(shop.owner.user_id.as_str())
        );
    }
}

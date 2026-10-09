//! Manager approvals kept in Rust (spec §4.5): `auth_manager_override` stores the
//! grant for the requester's sign-in; the next till command attaches it to its `TillContext`;
//! the next successful till write spends it. The webview never sends a grant back.

use pos_core::auth::{OverrideGrant, Session};
use pos_core::error::AppError;
use std::sync::Mutex;

/// A grant is good for one operation within two minutes.
pub const APPROVAL_TTL_MS: i64 = 120_000;

#[derive(Debug, Clone)]
struct Pending {
    user_id: String,
    started_at: i64,
    grant: OverrideGrant,
}

impl Pending {
    fn of(&self, session: &Session) -> bool {
        self.user_id == session.user_id && self.started_at == session.started_at
    }
}

#[derive(Debug, Default)]
pub struct Approvals(Mutex<Vec<Pending>>);

fn poisoned() -> AppError {
    AppError::Internal("approvals lock poisoned".into())
}

impl Approvals {
    /// Keeps `grant` for `session`'s sign-in; a newer grant of the same permission replaces it.
    pub fn add(&self, session: &Session, grant: OverrideGrant) -> Result<(), AppError> {
        let mut all = self.0.lock().map_err(|_| poisoned())?;
        all.retain(|p| !(p.of(session) && p.grant.permission == grant.permission));
        all.push(Pending {
            user_id: session.user_id.clone(),
            started_at: session.started_at,
            grant,
        });
        Ok(())
    }

    /// The live grants of this sign-in. Expired ones — older than the TTL, or dated in the
    /// future after a clock change — are dropped for good.
    pub fn pending(&self, session: &Session, now: i64) -> Result<Vec<OverrideGrant>, AppError> {
        let mut all = self.0.lock().map_err(|_| poisoned())?;
        all.retain(|p| {
            let age = now.saturating_sub(p.grant.granted_at);
            (0..=APPROVAL_TTL_MS).contains(&age)
        });
        Ok(all
            .iter()
            .filter(|p| p.of(session))
            .map(|p| p.grant.clone())
            .collect())
    }

    /// The operation these grants were for succeeded.
    pub fn spend(&self, session: &Session) -> Result<(), AppError> {
        let mut all = self.0.lock().map_err(|_| poisoned())?;
        all.retain(|p| !p.of(session));
        Ok(())
    }

    /// Every login and logout.
    pub fn clear(&self) -> Result<(), AppError> {
        self.0.lock().map_err(|_| poisoned())?.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::auth::Permission;
    use pos_core::lang::Lang;

    fn session(user: &str, started_at: i64) -> Session {
        Session {
            user_id: user.into(),
            username: user.into(),
            display_name: user.into(),
            role_id: "r".into(),
            role_name_ar: String::new(),
            role_name_fr: String::new(),
            role_name_en: String::new(),
            is_owner: false,
            lang: Lang::Fr,
            permissions: Vec::new(),
            discount_limit_bp: 0,
            terminal_id: "t".into(),
            terminal_code: "C1".into(),
            started_at,
        }
    }

    fn grant(permission: Permission, at: i64) -> OverrideGrant {
        OverrideGrant {
            approver_id: "u-owner".into(),
            approver_name: "Amine".into(),
            permission,
            granted_at: at,
            discount_limit_bp: 10_000,
        }
    }

    #[test]
    fn a_grant_belongs_to_one_sign_in_and_expires() {
        let approvals = Approvals::default();
        let yacine = session("u-yacine", 1_000);
        approvals
            .add(&yacine, grant(Permission::SalesDiscount, 10_000))
            .unwrap();
        assert_eq!(
            approvals
                .pending(&yacine, 10_000 + APPROVAL_TTL_MS)
                .unwrap()
                .len(),
            1
        );
        assert!(
            approvals
                .pending(&session("u-yacine", 2_000), 10_000)
                .unwrap()
                .is_empty(),
            "a later sign-in of the same user"
        );
        assert!(
            approvals
                .pending(&session("u-other", 1_000), 10_000)
                .unwrap()
                .is_empty(),
            "another cashier"
        );
        assert!(
            approvals
                .pending(&yacine, 10_000 + APPROVAL_TTL_MS + 1)
                .unwrap()
                .is_empty(),
            "expired"
        );
        assert!(
            approvals.pending(&yacine, 10_000).unwrap().is_empty(),
            "expired grants are dropped for good"
        );
    }

    #[test]
    fn a_clock_moved_back_does_not_extend_a_grant() {
        let approvals = Approvals::default();
        let yacine = session("u-yacine", 1_000);
        approvals
            .add(&yacine, grant(Permission::SalesVoidLine, 50_000))
            .unwrap();
        assert!(approvals.pending(&yacine, 40_000).unwrap().is_empty());
    }

    #[test]
    fn a_new_grant_replaces_the_old_one_and_spending_clears_the_sign_in() {
        let approvals = Approvals::default();
        let yacine = session("u-yacine", 1_000);
        let other = session("u-other", 1_000);
        approvals
            .add(&yacine, grant(Permission::SalesDiscount, 10_000))
            .unwrap();
        approvals
            .add(&yacine, grant(Permission::SalesDiscount, 11_000))
            .unwrap();
        approvals
            .add(&other, grant(Permission::SalesReturn, 11_000))
            .unwrap();
        let pending = approvals.pending(&yacine, 11_000).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].granted_at, 11_000);
        approvals.spend(&yacine).unwrap();
        assert!(approvals.pending(&yacine, 11_000).unwrap().is_empty());
        assert_eq!(
            approvals.pending(&other, 11_000).unwrap().len(),
            1,
            "only that sign-in's grants"
        );
        approvals.clear().unwrap();
        assert!(approvals.pending(&other, 11_000).unwrap().is_empty());
    }
}

//! Local authentication and authorisation (spec §4.5).

#[cfg(test)]
mod grant_tests;
pub mod permission;
pub mod pin;
pub mod roles;
pub mod session;
pub mod users;

pub use permission::{Permission, PermissionGroup};
pub use pin::{hash_pin, validate_pin, verify_pin};
pub use roles::{
    CASHIER_ROLE_ID, MANAGER_ROLE_ID, OWNER_ROLE_ID, PermissionGroupInfo, Role, RoleInput,
    STOREKEEPER_ROLE_ID,
};
pub use session::{
    LOCKOUT_MS, MAX_FAILED_ATTEMPTS, OverrideGrant, Session, build_session, change_language,
    confirm_grant, confirm_pin, login, login_at, logout, manager_override, refresh, require,
};
pub use users::{NewUser, User, UserTile, UserUpdate};

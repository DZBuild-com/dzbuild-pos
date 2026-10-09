//! What travels between a till and the main PC: JSON bodies, `x-dzpos-*` headers, and errors
//! as `{ code, message?, field?, details? }` with an HTTP status per kind.

use pos_core::error::{AppError, ErrorPayload};
use pos_core::sync::outbox::{ItemResult, WireItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

pub const PORT: u16 = 7345;
pub const H_VERSION: &str = "x-dzpos-version";
pub const H_SCHEMA: &str = "x-dzpos-schema";
pub const H_PENDING: &str = "x-dzpos-pending";
pub const H_SKEW: &str = "x-dzpos-skew";
pub const H_HOST_TIME: &str = "x-dzpos-host-time";
pub const H_SEQ: &str = "x-dzpos-seq";
pub const H_SHA256: &str = "x-dzpos-sha256";

pub const MAX_REPLAY_ITEMS: usize = 100;
pub const MAX_BODY: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayRequest {
    pub items: Vec<WireItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayResponse {
    pub results: Vec<ItemResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRequest {
    pub user_id: String,
    pub input: Value,
    /// The manager approvals pending for that user's sign-in on the till (none from a till of
    /// an older version).
    #[serde(default)]
    pub approvals: Vec<RpcGrant>,
    /// The till's key for a write that moves money (final review): the main PC answers a key it
    /// already answered from what it answered then, so a call sent again after a lost answer
    /// never writes twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// A manager's approval the till's user holds for a host service (M1a's
/// `auth_manager_override`, the approver's PIN checked on the till; Task 20 review). The main PC
/// uses it only once it has confirmed the approver against its own users and roles
/// (`pos_core::auth::confirm_grant`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcGrant {
    pub approver_id: String,
    pub permission: pos_core::auth::Permission,
    pub granted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponse {
    pub output: Value,
    pub host_seq: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedMessage {
    pub head: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HealthInfo {
    pub host_id: String,
    pub shop_name: String,
    pub app_version: String,
    pub schema_version: i64,
    pub host_time: i64,
    pub pairing_open: bool,
    pub suggested_code: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairResponse {
    pub host_id: String,
    pub shop_name: String,
    pub terminal: pos_core::terminal::Terminal,
    pub token: String,
    pub host_time: i64,
}

impl std::fmt::Debug for PairResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairResponse")
            .field("host_id", &self.host_id)
            .field("terminal", &self.terminal)
            .field("token", &"<hidden>")
            .finish_non_exhaustive()
    }
}

pub fn status_of(error: &AppError) -> u16 {
    match error {
        AppError::Auth { .. } => 401,
        AppError::Permission { .. } => 403,
        AppError::NotFound { .. } => 404,
        AppError::Conflict { code } if code.ends_with("rate_limited") => 429,
        AppError::Conflict { .. } | AppError::Rejected { .. } => 409,
        AppError::Validation { .. } => 422,
        AppError::Sync { .. } => 503,
        AppError::Db(_) | AppError::Io(_) | AppError::Hardware { .. } | AppError::Internal(_) => {
            500
        }
    }
}

/// The body of an error answer: the code, field and details; the developer message stays in
/// the main PC's log (it may name a file or a statement).
pub fn error_body(error: &AppError) -> ErrorPayload {
    ErrorPayload {
        message: None,
        ..error.payload()
    }
}

/// Back from an error body on the till.
pub fn from_body(status: u16, body: ErrorPayload) -> AppError {
    match status {
        401 => AppError::auth(&body.code),
        404 => AppError::conflict(&body.code),
        422 => AppError::validation(body.field.as_deref().unwrap_or(""), &body.code),
        _ => match body.details {
            Some(details) => AppError::rejected(details, &body.code),
            None => AppError::conflict(&body.code),
        },
    }
}

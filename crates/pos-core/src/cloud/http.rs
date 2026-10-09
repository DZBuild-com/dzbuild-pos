//! The HTTP transport to DZBuild (spec §4.7, research 06 §3.1.5 and §5.4): one blocking
//! `reqwest` client over the Windows trust store, the API envelope, cursor pages, a sliding 60 s
//! window per install (at most 120 calls in any minute, of which the sync takes at most 100 so 20
//! stay free for the till's own questions), retries with backoff on network errors, 5xx and 429
//! only — never another 4xx, and never a keyless POST that may have reached DZBuild —,
//! `Idempotency-Key` on every API write, and logs that name the endpoint and never carry a header,
//! a body, a query or a token.
//!
//! Blocking by design: callers are the engine threads and `commands::blocking`. Never build, use
//! or drop an `Http` inside an async task.

use super::config::CloudConfig;
use crate::error::AppError;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

pub const REQUESTS_PER_MINUTE: u32 = 120;
pub const INTERACTIVE_RESERVE: u32 = 20;
pub const MEDIA_PER_MINUTE: u32 = 10;
/// The client's timeout. An answer must start within it, and so must each read of a body
/// streamed by [`Http::send_to`]. reqwest's blocking client applies it to every read on its own,
/// so a slow line goes through and only a stall ends the download.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// One read of a streamed body.
const STREAM_CHUNK: usize = 64 * 1024;

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        crate::time::now_ms()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A clock moved by hand: `sleep` advances it and records the pause (tests).
#[derive(Debug)]
pub struct ManualClock {
    now: Mutex<i64>,
    slept: Mutex<Vec<Duration>>,
}

impl ManualClock {
    pub fn at(now_ms: i64) -> Arc<ManualClock> {
        Arc::new(ManualClock {
            now: Mutex::new(now_ms),
            slept: Mutex::new(Vec::new()),
        })
    }

    pub fn advance(&self, ms: i64) {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner) += ms;
    }

    pub fn slept(&self) -> Vec<Duration> {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn sleep(&self, duration: Duration) {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
        self.advance(i64::try_from(duration.as_millis()).unwrap_or(i64::MAX));
    }
}

/// Who is asking: the till waiting on screen, the background sync, or a photo upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Interactive,
    Background,
    Media,
}

/// The window DZBuild counts requests in (research 06 §3.1.5: fixed calendar minutes).
const WINDOW_MS: i64 = 60_000;

/// A sliding 60 s log of send times on the injected clock. DZBuild counts `per_minute` calls
/// per install in fixed calendar minutes, and every calendar minute is a 60 s window: keeping
/// every sliding window at `per_minute` keeps every calendar minute there too, whatever the
/// skew between the two clocks. Background and media calls stop at `per_minute - reserve` in the
/// window, so the till's `reserve` is still free on DZBuild's side when it asks (research 06
/// §5.4: about 100 of the 120 for the sync); media calls also stop at their own cap.
pub struct RateLimiter {
    total: usize,
    background: usize,
    media: usize,
    clock: Arc<dyn Clock>,
    sent: Mutex<VecDeque<(i64, Lane)>>,
}

fn count(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX).max(1)
}

impl RateLimiter {
    pub fn new(per_minute: u32, reserve: u32, clock: Arc<dyn Clock>) -> RateLimiter {
        let total = count(per_minute);
        let background = count(per_minute.saturating_sub(reserve));
        RateLimiter {
            total,
            background,
            media: background,
            clock,
            sent: Mutex::new(VecDeque::with_capacity(total)),
        }
    }

    /// Caps `Lane::Media` calls at `per_minute` in the window, inside the background share.
    pub fn with_media(mut self, per_minute: u32) -> RateLimiter {
        self.media = count(per_minute).min(self.background);
        self
    }

    /// Records one call for `lane`, sleeping (on the injected clock) until the window has room.
    pub fn take(&self, lane: Lane) {
        while let Some(wait) = self.try_take(lane) {
            self.clock.sleep(wait);
        }
    }

    fn try_take(&self, lane: Lane) -> Option<Duration> {
        let mut sent = self.sent.lock().unwrap_or_else(PoisonError::into_inner);
        let now = self.clock.now_ms();
        // A clock set back: calls stamped in its future count as sent now, never for longer.
        for entry in sent.iter_mut().rev() {
            if entry.0 <= now {
                break;
            }
            entry.0 = now;
        }
        while sent.front().is_some_and(|&(at, _)| at <= now - WINDOW_MS) {
            sent.pop_front();
        }
        let background = |l: Lane| l != Lane::Interactive;
        let limits = [
            Some(room_at(&sent, self.total, |_| true)),
            background(lane).then(|| room_at(&sent, self.background, background)),
            (lane == Lane::Media).then(|| room_at(&sent, self.media, |l| l == Lane::Media)),
        ];
        match limits.into_iter().flatten().flatten().max() {
            None => {
                sent.push_back((now, lane));
                None
            }
            Some(at) => Some(Duration::from_millis(
                u64::try_from(at - now).unwrap_or(1).max(1),
            )),
        }
    }
}

/// `None` while fewer than `cap` of the calls `counted` are in the window; else the time the
/// oldest one that has to leave for a new call to fit leaves it.
fn room_at(
    sent: &VecDeque<(i64, Lane)>,
    cap: usize,
    counted: impl Fn(Lane) -> bool,
) -> Option<i64> {
    let n = sent.iter().filter(|e| counted(e.1)).count();
    if n < cap {
        return None;
    }
    sent.iter()
        .filter(|e| counted(e.1))
        .nth(n - cap)
        .map(|e| e.0 + WINDOW_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub first: Duration,
    pub cap: Duration,
}

impl RetryPolicy {
    pub const BACKGROUND: RetryPolicy = RetryPolicy {
        attempts: 4,
        first: Duration::from_secs(1),
        cap: Duration::from_secs(30),
    };
    /// The till waits on screen: one quick retry, never a long `Retry-After`.
    pub const INTERACTIVE: RetryPolicy = RetryPolicy {
        attempts: 2,
        first: Duration::from_millis(500),
        cap: Duration::from_secs(2),
    };

    /// The pause before retry `n` (1-based): `Retry-After` when given, else doubling from
    /// `first`, capped. `None` when `Retry-After` is longer than this lane may wait.
    fn delay(&self, n: u32, retry_after: Option<Duration>) -> Option<Duration> {
        match retry_after {
            Some(after) if after > self.cap.max(Duration::from_secs(120)) => None,
            Some(after) if self == &RetryPolicy::INTERACTIVE && after > self.cap => None,
            Some(after) => Some(after),
            None => Some(
                self.first
                    .saturating_mul(2_u32.saturating_pow(n.saturating_sub(1)))
                    .min(self.cap),
            ),
        }
    }
}

/// Every DZBuild call the POS makes. `is_brief` marks the endpoints of the Backend Brief that
/// DZBuild may not have shipped yet: a bare 404 or a 501 on them means "not available yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Discovery,
    Token,
    DeviceAuthorization,
    Revoke,
    UserInfo,
    Me,
    Store,
    Devices,
    DeviceHeartbeat,
    DeviceDelete,
    Locations,
    ProductsBatch,
    Products,
    ProductGet,
    ProductImages,
    MediaUploads,
    InventoryAdjustments,
    InventoryLevels,
    PosSales,
    PosRefunds,
    PosClosures,
    Events,
    Orders,
    OrderGet,
    OrderClaim,
    OrderUpdate,
    OrderCancel,
    Customers,
    Upload,
}

impl Endpoint {
    pub const fn name(self) -> &'static str {
        match self {
            Endpoint::Discovery => "oauth.discovery",
            Endpoint::Token => "oauth.token",
            Endpoint::DeviceAuthorization => "oauth.device",
            Endpoint::Revoke => "oauth.revoke",
            Endpoint::UserInfo => "oauth.userinfo",
            Endpoint::Me => "me",
            Endpoint::Store => "store",
            Endpoint::Devices => "devices",
            Endpoint::DeviceHeartbeat => "devices.heartbeat",
            Endpoint::DeviceDelete => "devices.delete",
            Endpoint::Locations => "locations",
            Endpoint::ProductsBatch => "products.batch",
            Endpoint::Products => "products",
            Endpoint::ProductGet => "products.get",
            Endpoint::ProductImages => "products.images",
            Endpoint::MediaUploads => "media.uploads",
            Endpoint::InventoryAdjustments => "inventory.adjustments",
            Endpoint::InventoryLevels => "inventory.levels",
            Endpoint::PosSales => "pos.sales",
            Endpoint::PosRefunds => "pos.refunds",
            Endpoint::PosClosures => "pos.closures",
            Endpoint::Events => "events",
            Endpoint::Orders => "orders",
            Endpoint::OrderGet => "orders.get",
            Endpoint::OrderClaim => "orders.claim",
            Endpoint::OrderUpdate => "orders.update",
            Endpoint::OrderCancel => "orders.cancel",
            Endpoint::Customers => "customers",
            Endpoint::Upload => "upload",
        }
    }

    /// Endpoints the Backend Brief asks DZBuild to add (research 06 §6).
    pub const fn is_brief(self) -> bool {
        !matches!(
            self,
            Endpoint::Discovery
                | Endpoint::Token
                | Endpoint::Revoke
                | Endpoint::UserInfo
                | Endpoint::ProductGet
                | Endpoint::OrderGet
                | Endpoint::OrderUpdate
                | Endpoint::OrderCancel
                | Endpoint::Customers
                | Endpoint::Upload
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Body {
    None,
    Json(Vec<u8>),
    Form(Vec<(String, String)>),
    Bytes(Vec<u8>, String),
}

/// One request. `path` is relative to `api_base` unless `absolute` replaced it.
#[derive(Clone)]
pub struct Call {
    verb: Verb,
    endpoint: Endpoint,
    url: CallUrl,
    query: Vec<(String, String)>,
    body: Body,
    key: Option<String>,
    bearer: Option<String>,
    store: Option<String>,
    lane: Lane,
    timeout: Option<Duration>,
}

#[derive(Clone)]
enum CallUrl {
    Api(String),
    Absolute(String),
}

impl std::fmt::Debug for Call {
    /// Never prints the bearer, the body, the query or the key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("verb", &self.verb)
            .field("endpoint", &self.endpoint.name())
            .field("lane", &self.lane)
            .finish_non_exhaustive()
    }
}

impl Call {
    fn new(verb: Verb, endpoint: Endpoint, path: &str) -> Call {
        Call {
            verb,
            endpoint,
            url: CallUrl::Api(path.to_owned()),
            query: Vec::new(),
            body: Body::None,
            key: None,
            bearer: None,
            store: None,
            lane: Lane::Background,
            timeout: None,
        }
    }
    pub fn get(endpoint: Endpoint, path: &str) -> Call {
        Call::new(Verb::Get, endpoint, path)
    }
    pub fn post(endpoint: Endpoint, path: &str) -> Call {
        Call::new(Verb::Post, endpoint, path)
    }
    pub fn put(endpoint: Endpoint, path: &str) -> Call {
        Call::new(Verb::Put, endpoint, path)
    }
    pub fn patch(endpoint: Endpoint, path: &str) -> Call {
        Call::new(Verb::Patch, endpoint, path)
    }
    pub fn delete(endpoint: Endpoint, path: &str) -> Call {
        Call::new(Verb::Delete, endpoint, path)
    }
    /// A full URL (OAuth endpoints from discovery, pre-signed upload and download URLs).
    pub fn absolute(mut self, url: &str) -> Call {
        self.url = CallUrl::Absolute(url.to_owned());
        self
    }
    pub fn query(mut self, key: &str, value: impl Into<String>) -> Call {
        self.query.push((key.to_owned(), value.into()));
        self
    }
    pub fn json_body(mut self, body: Vec<u8>) -> Call {
        self.body = Body::Json(body);
        self
    }
    pub fn form(mut self, pairs: Vec<(String, String)>) -> Call {
        self.body = Body::Form(pairs);
        self
    }
    pub fn bytes(mut self, body: Vec<u8>, content_type: &str) -> Call {
        self.body = Body::Bytes(body, content_type.to_owned());
        self
    }
    pub fn key(mut self, key: &str) -> Call {
        self.key = Some(key.to_owned());
        self
    }
    pub fn bearer(mut self, token: &str) -> Call {
        self.bearer = Some(token.to_owned());
        self
    }
    pub fn store(mut self, store_id: &str) -> Call {
        self.store = Some(store_id.to_owned());
        self
    }
    pub fn lane(mut self, lane: Lane) -> Call {
        self.lane = lane;
        self
    }
    /// A total deadline, the body included. [`Http::send_to`] ignores it: a streamed body is
    /// bounded read by read ([`STALL_TIMEOUT`]).
    pub fn timeout(mut self, timeout: Duration) -> Call {
        self.timeout = Some(timeout);
        self
    }
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint
    }
    pub fn idempotency_key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// The same call with a fresh access token (the one retry after a 401).
    pub fn with_bearer(&self, token: &str) -> Call {
        self.clone().bearer(token)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    /// `Idempotency-Replay: 1`: DZBuild answered from its 24 h store.
    pub replayed: bool,
    pub request_id: Option<String>,
    pub retry_after: Option<Duration>,
    /// A storage host's `ETag` (a multipart part's receipt).
    pub etag: Option<String>,
}

impl std::fmt::Debug for Reply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("status", &self.status)
            .field("bytes", &self.body.len())
            .field("replayed", &self.replayed)
            .field("has_etag", &self.etag.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    pub data: T,
}

/// A cursor page (`limit` ≤ 200).
#[derive(Debug, Clone, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub details: Option<Value>,
}

fn parse_error(what: &str, e: &serde_json::Error) -> CallError {
    // Category and position only: serde's text can quote a value, and a value can be a token.
    CallError::Invalid(format!("{what}: {:?} at line {}", e.classify(), e.line()))
}

impl Reply {
    /// The `data` of the API envelope.
    pub fn data<T: DeserializeOwned>(&self) -> Result<T, CallError> {
        serde_json::from_slice::<Envelope<T>>(&self.body)
            .map(|e| e.data)
            .map_err(|e| parse_error("envelope", &e))
    }

    /// A plain JSON body (OAuth endpoints have no envelope).
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, CallError> {
        serde_json::from_slice(&self.body).map_err(|e| parse_error("json", &e))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// No answer: DNS, connection, TLS or timeout. Messages hold no URL or header.
    Offline(String),
    /// 5xx (not 501) or 429 after the retries.
    Server {
        status: u16,
        retry_after: Option<Duration>,
    },
    Unauthorized,
    /// The refresh token was refused (`invalid_grant`, reuse detection): link again.
    Relink,
    /// A Brief endpoint DZBuild has not shipped yet (bare 404, `route_not_found`, 501).
    NotReady {
        endpoint: Endpoint,
        status: u16,
    },
    /// The store's plan refuses (402/403 with a plan code).
    PlanCap {
        code: String,
    },
    Rejected {
        status: u16,
        code: String,
        message: Option<String>,
        details: Option<Value>,
    },
    Invalid(String),
    /// The sink of a streamed body refused a write, for example a full disk. This is not
    /// DZBuild's fault and is never retried.
    Sink(String),
}

const ROUTE_MISSING: &[&str] = &["route_not_found", "unknown_endpoint", "not_implemented"];
const PLAN_CODES: &[&str] = &[
    "plan_limit",
    "plan_limit_reached",
    "product_limit_reached",
    "quota_exceeded",
    "app_plan_required",
];

impl CallError {
    pub fn code(&self) -> &'static str {
        match self {
            CallError::Offline(_) => "cloud_unavailable",
            CallError::Server { status: 429, .. } => "cloud_rate_limited",
            CallError::Server { .. } => "cloud_unavailable",
            CallError::Unauthorized => "cloud_unauthorized",
            CallError::Relink => "cloud_relink_required",
            CallError::NotReady { .. } => "cloud_not_ready",
            CallError::PlanCap { .. } => "cloud_plan_cap",
            CallError::Rejected { .. } => "cloud_request_invalid",
            CallError::Invalid(_) => "cloud_response_invalid",
            CallError::Sink(_) => "io_error",
        }
    }

    pub fn to_app(&self) -> AppError {
        match self {
            CallError::Offline(_) => AppError::sync("cloud_unavailable"),
            CallError::Server { status: 429, .. } => AppError::sync("cloud_rate_limited"),
            CallError::Server { .. } => AppError::sync("cloud_unavailable"),
            CallError::Unauthorized => AppError::sync("cloud_unauthorized"),
            CallError::Relink => AppError::sync("cloud_relink_required"),
            CallError::NotReady { .. } => AppError::sync("cloud_not_ready"),
            CallError::PlanCap { .. } => AppError::sync("cloud_plan_cap"),
            CallError::Rejected {
                status,
                code,
                message,
                details,
            } => AppError::rejected(
                json!({ "status": status, "code": code, "message": message, "details": details }),
                "cloud_request_invalid",
            ),
            CallError::Invalid(_) => AppError::sync("cloud_response_invalid"),
            CallError::Sink(message) => AppError::Io(message.clone()),
        }
    }

    /// Worth retrying later without any change: the network, 5xx, 429, an unreadable answer.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            CallError::Offline(_) | CallError::Server { .. } | CallError::Invalid(_)
        )
    }

    pub fn is_offline(&self) -> bool {
        matches!(self, CallError::Offline(_))
    }
}

/// `{"error":{"code",…}}` (API) or `{"error":"invalid_grant",…}` (OAuth, RFC 6749 §5.2).
fn error_body(body: &[u8]) -> Option<ApiErrorBody> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match value.get("error")? {
        Value::String(code) => Some(ApiErrorBody {
            code: code.clone(),
            message: value
                .get("error_description")
                .and_then(Value::as_str)
                .map(str::to_owned),
            details: None,
        }),
        object @ Value::Object(_) => serde_json::from_value(object.clone()).ok(),
        _ => None,
    }
}

fn classify(endpoint: Endpoint, reply: &Reply) -> CallError {
    let body = error_body(&reply.body);
    let code = body.as_ref().map(|b| b.code.as_str());
    match reply.status {
        401 => CallError::Unauthorized,
        429 => CallError::Server {
            status: 429,
            retry_after: reply.retry_after,
        },
        501 => CallError::NotReady {
            endpoint,
            status: 501,
        },
        404 if endpoint.is_brief() && code.is_none_or(|c| ROUTE_MISSING.contains(&c)) => {
            CallError::NotReady {
                endpoint,
                status: 404,
            }
        }
        402 | 403 if code.is_some_and(|c| PLAN_CODES.contains(&c)) => CallError::PlanCap {
            code: code.unwrap_or_default().to_owned(),
        },
        s if s >= 500 => CallError::Server {
            status: s,
            retry_after: reply.retry_after,
        },
        s => CallError::Rejected {
            status: s,
            code: code.map_or_else(|| format!("http_{s}"), str::to_owned),
            message: body.as_ref().and_then(|b| b.message.clone()),
            details: body.and_then(|b| b.details),
        },
    }
}

/// Seconds, or an HTTP date.
fn retry_after(value: &str, now_ms: i64) -> Option<Duration> {
    if let Ok(secs) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value.trim()).ok()?;
    let ms = (at.timestamp_millis() - now_ms).max(0);
    Some(Duration::from_millis(u64::try_from(ms).ok()?))
}

pub struct Http {
    client: reqwest::blocking::Client,
    api_base: String,
    store_header: String,
    limiter: RateLimiter,
    clock: Arc<dyn Clock>,
    /// Calls sent, and calls DZBuild answered with a success (`counts`).
    calls: AtomicU64,
    answered: AtomicU64,
}

impl Http {
    /// A client that connects within 10 s and whose answers must start (and each read of a
    /// streamed body come) within [`STALL_TIMEOUT`].
    pub fn new(
        config: &CloudConfig,
        clock: Arc<dyn Clock>,
        user_agent: String,
    ) -> Result<Http, AppError> {
        Http::with_timeouts(
            config,
            clock,
            user_agent,
            Duration::from_secs(10),
            STALL_TIMEOUT,
        )
    }

    /// [`Http::new`] with other timeouts: `connect` for the connection, `request` for the
    /// client's own timeout (tests: a stall or a lost answer in a fraction of a second).
    pub fn with_timeouts(
        config: &CloudConfig,
        clock: Arc<dyn Clock>,
        user_agent: String,
        connect: Duration,
        request: Duration,
    ) -> Result<Http, AppError> {
        config.validate()?;
        let client = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(connect)
            .timeout(request)
            .build()
            .map_err(|e| {
                tracing::error!(target: "cloud", kind = ?e.is_builder(), "http client could not start");
                AppError::sync("cloud_unavailable")
            })?;
        Ok(Http {
            client,
            api_base: config.api_base.trim_end_matches('/').to_owned(),
            store_header: config.store_header.clone(),
            limiter: RateLimiter::new(REQUESTS_PER_MINUTE, INTERACTIVE_RESERVE, clock.clone())
                .with_media(MEDIA_PER_MINUTE),
            clock,
            calls: AtomicU64::new(0),
            answered: AtomicU64::new(0),
        })
    }

    pub fn clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    /// Calls sent so far, and how many DZBuild answered with a success: a sync cycle whose
    /// calls all failed is no sync (the engine compares them before and after a cycle).
    pub fn counts(&self) -> (u64, u64) {
        (
            self.calls.load(Ordering::Relaxed),
            self.answered.load(Ordering::Relaxed),
        )
    }

    /// Sends `call`, retrying network errors, 5xx and 429 per the lane's policy, and only
    /// when a replay is safe (see [`may_retry`]). An API write without an `Idempotency-Key`
    /// never leaves the PC.
    pub fn send(&self, call: &Call) -> Result<Reply, CallError> {
        self.exchange(call, None)
    }

    /// [`Http::send`] for a large download, such as a backup.
    ///
    /// - A body under 400 is copied to `sink` as it arrives and never held in memory, so
    ///   `Reply.body` is empty.
    /// - There is no total deadline, since `call`'s own `timeout` is ignored. Only a read that
    ///   waits longer than the stall timeout ends the call.
    /// - A failure once part of the body reached `sink` is never retried, because `sink` already
    ///   holds that part. The caller discards what was written.
    /// - An error body is still read into memory for its code.
    pub fn send_to(&self, call: &Call, sink: &mut dyn Write) -> Result<Reply, CallError> {
        self.exchange(call, Some(sink))
    }

    fn exchange(
        &self,
        call: &Call,
        mut sink: Option<&mut (dyn Write + '_)>,
    ) -> Result<Reply, CallError> {
        if call.key.is_none()
            && matches!(call.url, CallUrl::Api(_))
            && matches!(call.verb, Verb::Post | Verb::Patch | Verb::Delete)
        {
            tracing::error!(target: "cloud", endpoint = call.endpoint.name(), "dzbuild write without an idempotency key");
            return Err(CallError::Invalid(format!(
                "{}: a write needs an idempotency key",
                call.endpoint.name()
            )));
        }
        let policy = if call.lane == Lane::Interactive {
            RetryPolicy::INTERACTIVE
        } else {
            RetryPolicy::BACKGROUND
        };
        self.calls.fetch_add(1, Ordering::Relaxed);
        let mut retry = 0;
        loop {
            self.limiter.take(call.lane);
            let started = Instant::now();
            let outcome = self.once(call, sink.as_deref_mut());
            let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let error = match outcome {
                Ok(reply) if reply.status < 400 => {
                    tracing::debug!(target: "cloud", endpoint = call.endpoint.name(), status = reply.status,
                        replayed = reply.replayed, request_id = reply.request_id.as_deref(), ms, "dzbuild call");
                    self.answered.fetch_add(1, Ordering::Relaxed);
                    return Ok(reply);
                }
                Ok(reply) => {
                    let error = classify(call.endpoint, &reply);
                    tracing::warn!(target: "cloud", endpoint = call.endpoint.name(), status = reply.status,
                        code = error.code(), request_id = reply.request_id.as_deref(), ms, "dzbuild call refused");
                    error
                }
                Err(Failure::Unreachable(message)) => {
                    tracing::warn!(target: "cloud", endpoint = call.endpoint.name(), reason = %message, ms, "dzbuild unreachable");
                    CallError::Offline(message)
                }
                Err(Failure::Broken(message)) => {
                    tracing::warn!(target: "cloud", endpoint = call.endpoint.name(), reason = %message, ms, "dzbuild download broke off");
                    return Err(CallError::Offline(message));
                }
                Err(Failure::Sink(message)) => {
                    tracing::warn!(target: "cloud", endpoint = call.endpoint.name(), reason = %message, ms, "dzbuild download not written");
                    return Err(CallError::Sink(message));
                }
            };
            retry += 1;
            let wait = match &error {
                CallError::Server { retry_after, .. } => *retry_after,
                _ => None,
            };
            match (
                may_retry(call, &error) && retry < policy.attempts,
                policy.delay(retry, wait),
            ) {
                (true, Some(pause)) => self.clock.sleep(pause),
                _ => return Err(error),
            }
        }
    }

    fn once(&self, call: &Call, sink: Option<&mut (dyn Write + '_)>) -> Result<Reply, Failure> {
        let url = match &call.url {
            CallUrl::Api(path) => format!("{}{path}", self.api_base),
            CallUrl::Absolute(url) => url.clone(),
        };
        let mut request = match call.verb {
            Verb::Get => self.client.get(&url),
            Verb::Post => self.client.post(&url),
            Verb::Put => self.client.put(&url),
            Verb::Patch => self.client.patch(&url),
            Verb::Delete => self.client.delete(&url),
        };
        request = request
            .header("Accept", "application/json")
            .header("X-Request-Id", crate::ids::new_id());
        if !call.query.is_empty() {
            request = request.query(&call.query);
        }
        if let Some(key) = &call.key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(token) = &call.bearer {
            request = request.bearer_auth(token);
        }
        if let Some(store) = &call.store {
            request = request.header(self.store_header.as_str(), store);
        }
        // A request's own timeout would bound the whole body; a streamed one has none.
        if let Some(timeout) = call.timeout.filter(|_| sink.is_none()) {
            request = request.timeout(timeout);
        }
        request = match &call.body {
            Body::None => request,
            Body::Json(bytes) => request
                .header("Content-Type", "application/json")
                .body(bytes.clone()),
            Body::Form(pairs) => request.form(pairs),
            Body::Bytes(bytes, content_type) => request
                .header("Content-Type", content_type.as_str())
                .body(bytes.clone()),
        };
        // `reqwest::Error`'s Display names the URL (with its query): report the kind only.
        let mut response = request
            .send()
            .map_err(|e| Failure::Unreachable(describe(&e)))?;
        let status = response.status().as_u16();
        let headers = response.headers();
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let replayed = text("idempotency-replay").as_deref() == Some("1");
        let request_id = text("x-request-id");
        let retry = text("retry-after").and_then(|v| retry_after(&v, self.clock.now_ms()));
        let etag = text("etag");
        let body = match sink {
            Some(sink) if status < 400 => {
                stream(&mut response, sink)?;
                Vec::new()
            }
            _ => response
                .bytes()
                .map_err(|e| Failure::Unreachable(describe(&e)))?
                .to_vec(),
        };
        Ok(Reply {
            status,
            body,
            replayed,
            request_id,
            retry_after: retry,
            etag,
        })
    }
}

/// Whether a failed attempt may go again. A write carrying its `Idempotency-Key` is answered
/// from DZBuild's 24 h store, and GET, PUT and DELETE are idempotent: they go again after a
/// network error, a 5xx or a 429. A keyless POST or PATCH (the OAuth token, device and revoke
/// calls) goes again only when DZBuild provably never took it: no connection, or a 429 (the rate
/// limit is checked before any work). After a lost answer or a 5xx it may have landed, and a
/// replayed refresh token or authorization code is reuse: DZBuild revokes the whole grant.
fn may_retry(call: &Call, error: &CallError) -> bool {
    let replay_safe =
        call.key.is_some() || matches!(call.verb, Verb::Get | Verb::Put | Verb::Delete);
    match error {
        CallError::Server { status: 429, .. } => true,
        CallError::Offline(kind) if kind == NOT_CONNECTED => true,
        CallError::Offline(_) | CallError::Server { .. } => replay_safe,
        _ => false,
    }
}

/// The failure kind of a request that never reached a server (DNS, TCP or TLS).
const NOT_CONNECTED: &str = "connect";

fn describe(e: &reqwest::Error) -> String {
    // Connect first: a connect timeout is both, and its request never left the PC.
    let kind = if e.is_connect() {
        NOT_CONNECTED
    } else if e.is_timeout() {
        "timeout"
    } else if e.is_request() {
        "request"
    } else if e.is_body() || e.is_decode() {
        "body"
    } else {
        "other"
    };
    kind.to_owned()
}

/// Why an attempt brought back no reply.
enum Failure {
    /// No complete answer: DNS, connection, TLS, a timeout or a broken body. It is retried per
    /// [`may_retry`].
    Unreachable(String),
    /// A streamed body broke off after part of it reached the sink. It is never retried.
    Broken(String),
    /// The sink refused a write.
    Sink(String),
}

/// Copies a body to `sink` one read at a time. Each read waits at most the client's timeout.
fn stream(
    response: &mut reqwest::blocking::Response,
    sink: &mut (dyn Write + '_),
) -> Result<(), Failure> {
    let mut chunk = vec![0_u8; STREAM_CHUNK];
    let mut written = false;
    loop {
        let n = match response.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // reqwest wraps its own error, whose Display names the URL: keep only the kind.
            Err(e) => {
                let kind = e
                    .get_ref()
                    .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
                    .map_or_else(|| "body".to_owned(), describe);
                // Nothing reached the sink yet: the attempt may go again like any lost answer.
                return Err(if written {
                    Failure::Broken(kind)
                } else {
                    Failure::Unreachable(kind)
                });
            }
        };
        sink.write_all(&chunk[..n])
            .map_err(|e| Failure::Sink(e.to_string()))?;
        written = true;
    }
    sink.flush().map_err(|e| Failure::Sink(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::testkit::{MockDz, header};
    use serde::Deserialize;
    use serde_json::json;
    use std::io::Write;
    use wiremock::matchers::{header_exists, method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    #[derive(Debug, Deserialize)]
    struct Ping {
        time: String,
    }

    fn ok(data: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(
            json!({ "data": data, "meta": { "request_id": "req_1", "api_version": "1" } }),
        )
    }

    #[test]
    fn the_envelope_is_unwrapped_and_the_request_is_tagged() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(method("GET"))
                .and(path("/v1/me"))
                .and(header_exists("x-request-id"))
                .respond_with(ok(json!({ "time": "2026-09-28T11:00:00+01:00" })))
                .expect(1),
        );
        let http = dz.http();
        let reply = http
            .send(
                &Call::get(Endpoint::Me, "/me")
                    .bearer("at_1")
                    .store("141")
                    .lane(Lane::Interactive),
            )
            .unwrap();
        assert_eq!(
            reply.data::<Ping>().unwrap().time,
            "2026-09-28T11:00:00+01:00"
        );
        assert_eq!(
            reply.request_id.as_deref(),
            None,
            "the mock does not echo X-Request-Id"
        );
        let request = &dz.requests()[0];
        assert_eq!(
            header(request, "authorization").as_deref(),
            Some("Bearer at_1")
        );
        assert_eq!(header(request, "x-dz-store").as_deref(), Some("141"));
        assert!(
            header(request, "user-agent")
                .unwrap()
                .starts_with("DZBuildPOS/")
        );
        assert_eq!(
            header(request, "idempotency-key"),
            None,
            "reads carry no key"
        );
    }

    #[test]
    fn writes_carry_their_idempotency_key_and_a_replay_is_seen() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .insert_header("Idempotency-Replay", "1")
                        .set_body_json(json!({ "data": { "id": 99120 } })),
                ),
        );
        let reply = dz
            .http()
            .send(
                &Call::post(Endpoint::PosSales, "/pos/sales")
                    .json_body(b"{}".to_vec())
                    .key("sale:abc"),
            )
            .unwrap();
        assert!(reply.replayed);
        assert_eq!(
            header(&dz.requests()[0], "idempotency-key").as_deref(),
            Some("sale:abc")
        );
        assert!(
            Call::post(Endpoint::PosSales, "/pos/sales")
                .idempotency_key()
                .is_none()
        );
    }

    #[test]
    fn a_5xx_is_retried_with_backoff_and_retry_after_is_honoured() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/v1/store"))
                .respond_with(ResponseTemplate::new(503))
                .up_to_n_times(1),
        );
        dz.mount(
            Mock::given(path("/v1/store"))
                .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "7"))
                .up_to_n_times(1),
        );
        dz.mount(Mock::given(path("/v1/store")).respond_with(ok(json!({}))));
        let clock = ManualClock::at(1_000);
        let reply = dz
            .http_at(clock.clone())
            .send(&Call::get(Endpoint::Store, "/store"))
            .unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(dz.requests().len(), 3);
        assert_eq!(
            clock.slept(),
            [Duration::from_secs(1), Duration::from_secs(7)],
            "1 s after the 503, Retry-After after the 429"
        );
    }

    #[test]
    fn a_4xx_is_never_retried() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/v1/pos/sales"))
                .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                    "error": { "code": "validation_failed", "message": "lines[0].quantity" }
                })))
                .expect(1),
        );
        let err = dz
            .http_at(ManualClock::at(0))
            .send(
                &Call::post(Endpoint::PosSales, "/pos/sales")
                    .key("sale:x")
                    .json_body(b"{}".to_vec()),
            )
            .unwrap_err();
        match &err {
            CallError::Rejected { status, code, .. } => {
                assert_eq!((*status, code.as_str()), (422, "validation_failed"))
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(err.to_app().code(), "cloud_request_invalid");
        assert!(!err.is_transient());
        dz.verify();
    }

    #[test]
    fn a_missing_brief_endpoint_is_not_ready_but_a_missing_record_is_a_rejection() {
        let dz = MockDz::start();
        dz.mount(Mock::given(path("/v1/pos/closures")).respond_with(ResponseTemplate::new(404)));
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ResponseTemplate::new(501)),
        );
        dz.mount(
            Mock::given(path("/v1/orders/77")).respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "error": { "code": "order_not_found" } })),
            ),
        );
        let http = dz.http_at(ManualClock::at(0));
        let not_ready = http
            .send(&Call::post(Endpoint::PosClosures, "/pos/closures").key("closure:s"))
            .unwrap_err();
        assert!(matches!(
            not_ready,
            CallError::NotReady {
                endpoint: Endpoint::PosClosures,
                status: 404
            }
        ));
        assert_eq!(not_ready.to_app().code(), "cloud_not_ready");
        let not_impl = http
            .send(
                &Call::post(
                    Endpoint::InventoryAdjustments,
                    "/inventory/adjustments/batch",
                )
                .key("stock:a"),
            )
            .unwrap_err();
        assert!(matches!(not_impl, CallError::NotReady { status: 501, .. }));
        let missing = http
            .send(&Call::get(Endpoint::OrderGet, "/orders/77"))
            .unwrap_err();
        assert!(matches!(missing, CallError::Rejected { status: 404, .. }));
    }

    #[test]
    fn unauthorized_plan_caps_and_the_network_are_told_apart() {
        let dz = MockDz::start();
        dz.mount(Mock::given(path("/v1/me")).respond_with(ResponseTemplate::new(401)));
        dz.mount(
            Mock::given(path("/v1/products/batch")).respond_with(
                ResponseTemplate::new(402)
                    .set_body_json(json!({ "error": { "code": "plan_limit_reached" } })),
            ),
        );
        let http = dz.http_at(ManualClock::at(0));
        assert!(matches!(
            http.send(&Call::get(Endpoint::Me, "/me")).unwrap_err(),
            CallError::Unauthorized
        ));
        let cap = http
            .send(&Call::post(Endpoint::ProductsBatch, "/products/batch").key("products:x"))
            .unwrap_err();
        assert_eq!(cap.to_app().code(), "cloud_plan_cap");
        let offline = Http::new(
            &CloudConfig::loopback("http://127.0.0.1:9"),
            ManualClock::at(0),
            "t".into(),
        )
        .unwrap()
        .send(&Call::get(Endpoint::Me, "/me"))
        .unwrap_err();
        assert!(offline.is_offline());
        assert_eq!(offline.to_app().code(), "cloud_unavailable");
    }

    /// The most sends any 60 s window `[t, t + 60 s)` holds (`sent` in time order). DZBuild
    /// counts fixed calendar minutes, and every calendar minute is such a window.
    fn busiest_minute(sent: &[i64]) -> usize {
        (0..sent.len())
            .map(|i| {
                sent[i..]
                    .iter()
                    .take_while(|&&t| t < sent[i] + 60_000)
                    .count()
            })
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn no_minute_holds_more_than_a_hundred_background_calls_and_the_till_keeps_twenty() {
        let clock = ManualClock::at(0);
        let limiter = RateLimiter::new(REQUESTS_PER_MINUTE, INTERACTIVE_RESERVE, clock.clone());
        let mut sent = Vec::new();
        for _ in 0..1_000 {
            limiter.take(Lane::Background);
            sent.push(clock.now_ms());
        }
        assert_eq!(
            busiest_minute(&sent),
            100,
            "100 background calls a minute, never more"
        );
        assert_eq!(
            clock.now_ms(),
            540_000,
            "and never fewer: 1 000 calls in ten bursts, one a minute"
        );
        let before = clock.slept().len();
        for _ in 0..20 {
            limiter.take(Lane::Interactive);
        }
        assert_eq!(
            clock.slept().len(),
            before,
            "the till's 20 are still free on DZBuild's side"
        );
        limiter.take(Lane::Interactive);
        assert_eq!(
            clock.slept(),
            [&clock.slept()[..before], &[Duration::from_secs(60)]].concat(),
            "the 121st call of the minute waits for the oldest to leave the window"
        );
    }

    #[test]
    fn a_clock_set_back_never_holds_the_limiter_for_more_than_a_minute() {
        let clock = ManualClock::at(10_000_000);
        let limiter = RateLimiter::new(REQUESTS_PER_MINUTE, INTERACTIVE_RESERVE, clock.clone());
        for _ in 0..100 {
            limiter.take(Lane::Background);
        }
        clock.advance(-3_600_000);
        limiter.take(Lane::Background);
        assert_eq!(clock.slept(), [Duration::from_secs(60)]);
    }

    #[test]
    fn photo_uploads_stop_at_ten_a_minute() {
        let clock = ManualClock::at(0);
        let limiter = RateLimiter::new(REQUESTS_PER_MINUTE, INTERACTIVE_RESERVE, clock.clone())
            .with_media(MEDIA_PER_MINUTE);
        for _ in 0..10 {
            limiter.take(Lane::Media);
        }
        limiter.take(Lane::Background);
        assert!(
            clock.slept().is_empty(),
            "10 photos, and the rest of the sync still goes"
        );
        limiter.take(Lane::Media);
        assert_eq!(
            clock.slept(),
            [Duration::from_secs(60)],
            "the 11th photo of the minute waits"
        );
    }

    #[test]
    fn a_mixed_load_keeps_every_minute_under_every_cap() {
        let clock = ManualClock::at(1_000);
        let limiter = RateLimiter::new(REQUESTS_PER_MINUTE, INTERACTIVE_RESERVE, clock.clone())
            .with_media(MEDIA_PER_MINUTE);
        let mut seed: u64 = 7;
        let mut sent: Vec<(i64, Lane)> = Vec::new();
        for _ in 0..3_000 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let lane = match (seed >> 33) % 10 {
                0..=3 => Lane::Interactive,
                4 => Lane::Media,
                _ => Lane::Background,
            };
            clock.advance(i64::try_from((seed >> 40) % 400).unwrap());
            limiter.take(lane);
            sent.push((clock.now_ms(), lane));
        }
        let busiest = |keep: fn(Lane) -> bool| {
            let times: Vec<i64> = sent
                .iter()
                .filter(|(_, lane)| keep(*lane))
                .map(|(at, _)| *at)
                .collect();
            busiest_minute(&times)
        };
        let (all, background, media) = (
            busiest(|_| true),
            busiest(|l| l != Lane::Interactive),
            busiest(|l| l == Lane::Media),
        );
        assert!(
            all <= 120 && background <= 100 && media <= 10,
            "no minute passes a cap: {all} calls, {background} background with media, {media} media"
        );
        assert_eq!(
            (all, media),
            (120, 10),
            "the load saturates the limiter, so the check is not vacuous"
        );
    }

    #[test]
    fn only_a_replay_safe_call_goes_again_after_a_lost_answer() {
        let refresh = Call::post(Endpoint::Token, "");
        let sale = Call::post(Endpoint::PosSales, "/pos/sales").key("sale:a");
        let read = Call::get(Endpoint::Me, "/me");
        let upload = Call::put(Endpoint::Upload, "");
        let lost = CallError::Offline("timeout".into());
        let cut = CallError::Offline("body".into());
        let refused = CallError::Offline(NOT_CONNECTED.into());
        let gateway = CallError::Server {
            status: 502,
            retry_after: None,
        };
        let busy = CallError::Server {
            status: 429,
            retry_after: None,
        };
        for (call, error, expected) in [
            (&refresh, &lost, false),
            (&refresh, &cut, false),
            (&refresh, &gateway, false),
            (&refresh, &refused, true),
            (&refresh, &busy, true),
            (&refresh, &CallError::Unauthorized, false),
            (&sale, &lost, true),
            (&sale, &gateway, true),
            (&sale, &CallError::Invalid(String::new()), false),
            (&read, &lost, true),
            (&read, &gateway, true),
            (&upload, &lost, true),
        ] {
            assert_eq!(may_retry(call, error), expected, "{call:?} after {error:?}");
        }
    }

    fn requests_to(dz: &MockDz, route: &str) -> usize {
        dz.requests()
            .iter()
            .filter(|r| r.url.path() == route)
            .count()
    }

    fn form(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn a_keyless_post_that_may_have_reached_dzbuild_is_sent_once() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/oauth/token"))
                .respond_with(ResponseTemplate::new(503)),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/oauth/revoke"))
                .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2))),
        );
        let clock = ManualClock::at(0);
        let http = dz.http_at(clock.clone());
        let refresh = Call::post(Endpoint::Token, "")
            .absolute(&format!("{}/oauth/token", dz.uri()))
            .form(form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", "rt_1"),
            ]))
            .lane(Lane::Interactive);
        assert!(matches!(
            http.send(&refresh).unwrap_err(),
            CallError::Server { status: 503, .. }
        ));
        assert_eq!(
            requests_to(&dz, "/oauth/token"),
            1,
            "a refresh token DZBuild may have rotated is never sent again"
        );
        let revoke = Call::post(Endpoint::Revoke, "")
            .absolute(&format!("{}/oauth/revoke", dz.uri()))
            .form(form(&[("token", "rt_1")]))
            .timeout(Duration::from_millis(200));
        assert_eq!(
            http.send(&revoke).unwrap_err(),
            CallError::Offline("timeout".into())
        );
        assert_eq!(
            requests_to(&dz, "/oauth/revoke"),
            1,
            "the answer was lost, the request may have landed: the background lane's 4 attempts are not used"
        );
        assert!(clock.slept().is_empty());
    }

    #[test]
    fn a_keyed_write_is_retried_after_a_5xx_or_a_lost_answer() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .respond_with(ResponseTemplate::new(502)),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/refunds"))
                .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_secs(2))),
        );
        let clock = ManualClock::at(0);
        let http = dz.http_at(clock.clone());
        let sale = Call::post(Endpoint::PosSales, "/pos/sales")
            .json_body(b"{}".to_vec())
            .key("sale:a")
            .lane(Lane::Interactive);
        assert!(matches!(
            http.send(&sale).unwrap_err(),
            CallError::Server { status: 502, .. }
        ));
        let refund = Call::post(Endpoint::PosRefunds, "/pos/refunds")
            .json_body(b"{}".to_vec())
            .key("refund:a")
            .lane(Lane::Interactive)
            .timeout(Duration::from_millis(200));
        assert_eq!(
            http.send(&refund).unwrap_err(),
            CallError::Offline("timeout".into())
        );
        assert_eq!(requests_to(&dz, "/v1/pos/sales"), 2);
        assert_eq!(requests_to(&dz, "/v1/pos/refunds"), 2);
        assert_eq!(
            clock.slept(),
            [Duration::from_millis(500), Duration::from_millis(500)],
            "the key makes the replay safe: DZBuild answers it from its 24 h store"
        );
    }

    #[test]
    fn a_keyless_post_is_retried_when_dzbuild_never_took_it() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/oauth/token"))
                .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
                .up_to_n_times(1),
        );
        dz.mount(
            Mock::given(path("/oauth/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({}))),
        );
        let clock = ManualClock::at(0);
        let code = Call::post(Endpoint::Token, "")
            .absolute(&format!("{}/oauth/token", dz.uri()))
            .form(form(&[("grant_type", "authorization_code"), ("code", "c")]))
            .lane(Lane::Interactive);
        assert_eq!(dz.http_at(clock.clone()).send(&code).unwrap().status, 200);
        assert_eq!(
            requests_to(&dz, "/oauth/token"),
            2,
            "a 429 is refused before any work"
        );
        assert_eq!(clock.slept(), [Duration::from_secs(1)]);

        let offline = ManualClock::at(0);
        let refused = Http::new(
            &CloudConfig::loopback("http://127.0.0.1:9"),
            offline.clone(),
            "t".into(),
        )
        .unwrap()
        .send(
            &Call::post(Endpoint::Token, "")
                .absolute("http://127.0.0.1:9/oauth/token")
                .form(form(&[("grant_type", "authorization_code"), ("code", "c")]))
                .lane(Lane::Interactive),
        )
        .unwrap_err();
        assert_eq!(refused, CallError::Offline("connect".into()));
        assert_eq!(
            offline.slept(),
            [Duration::from_millis(500)],
            "no connection, nothing sent: retried as the lane allows"
        );
    }

    #[test]
    fn an_api_write_without_a_key_never_leaves_the_pc() {
        let dz = MockDz::start();
        dz.mount(Mock::given(wiremock::matchers::any()).respond_with(ok(json!({}))));
        let http = dz.http_at(ManualClock::at(0));
        for call in [
            Call::post(Endpoint::PosSales, "/pos/sales").json_body(b"{}".to_vec()),
            Call::patch(Endpoint::OrderUpdate, "/orders/7").json_body(b"{}".to_vec()),
            Call::delete(Endpoint::DeviceDelete, "/devices/d1"),
        ] {
            let err = http.send(&call).unwrap_err();
            assert!(matches!(err, CallError::Invalid(_)), "{call:?}: {err:?}");
        }
        assert!(dz.requests().is_empty());
    }

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn logs_never_carry_tokens_bodies_or_oauth_queries() {
        let dz = MockDz::start();
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
        ));
        dz.mount(
            Mock::given(path("/v1/me"))
                .and(query_param("q", "secret-query"))
                .respond_with(ok(json!({}))),
        );
        // tracing caches each callsite's interest process-wide. With one scoped dispatcher alive,
        // a callsite first reached on another test thread (no subscriber there) is cached as
        // "never" and this thread's events are dropped. A second live dispatcher makes every
        // registration consult the whole list, this capture included.
        let _second = tracing::Dispatch::new(tracing_subscriber::registry());
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let http = dz.http_at(ManualClock::at(0));
            let token_url = format!("{}/oauth/token", dz.uri());
            let _ = http.send(
                &Call::post(Endpoint::Token, "")
                    .absolute(&token_url)
                    .form(vec![("refresh_token".into(), "rt_SECRET_REFRESH".into())])
                    .lane(Lane::Interactive),
            );
            let _ = http.send(
                &Call::get(Endpoint::Me, "/me")
                    .query("q", "secret-query")
                    .bearer("at_SECRET_ACCESS"),
            );
        });
        let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert!(
            text.contains("oauth.token"),
            "the endpoint name is logged: {text}"
        );
        for secret in [
            "rt_SECRET_REFRESH",
            "at_SECRET_ACCESS",
            "secret-query",
            "Bearer",
            "invalid_grant\"",
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }

    fn chunk(fill: u8) -> Vec<u8> {
        vec![fill; 16 * 1024]
    }

    #[test]
    fn a_streamed_body_takes_its_time_on_a_slow_line_and_only_a_stall_ends_it() {
        use crate::cloud::testkit::trickle;
        let stall = Duration::from_millis(600);
        let clock = ManualClock::at(0);
        let http_for = |url: &str| {
            Http::with_timeouts(
                &CloudConfig::loopback(url),
                clock.clone(),
                "test".into(),
                Duration::from_secs(10),
                stall,
            )
            .unwrap()
        };
        // Slow but steady: 8 pieces 150 ms apart, twice the stall in all, under a call timeout
        // of 300 ms that a streamed body does not obey.
        let steps: Vec<_> = (0..8)
            .map(|n| (Duration::from_millis(150), chunk(n)))
            .collect();
        let whole: Vec<u8> = steps.iter().flat_map(|(_, c)| c.clone()).collect();
        let url = trickle(steps, whole.len());
        let call = Call::get(Endpoint::ProductImages, "")
            .absolute(&format!("{url}/download"))
            .timeout(Duration::from_millis(300));
        let mut sink = Vec::new();
        let started = Instant::now();
        let reply = http_for(&url).send_to(&call, &mut sink).unwrap();
        assert!(started.elapsed() > stall, "longer than the stall in all");
        assert_eq!(
            (reply.status, reply.body.len()),
            (200, 0),
            "the body is not held in memory"
        );
        assert_eq!(sink, whole);

        // A line that stops: 16 KiB, then nothing for longer than the stall.
        let url = trickle(
            vec![
                (Duration::ZERO, chunk(1)),
                (Duration::from_secs(3), chunk(2)),
            ],
            32 * 1024,
        );
        let call = Call::get(Endpoint::ProductImages, "").absolute(&format!("{url}/download"));
        let mut sink = Vec::new();
        assert_eq!(
            http_for(&url).send_to(&call, &mut sink).unwrap_err(),
            CallError::Offline("timeout".into())
        );
        assert_eq!(sink.len(), 16 * 1024, "the sink holds what came");
        assert!(
            clock.slept().is_empty(),
            "never retried once part of the body reached the sink"
        );

        // A line that stops before the first byte: nothing to discard, so it goes again.
        let url = trickle(vec![(Duration::from_secs(3), chunk(1))], 16 * 1024);
        let call = Call::get(Endpoint::ProductImages, "")
            .absolute(&format!("{url}/download"))
            .lane(Lane::Interactive);
        let mut sink = Vec::new();
        assert_eq!(
            http_for(&url).send_to(&call, &mut sink).unwrap_err(),
            CallError::Offline("timeout".into())
        );
        assert!(sink.is_empty());
        assert_eq!(clock.slept(), [Duration::from_millis(500)], "retried once");
    }

    struct FullDisk;

    impl Write for FullDisk {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_streamed_call_reads_an_error_for_its_code_and_a_failing_sink_is_not_dzbuild() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/v1/media/m_9")).respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "error": { "code": "media_not_found" } })),
            ),
        );
        dz.mount(
            Mock::given(path("/v1/media/m_1"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7_u8; 100_000])),
        );
        let clock = ManualClock::at(0);
        let http = dz.http_at(clock.clone());
        let mut sink = Vec::new();
        let missing = http
            .send_to(&Call::get(Endpoint::ProductImages, "/media/m_9"), &mut sink)
            .unwrap_err();
        assert!(
            matches!(&missing, CallError::Rejected { status: 404, code, .. } if code == "media_not_found"),
            "{missing:?}"
        );
        assert!(sink.is_empty(), "an error body never reaches the sink");
        let full = http
            .send_to(
                &Call::get(Endpoint::ProductImages, "/media/m_1"),
                &mut FullDisk,
            )
            .unwrap_err();
        assert_eq!(full, CallError::Sink("disk full".into()));
        assert_eq!(
            (full.code(), full.to_app().code()),
            ("io_error", "io_error".to_owned())
        );
        assert!(!full.is_transient());
        assert_eq!(
            requests_to(&dz, "/v1/media/m_1"),
            1,
            "a full disk is not retried"
        );
        assert!(clock.slept().is_empty());
    }
}

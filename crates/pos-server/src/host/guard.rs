//! Request guards of the main PC: token buckets, bearer tokens, and a log line per request that
//! never carries a header, a query string or a body (tokens, codes and row images stay out).

use super::Shared;
use crate::wire::{H_PENDING, H_SCHEMA, H_SKEW, H_VERSION, error_body, status_of};
use axum::Json;
use axum::extract::{ConnectInfo, MatchedPath, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use pos_core::error::AppError;
use pos_core::sync::devices::{self, Device, Touch};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A token bucket counted in thousandths of a request (integers only).
#[derive(Debug, Clone, Copy)]
pub struct Bucket {
    milli_tokens: i64,
    at: i64,
}

/// Token buckets per key (an address and a route, or a till).
#[derive(Debug, Default)]
pub struct Limiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl Limiter {
    pub fn allow(&self, key: &str, capacity: u32, per_minute: u32, now_ms: i64) -> bool {
        let Ok(mut buckets) = self.buckets.lock() else {
            return false;
        };
        let full = i64::from(capacity) * 1_000;
        let bucket = buckets.entry(key.to_owned()).or_insert(Bucket {
            milli_tokens: full,
            at: now_ms,
        });
        // per_minute requests a minute = per_minute thousandths of a request each 60 ms.
        let refill = (now_ms - bucket.at)
            .max(0)
            .saturating_mul(i64::from(per_minute))
            / 60;
        bucket.milli_tokens = bucket.milli_tokens.saturating_add(refill).min(full);
        bucket.at = now_ms;
        if bucket.milli_tokens >= 1_000 {
            bucket.milli_tokens -= 1_000;
            true
        } else {
            false
        }
    }
}

pub fn error_response(error: &AppError) -> Response {
    let status =
        StatusCode::from_u16(status_of(error)).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(%error, "LAN request failed");
    }
    (status, Json(error_body(error))).into_response()
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value.strip_prefix("Bearer ").map(|t| t.trim().to_owned())
}

/// Bearer token → `Device` extension; records the till's contact at most every 10 s.
pub async fn auth(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer(request.headers()) else {
        return error_response(&AppError::auth("lan_token_invalid"));
    };
    let db = shared.services.db.clone();
    let device: Result<Device, AppError> = match tokio::task::spawn_blocking(move || {
        db.read(move |c| devices::authenticate(c, &token))
    })
    .await
    {
        Ok(found) => found,
        Err(e) => Err(AppError::Internal(format!("auth task: {e}"))),
    };
    let device = match device {
        Ok(device) => device,
        Err(error) => return error_response(&error),
    };
    let now = (shared.services.clock)();
    if shared.should_touch(&device.terminal_id, now) {
        let headers = request.headers();
        let info = Touch {
            address: Some(peer.ip().to_string()),
            app_version: headers
                .get(H_VERSION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            schema_version: header_i64(headers, H_SCHEMA),
            outbox_pending: header_i64(headers, H_PENDING),
            skew_ms: header_i64(headers, H_SKEW),
        };
        let (db, id, schema) = (
            shared.services.db.clone(),
            device.terminal_id.clone(),
            shared.config.schema_version,
        );
        tokio::task::spawn_blocking(move || {
            if let Err(error) = db.write(move |tx| devices::touch(tx, &id, &info, schema, now)) {
                tracing::warn!(code = %error.code(), "could not record a till's contact");
            }
        });
    }
    request.extensions_mut().insert(device);
    next.run(request).await
}

/// One line per request: method, route template, status, duration, the till's code.
pub async fn log(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "?".to_owned(), |m| m.as_str().to_owned());
    let started = Instant::now();
    let response = next.run(request).await;
    let till = response
        .extensions()
        .get::<Device>()
        .map(|d| d.code.clone());
    tracing::info!(
        %method,
        route,
        status = response.status().as_u16(),
        ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        till = till.as_deref().unwrap_or("-"),
        "lan request"
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bucket_refills_over_a_minute() {
        let limiter = Limiter::default();
        let key = "127.0.0.1:/pair".to_owned();
        for _ in 0..10 {
            assert!(limiter.allow(&key, 10, 10, 0));
        }
        assert!(
            !limiter.allow(&key, 10, 10, 1_000),
            "the eleventh within a second"
        );
        assert!(
            limiter.allow(&key, 10, 10, 7_000),
            "one token back after six seconds"
        );
        assert!(
            limiter.allow("127.0.0.2:/pair", 10, 10, 1_000),
            "per address"
        );
    }
}

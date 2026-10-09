//! HTTP calls of a till to its main PC (reqwest, plain HTTP on the shop's LAN, never through a
//! proxy). A main PC that cannot be reached is `lan_offline`; an answer with an error body comes
//! back as the `AppError` the main PC sent.

use crate::wire::{
    H_HOST_TIME, H_PENDING, H_SCHEMA, H_SEQ, H_SHA256, H_SKEW, H_VERSION, HealthInfo, PairResponse,
    ReplayRequest, ReplayResponse, RpcGrant, RpcRequest, RpcResponse, from_body,
};
use futures_util::StreamExt;
use pos_core::error::{AppError, ErrorPayload};
use pos_core::sync::devices::PairRequest;
use pos_core::sync::feed::FeedPage;
use pos_core::sync::outbox::{ItemResult, WireItem};
use pos_core::sync::skew;
use pos_core::time::now_ms;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// `192.168.1.10`, `192.168.1.10:8000` or `http://caisse-1.local:7345/` → `http://host:port`
/// (port 7345 when none is given).
pub fn normalize_address(input: &str) -> Result<String, AppError> {
    let invalid = || AppError::validation("address", "lan_address_invalid");
    let trimmed = input.trim().trim_end_matches('/');
    let without = trimmed.strip_prefix("http://").unwrap_or(trimmed);
    if without.is_empty()
        || without.contains("://")
        || without.contains('/')
        || without.contains(' ')
    {
        return Err(invalid());
    }
    let with_port = if without
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        without.to_owned()
    } else if without.contains(':') {
        return Err(invalid());
    } else {
        format!("{without}:{}", crate::wire::PORT)
    };
    Ok(format!("http://{with_port}"))
}

/// The main PC did not answer (or the answer broke off). The error itself is not logged: it
/// names the URL, and the log keeps only what failed.
fn offline(error: reqwest::Error) -> AppError {
    tracing::info!(
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        "the main PC did not answer"
    );
    AppError::Sync {
        code: "lan_offline".into(),
    }
}

async fn answer<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, AppError> {
    let status = response.status();
    if status.is_success() {
        return response.json::<T>().await.map_err(offline);
    }
    let body: ErrorPayload = response.json().await.map_err(offline)?;
    Err(from_body(status.as_u16(), body))
}

#[derive(Clone)]
pub struct HostApi {
    client: reqwest::Client,
    base: String,
    token: Option<String>,
    app_version: String,
    schema_version: i64,
}

impl std::fmt::Debug for HostApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostApi")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl HostApi {
    pub fn new(
        base: &str,
        token: Option<String>,
        app_version: &str,
        schema_version: i64,
    ) -> Result<HostApi, AppError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .no_proxy()
            .build()
            .map_err(offline)?;
        Ok(HostApi {
            client,
            base: base.trim_end_matches('/').to_owned(),
            token,
            app_version: app_version.to_owned(),
            schema_version,
        })
    }

    fn with_headers(
        &self,
        request: reqwest::RequestBuilder,
        pending: Option<i64>,
        skew: Option<i64>,
    ) -> reqwest::RequestBuilder {
        let mut request = request
            .header(H_VERSION, &self.app_version)
            .header(H_SCHEMA, self.schema_version.to_string());
        if let Some(pending) = pending {
            request = request.header(H_PENDING, pending.to_string());
        }
        if let Some(skew) = skew {
            request = request.header(H_SKEW, skew.to_string());
        }
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    /// The main PC's identity, and its clock minus ours measured around the call.
    pub async fn health(&self) -> Result<(HealthInfo, i64), AppError> {
        let sent = now_ms();
        let response = self
            .with_headers(self.client.get(format!("{}/health", self.base)), None, None)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(offline)?;
        let received = now_ms();
        let host_time = response
            .headers()
            .get(H_HOST_TIME)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok());
        let info: HealthInfo = answer(response).await?;
        let skew = skew::measure(sent, host_time.unwrap_or(info.host_time), received);
        Ok((info, skew))
    }

    pub async fn pair(&self, request: &PairRequest) -> Result<PairResponse, AppError> {
        let response = self
            .with_headers(self.client.post(format!("{}/pair", self.base)), None, None)
            .timeout(Duration::from_secs(15))
            .json(request)
            .send()
            .await
            .map_err(offline)?;
        answer(response).await
    }

    /// The snapshot streamed into `path` (no overall timeout: a big copy on a slow LAN takes
    /// its time); `(seq, sha256)` from the headers. `progress(received, total)` per chunk.
    pub async fn download(
        &self,
        path: &Path,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(i64, String), AppError> {
        let response = self
            .with_headers(
                self.client.get(format!("{}/snapshot", self.base)),
                None,
                None,
            )
            .send()
            .await
            .map_err(offline)?;
        if !response.status().is_success() {
            return Err(answer::<Value>(response)
                .await
                .err()
                .unwrap_or_else(|| AppError::conflict("snapshot_invalid")));
        }
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let seq = header(H_SEQ)
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(|| AppError::conflict("snapshot_invalid"))?;
        let sha = header(H_SHA256).ok_or_else(|| AppError::conflict("snapshot_invalid"))?;
        let total = response.content_length();
        let mut file = tokio::fs::File::create(path).await?;
        let mut received = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(offline)?;
            file.write_all(&chunk).await?;
            received += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            progress(received, total);
        }
        file.flush().await?;
        file.sync_all().await?;
        Ok((seq, sha))
    }

    /// The manifest of the update the main PC verified; `None` when it has none.
    pub async fn update_manifest(&self) -> Result<Option<Value>, AppError> {
        let response = self
            .with_headers(
                self.client.get(format!("{}/updates/manifest", self.base)),
                None,
                None,
            )
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(offline)?;
        match answer::<Value>(response).await {
            Ok(manifest) => Ok(Some(manifest)),
            Err(error) if error.code() == "update_not_available" => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The setup the main PC cached, read whole (a few MB on the LAN), with `progress(received,
    /// total)` per chunk. A read that stalls 60 s fails (`lan_offline`).
    pub async fn update_setup(
        &self,
        progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<Vec<u8>, AppError> {
        let response = self
            .with_headers(
                self.client.get(format!("{}/updates/setup", self.base)),
                None,
                None,
            )
            .send()
            .await
            .map_err(offline)?;
        if !response.status().is_success() {
            return Err(answer::<Value>(response)
                .await
                .err()
                .unwrap_or_else(|| AppError::conflict("update_not_available")));
        }
        let total = response.content_length();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::time::timeout(Duration::from_secs(60), stream.next())
                .await
                .map_err(|_| AppError::sync("lan_offline"))?;
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(offline)?;
            bytes.extend_from_slice(&chunk);
            progress(u64::try_from(bytes.len()).unwrap_or(u64::MAX), total);
        }
        Ok(bytes)
    }

    pub async fn changes(
        &self,
        since: i64,
        pending: i64,
        skew: Option<i64>,
    ) -> Result<FeedPage, AppError> {
        let response = self
            .with_headers(
                self.client.get(format!("{}/changes", self.base)),
                Some(pending),
                skew,
            )
            .query(&[("since", since)])
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(offline)?;
        answer(response).await
    }

    pub async fn replay(
        &self,
        items: Vec<WireItem>,
        pending: i64,
        skew: Option<i64>,
    ) -> Result<Vec<ItemResult>, AppError> {
        let response = self
            .with_headers(
                self.client.post(format!("{}/replay", self.base)),
                Some(pending),
                skew,
            )
            .timeout(Duration::from_secs(30))
            .json(&ReplayRequest { items })
            .send()
            .await
            .map_err(offline)?;
        answer::<ReplayResponse>(response).await.map(|r| r.results)
    }

    pub async fn rpc(
        &self,
        service: &str,
        user_id: &str,
        approvals: Vec<RpcGrant>,
        input: Value,
        idempotency_key: Option<String>,
    ) -> Result<RpcResponse, AppError> {
        let response = self
            .with_headers(
                self.client.post(format!("{}/rpc/{service}", self.base)),
                None,
                None,
            )
            .timeout(Duration::from_secs(15))
            .json(&RpcRequest {
                user_id: user_id.to_owned(),
                input,
                approvals,
                idempotency_key,
            })
            .send()
            .await
            .map_err(offline)?;
        answer(response).await
    }

    pub fn feed_url(&self) -> String {
        format!("ws://{}/feed", self.base.trim_start_matches("http://"))
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

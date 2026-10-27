//! Typed calls to the DZBuild API (Backend Brief) on behalf of one link: the bearer and the store
//! header are added here; a 401 gets one forced refresh and one retry; a refused refresh surfaces
//! as `CallError::Relink`.

use super::config::CloudConfig;
use super::http::{ApiErrorBody, Call, CallError, Endpoint, Http, Lane, Reply};
use super::oauth::{Metadata, RemoteStore};
use super::tokens::TokenManager;
use super::wire::{ApiMoney, ApiTime, IdString, de_id};
use crate::error::AppError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::sync::Arc;

#[derive(Clone)]
pub struct Remote {
    http: Arc<Http>,
    tokens: Arc<TokenManager>,
    meta: Arc<Metadata>,
    config: Arc<CloudConfig>,
    store_id: Option<String>,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote")
            .field("store_id", &self.store_id)
            .finish_non_exhaustive()
    }
}

/// A token failure as the transport sees it.
fn auth_failure(e: &AppError) -> CallError {
    match e.code().as_str() {
        "cloud_relink_required" => CallError::Relink,
        "cloud_unavailable" => CallError::Offline("refresh".into()),
        "cloud_rate_limited" => CallError::Server {
            status: 429,
            retry_after: None,
        },
        _ => CallError::Invalid("refresh".into()),
    }
}

impl Remote {
    pub fn new(
        http: Arc<Http>,
        tokens: Arc<TokenManager>,
        meta: Arc<Metadata>,
        config: Arc<CloudConfig>,
        store_id: Option<String>,
    ) -> Remote {
        Remote {
            http,
            tokens,
            meta,
            config,
            store_id,
        }
    }

    pub fn for_store(&self, store_id: &str) -> Remote {
        Remote {
            store_id: Some(store_id.to_owned()),
            ..self.clone()
        }
    }

    pub fn http(&self) -> &Http {
        &self.http
    }
    pub fn tokens(&self) -> &Arc<TokenManager> {
        &self.tokens
    }
    pub fn meta(&self) -> &Metadata {
        &self.meta
    }
    pub fn config(&self) -> &CloudConfig {
        &self.config
    }
    pub fn store_id(&self) -> Option<&str> {
        self.store_id.as_deref()
    }

    pub fn send(&self, call: Call) -> Result<Reply, CallError> {
        self.authorized(call, |http, call| http.send(call))
    }

    /// [`Remote::send`] with the body streamed into `sink` ([`Http::send_to`]). A 401 arrives
    /// before any byte of the body, so its one retry is safe.
    pub fn send_to(&self, call: Call, sink: &mut dyn Write) -> Result<Reply, CallError> {
        self.authorized(call, |http, call| http.send_to(call, &mut *sink))
    }

    fn authorized(
        &self,
        call: Call,
        mut go: impl FnMut(&Http, &Call) -> Result<Reply, CallError>,
    ) -> Result<Reply, CallError> {
        let token = self
            .tokens
            .access_token(&self.http, &self.meta, &self.config)
            .map_err(|e| auth_failure(&e))?;
        let mut call = call.bearer(&token);
        if let Some(store) = &self.store_id {
            call = call.store(store);
        }
        match go(&self.http, &call) {
            Err(CallError::Unauthorized) => {
                let next = self
                    .tokens
                    .force_refresh(&self.http, &self.meta, &self.config, &token)
                    .map_err(|e| auth_failure(&e))?;
                go(&self.http, &call.with_bearer(&next))
            }
            other => other,
        }
    }

    pub fn data<T: DeserializeOwned>(&self, call: Call) -> Result<T, CallError> {
        self.send(call)?.data()
    }
}

/// `Ok(None)` for an endpoint DZBuild has not shipped yet, the `AppError` otherwise.
pub fn ready<T>(result: Result<T, CallError>) -> Result<Option<T>, AppError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(CallError::NotReady { .. }) => Ok(None),
        Err(e) => Err(e.to_app()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Me {
    #[serde(deserialize_with = "de_id")]
    pub sub: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub stores: Vec<RemoteStore>,
}

/// `GET /v1/me`: the account, its stores, roles and plans.
pub fn me(remote: &Remote) -> Result<Me, CallError> {
    remote.data(Call::get(Endpoint::Me, "/me").lane(Lane::Interactive))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanLimits {
    #[serde(default)]
    pub active_products: Option<i64>,
    #[serde(default)]
    pub orders_per_month: Option<i64>,
}

/// `GET /v1/store` (+ the Brief's `stock_deduction`, `timezone`, `currency`, `plan_limits`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoreProfile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub public_url: Option<String>,
    #[serde(default)]
    pub stock_deduction: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub plan_limits: Option<PlanLimits>,
}

pub fn store_profile(remote: &Remote) -> Result<StoreProfile, CallError> {
    remote.data(Call::get(Endpoint::Store, "/store"))
}

/// This till as DZBuild's device registry shows it to the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub os: String,
    pub app_version: String,
    pub terminal_code: String,
    pub terminal_id: String,
}

/// `POST /v1/devices`. The key names one registration of this till with the remote's store,
/// `device:<install>:<terminal>:<store>:g<generation>`, where `generation` counts the till's
/// registrations that ended (unlinks, revocations). A retry of a failed attempt reuses the key, so
/// DZBuild replays the device it made instead of making a twin. A link after an unlink or a
/// revocation takes a new key, so it never gets back a deleted device (DZBuild replays a key for
/// 24 h), nor `422 idempotency_key_reuse` because the app version or the name changed.
/// `external_id` stays the till's own, for DZBuild's deduplication.
pub fn register_device(
    remote: &Remote,
    device: &DeviceInfo,
    install_id: &str,
    generation: i64,
) -> Result<String, CallError> {
    let body = json!({
        "external_id": format!("{install_id}:{}", device.terminal_code),
        "name": device.name,
        "os": device.os,
        "app_version": device.app_version,
        "terminal": device.terminal_code,
        "platform": "windows",
    });
    let key = super::outbox_key(
        "device",
        &format!(
            "{install_id}:{}:{}:g{generation}",
            device.terminal_code,
            remote.store_id().unwrap_or_default()
        ),
    );
    let created: CreatedRecord = remote.data(
        Call::post(Endpoint::Devices, "/devices")
            .json_body(body.to_string().into_bytes())
            .key(&key)
            .lane(Lane::Interactive),
    )?;
    Ok(created.id)
}

#[derive(Debug, Clone, Serialize)]
pub struct Heartbeat {
    pub app_version: String,
    pub pending: i64,
    pub failed: i64,
    pub last_sync_at: Option<ApiTime>,
}

pub fn heartbeat(remote: &Remote, device_id: &str, beat: &Heartbeat) -> Result<(), CallError> {
    let body = serde_json::to_vec(beat)
        .map_err(|e| CallError::Invalid(format!("heartbeat {:?}", e.classify())))?;
    remote
        .send(
            Call::post(
                Endpoint::DeviceHeartbeat,
                &format!("/devices/{device_id}/heartbeat"),
            )
            .json_body(body)
            .key(&format!("beat:{}", crate::ids::new_id())),
        )
        .map(drop)
}

pub fn delete_device(remote: &Remote, device_id: &str) -> Result<(), CallError> {
    remote
        .send(
            Call::delete(Endpoint::DeviceDelete, &format!("/devices/{device_id}"))
                .key(&super::outbox_key("device-delete", device_id))
                .lane(Lane::Interactive)
                .timeout(std::time::Duration::from_secs(5)),
        )
        .map(drop)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteDevice {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<ApiTime>,
}

/// `GET /v1/devices` (optional in the Backend contract: not ready is shown as such).
pub fn list_devices(remote: &Remote) -> Result<Vec<RemoteDevice>, CallError> {
    let page: super::http::Page<RemoteDevice> =
        remote.data(Call::get(Endpoint::Devices, "/devices").lane(Lane::Interactive))?;
    Ok(page.items)
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteLocation {
    #[serde(deserialize_with = "de_id")]
    id: String,
    #[serde(default)]
    external_id: Option<String>,
}

/// The shop's location on DZBuild: found by `external_id` (the install id) or created.
pub fn ensure_location(remote: &Remote, install_id: &str, name: &str) -> Result<String, CallError> {
    let page: super::http::Page<RemoteLocation> =
        remote.data(Call::get(Endpoint::Locations, "/locations").query("limit", "200"))?;
    if let Some(found) = page
        .items
        .iter()
        .find(|l| l.external_id.as_deref() == Some(install_id))
    {
        return Ok(found.id.clone());
    }
    let body = json!({ "external_id": install_id, "name": name, "type": "shop" });
    let created: CreatedRecord = remote.data(
        Call::post(Endpoint::Locations, "/locations")
            .json_body(body.to_string().into_bytes())
            .key(&super::outbox_key("location", install_id)),
    )?;
    Ok(created.id)
}

/// `{"id": …}` of a created record (a sale, a refund, a closure, a device, a location); a sale's
/// answer also says whether DZBuild moved its stock although asked not to.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatedRecord {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    #[serde(default)]
    pub stock_applied: Option<bool>,
}

/// One answer of a batch endpoint (products by `external_id`, stock by `ref`).
#[derive(Debug, Clone, Deserialize)]
pub struct BatchItemAnswer {
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub id: Option<IdString>,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub error: Option<ApiErrorBody>,
    /// A variant family's children as DZBuild answered them (v1.12).
    #[serde(default)]
    pub variants: Option<Vec<VariantAnswer>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VariantAnswer {
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub price_online: Option<ApiMoney>,
    #[serde(default)]
    pub error: Option<ApiErrorBody>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchAnswer {
    #[serde(default)]
    pub items: Vec<BatchItemAnswer>,
}

/// `POST /v1/media/uploads` → where to `PUT` the bytes. The pre-signed URL is a capability:
/// `Debug` never prints it.
#[derive(Clone, Deserialize)]
pub struct MediaTicket {
    #[serde(deserialize_with = "de_id")]
    pub media_id: String,
    pub upload_url: String,
}

impl std::fmt::Debug for MediaTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaTicket")
            .field("media_id", &self.media_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::http::ManualClock;
    use crate::cloud::oauth::TokenResponse;
    use crate::cloud::oauth::tests::metadata;
    use crate::cloud::secrets::MemorySecrets;
    use crate::cloud::testkit::MockDz;
    use serde_json::json;
    use wiremock::matchers::{bearer_token, path};
    use wiremock::{Mock, ResponseTemplate};

    #[test]
    fn a_401_gets_one_refresh_and_one_retry() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/v1/me"))
                .and(bearer_token("at_1"))
                .respond_with(ResponseTemplate::new(401)),
        );
        dz.mount(
            Mock::given(path("/v1/me")).and(bearer_token("at_2")).respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "data": { "sub": 84211, "name": "Amel", "stores": [{ "store_id": 141, "store_name": "Boutique Amel" }] } }),
            )),
        );
        dz.mount(
            Mock::given(path("/oauth/token")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_2", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_2"
            }))),
        );
        let clock = ManualClock::at(0);
        let tokens = Arc::new(TokenManager::new(
            "00000000-0000-7000-8000-00000000c001",
            Arc::new(MemorySecrets::default()),
            clock.clone(),
        ));
        tokens
            .store(&TokenResponse {
                access_token: "at_1".into(),
                token_type: "Bearer".into(),
                expires_in: Some(900),
                refresh_token: Some("rt_1".into()),
                scope: None,
                stores: Vec::new(),
            })
            .unwrap();
        let remote = Remote::new(
            Arc::new(dz.http_at(clock)),
            tokens,
            Arc::new(metadata(&dz.uri())),
            Arc::new(dz.config()),
            None,
        );
        let me = me(&remote).unwrap();
        assert_eq!(
            (me.sub.as_str(), me.stores[0].store_id.as_str()),
            ("84211", "141")
        );
        let paths: Vec<String> = dz
            .requests()
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect();
        assert_eq!(paths, ["/v1/me", "/oauth/token", "/v1/me"]);
    }

    #[test]
    fn a_store_bound_remote_sends_the_store_header() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/v1/store"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} }))),
        );
        let clock = ManualClock::at(0);
        let tokens = Arc::new(TokenManager::new(
            "l",
            Arc::new(MemorySecrets::default()),
            clock.clone(),
        ));
        tokens
            .store(&TokenResponse {
                access_token: "at_1".into(),
                token_type: "bearer".into(),
                expires_in: Some(900),
                refresh_token: Some("rt_1".into()),
                scope: None,
                stores: Vec::new(),
            })
            .unwrap();
        let remote = Remote::new(
            Arc::new(dz.http_at(clock)),
            tokens,
            Arc::new(metadata(&dz.uri())),
            Arc::new(dz.config()),
            None,
        )
        .for_store("141");
        remote.send(Call::get(Endpoint::Store, "/store")).unwrap();
        assert_eq!(
            crate::cloud::testkit::header(&dz.requests()[0], "x-dz-store").as_deref(),
            Some("141")
        );
        assert!(matches!(
            ready::<()>(Err(CallError::NotReady {
                endpoint: Endpoint::Store,
                status: 404
            })),
            Ok(None)
        ));
    }
}

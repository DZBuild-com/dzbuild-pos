//! OAuth 2.0 for a native public client (RFC 8252, spec §4.7, research 06 §5.3–5.4): RFC 8414
//! discovery, the authorization code with PKCE S256 and a loopback redirect, `state` and RFC 9207
//! `iss` checks, the device grant (RFC 8628), refresh with rotation, revocation (RFC 7009) and
//! userinfo. `client_id` only, never a secret. No token or code reaches `Debug`, a log or SQLite.

use super::config::{CloudConfig, check_url};
use super::http::{Call, CallError, Endpoint, Http, Lane};
use super::wire::de_id;
use crate::error::AppError;
use crate::lang::Lang;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Metadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub device_authorization_endpoint: Option<String>,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    pub authorization_response_iss_parameter_supported: bool,
}

fn same_issuer(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// RFC 8414 metadata, refused unless the issuer is the configured one (§3.3), S256 is offered,
/// and every endpoint is https (or the loopback mock).
pub fn discover(http: &Http, config: &CloudConfig) -> Result<Metadata, AppError> {
    let url = config.well_known()?;
    let call = Call::get(Endpoint::Discovery, "")
        .absolute(url.as_str())
        .lane(Lane::Interactive);
    let reply = http.send(&call).map_err(|e| match e {
        CallError::Offline(_) | CallError::Server { .. } => e.to_app(),
        _ => AppError::sync("cloud_discovery_invalid"),
    })?;
    let meta: Metadata = reply
        .json()
        .map_err(|_| AppError::sync("cloud_discovery_invalid"))?;
    let issuer_ok = same_issuer(&meta.issuer, &config.issuer);
    let s256 = meta
        .code_challenge_methods_supported
        .iter()
        .any(|m| m == "S256");
    let endpoints_ok = [
        Some(&meta.authorization_endpoint),
        Some(&meta.token_endpoint),
        meta.device_authorization_endpoint.as_ref(),
        meta.revocation_endpoint.as_ref(),
        meta.userinfo_endpoint.as_ref(),
    ]
    .into_iter()
    .flatten()
    .all(|u| check_url(u).is_ok());
    if issuer_ok && s256 && endpoints_ok {
        Ok(meta)
    } else {
        tracing::warn!(target: "cloud", issuer_ok, s256, endpoints_ok, "authorization server metadata refused");
        Err(AppError::sync("cloud_discovery_invalid"))
    }
}

/// `bytes` from the OS generator, base64url without padding (32 bytes → 43 characters).
pub fn random_token(bytes: usize) -> Result<String, AppError> {
    let mut buf = vec![0_u8; bytes];
    getrandom::fill(&mut buf)
        .map_err(|_| AppError::Internal("the OS random generator failed".into()))?;
    Ok(URL_SAFE_NO_PAD.encode(&buf))
}

/// RFC 7636 with S256 only.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce {
    verifier: String,
    pub challenge: String,
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pkce(S256, verifier redacted)")
    }
}

impl Pkce {
    pub fn generate() -> Result<Pkce, AppError> {
        Ok(Pkce::from_verifier(random_token(32)?))
    }

    pub fn from_verifier(verifier: String) -> Pkce {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()).as_slice());
        Pkce {
            verifier,
            challenge,
        }
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }
}

pub struct AuthRequest {
    pub url: String,
    pub state: String,
    pub pkce: Pkce,
    pub redirect_uri: String,
}

impl std::fmt::Debug for AuthRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthRequest(redacted)")
    }
}

pub fn authorization_url(
    meta: &Metadata,
    config: &CloudConfig,
    redirect_uri: &str,
    lang: Lang,
) -> Result<AuthRequest, AppError> {
    let pkce = Pkce::generate()?;
    let state = random_token(32)?;
    let mut url = url::Url::parse(&meta.authorization_endpoint)
        .map_err(|_| AppError::sync("cloud_discovery_invalid"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", &config.scope())
        .append_pair("state", &state)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("ui_locales", lang.as_str());
    Ok(AuthRequest {
        url: url.into(),
        state,
        pkce,
        redirect_uri: redirect_uri.to_owned(),
    })
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub iss: Option<String>,
    pub error: Option<String>,
}

impl std::fmt::Debug for CallbackParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackParams")
            .field("code", &self.code.as_ref().map(|_| "…"))
            .field("state", &self.state.as_ref().map(|_| "…"))
            .field("error", &self.error)
            .finish()
    }
}

pub fn parse_callback(query: &str) -> CallbackParams {
    let mut params = CallbackParams::default();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "code" => &mut params.code,
            "state" => &mut params.state,
            "iss" => &mut params.iss,
            "error" => &mut params.error,
            _ => continue,
        };
        *slot = Some(value.into_owned());
    }
    params
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The authorization code of a genuine answer to this request. `state` first (a forged callback
/// learns nothing else), then `iss` (RFC 9207: required when advertised, equal to the issuer),
/// then the error the server returned.
pub fn check_callback(
    params: &CallbackParams,
    expected_state: &str,
    meta: &Metadata,
) -> Result<String, AppError> {
    let state_ok = params
        .state
        .as_deref()
        .is_some_and(|s| constant_time_eq(s.as_bytes(), expected_state.as_bytes()));
    let iss_ok = match &params.iss {
        Some(iss) => same_issuer(iss, &meta.issuer),
        None => !meta.authorization_response_iss_parameter_supported,
    };
    if !state_ok || !iss_ok {
        return Err(AppError::sync("cloud_state_mismatch"));
    }
    if let Some(error) = &params.error {
        return Err(if error == "access_denied" {
            AppError::sync("cloud_link_denied")
        } else {
            AppError::sync("cloud_request_invalid")
        });
    }
    params
        .code
        .clone()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| AppError::sync("cloud_request_invalid"))
}

/// A store the account may link (token response `stores[]`, `/v1/me`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct RemoteStore {
    /// DZBuild may send a number; the window always gets the text.
    #[serde(deserialize_with = "de_id")]
    #[specta(type = String)]
    pub store_id: String,
    pub store_name: String,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub public_url: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub stores: Vec<RemoteStore>,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "…"))
            .field("stores", &self.stores.len())
            .finish_non_exhaustive()
    }
}

/// Token-endpoint refusals (RFC 6749 §5.2) that end a refresh token for good: sending it again
/// only gets the same answer, so the link waits for the owner to link again.
const TERMINAL_REFUSALS: [&str; 3] = ["invalid_grant", "invalid_client", "unauthorized_client"];

/// A token-endpoint call; `invalid_grant` becomes `on_invalid_grant` (an expired code, or a
/// refresh token refused). With `terminal`, so do the other refusals that never change on a
/// retry ([`TERMINAL_REFUSALS`]) and a 401.
fn token_call(
    http: &Http,
    meta: &Metadata,
    form: Vec<(String, String)>,
    on_invalid_grant: &'static str,
) -> Result<TokenResponse, AppError> {
    token_call_with(http, meta, form, on_invalid_grant, false)
}

fn token_call_with(
    http: &Http,
    meta: &Metadata,
    form: Vec<(String, String)>,
    on_invalid_grant: &'static str,
    terminal: bool,
) -> Result<TokenResponse, AppError> {
    let call = Call::post(Endpoint::Token, "")
        .absolute(&meta.token_endpoint)
        .form(form)
        .lane(Lane::Interactive);
    match http.send(&call) {
        Ok(reply) => {
            let tokens: TokenResponse = reply.json().map_err(|e| e.to_app())?;
            if tokens.access_token.is_empty() || !tokens.token_type.eq_ignore_ascii_case("bearer") {
                return Err(AppError::sync("cloud_response_invalid"));
            }
            Ok(tokens)
        }
        Err(CallError::Rejected { code, status, .. }) => Err(match code.as_str() {
            "invalid_grant" => AppError::sync(on_invalid_grant),
            c if terminal && (TERMINAL_REFUSALS.contains(&c) || status == 401) => {
                AppError::sync(on_invalid_grant)
            }
            "access_denied" => AppError::sync("cloud_link_denied"),
            _ => AppError::sync("cloud_request_invalid"),
        }),
        Err(CallError::Unauthorized) if terminal => Err(AppError::sync(on_invalid_grant)),
        Err(e) => Err(e.to_app()),
    }
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

pub fn exchange_code(
    http: &Http,
    meta: &Metadata,
    config: &CloudConfig,
    code: &str,
    pkce: &Pkce,
    redirect_uri: &str,
) -> Result<TokenResponse, AppError> {
    token_call(
        http,
        meta,
        vec![
            pair("grant_type", "authorization_code"),
            pair("code", code),
            pair("redirect_uri", redirect_uri),
            pair("client_id", &config.client_id),
            pair("code_verifier", pkce.verifier()),
        ],
        "cloud_link_expired",
    )
}

pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

fn default_interval() -> u64 {
    5
}

/// RFC 8628 §3.2. The user code (e.g. `DZKM-4Q7P`) and the verification URI are shown to the
/// owner; the device code stays in this process.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

impl std::fmt::Debug for DeviceAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceAuthorization")
            .field("user_code", &self.user_code)
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

pub fn start_device(
    http: &Http,
    meta: &Metadata,
    config: &CloudConfig,
) -> Result<DeviceAuthorization, AppError> {
    let endpoint = meta
        .device_authorization_endpoint
        .as_deref()
        .ok_or_else(|| AppError::sync("cloud_not_ready"))?;
    let call = Call::post(Endpoint::DeviceAuthorization, "")
        .absolute(endpoint)
        .form(vec![
            pair("client_id", &config.client_id),
            pair("scope", &config.scope()),
        ])
        .lane(Lane::Interactive);
    let auth: DeviceAuthorization = http
        .send(&call)
        .and_then(|r| r.json())
        .map_err(|e| e.to_app())?;
    let uris_ok = check_url(&auth.verification_uri).is_ok()
        && auth
            .verification_uri_complete
            .as_deref()
            .is_none_or(|u| check_url(u).is_ok());
    if !uris_ok || auth.user_code.is_empty() || auth.expires_in == 0 {
        return Err(AppError::sync("cloud_response_invalid"));
    }
    Ok(auth)
}

pub enum DevicePoll {
    Pending,
    SlowDown,
    Granted(TokenResponse),
}

pub fn poll_device(
    http: &Http,
    meta: &Metadata,
    config: &CloudConfig,
    device_code: &str,
) -> Result<DevicePoll, AppError> {
    let call = Call::post(Endpoint::Token, "")
        .absolute(&meta.token_endpoint)
        .form(vec![
            pair("grant_type", DEVICE_GRANT),
            pair("device_code", device_code),
            pair("client_id", &config.client_id),
        ])
        .lane(Lane::Interactive);
    match http.send(&call) {
        Ok(reply) => {
            let tokens: TokenResponse = reply.json().map_err(|e| e.to_app())?;
            Ok(DevicePoll::Granted(tokens))
        }
        Err(CallError::Rejected { code, .. }) => match code.as_str() {
            "authorization_pending" => Ok(DevicePoll::Pending),
            "slow_down" => Ok(DevicePoll::SlowDown),
            "access_denied" => Err(AppError::sync("cloud_link_denied")),
            "expired_token" => Err(AppError::sync("cloud_link_expired")),
            _ => Err(AppError::sync("cloud_request_invalid")),
        },
        Err(e) => Err(e.to_app()),
    }
}

/// Polls every `interval` seconds (+5 s after each `slow_down`, RFC 8628 §3.5) until the owner
/// approves on the phone, refuses, the code expires, or the attempt is cancelled. A network
/// hiccup keeps polling until the deadline.
pub fn wait_device(
    http: &Http,
    meta: &Metadata,
    config: &CloudConfig,
    auth: &DeviceAuthorization,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<TokenResponse, AppError> {
    let clock = http.clock();
    let lifetime_ms = i64::try_from(auth.expires_in)
        .unwrap_or(600)
        .saturating_mul(1_000);
    let deadline = clock.now_ms().saturating_add(lifetime_ms);
    let mut interval = auth.interval.clamp(1, 60);
    loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(AppError::sync("cloud_link_cancelled"));
        }
        if clock.now_ms() >= deadline {
            return Err(AppError::sync("cloud_link_expired"));
        }
        clock.sleep(std::time::Duration::from_secs(interval));
        match poll_device(http, meta, config, &auth.device_code) {
            Ok(DevicePoll::Granted(tokens)) => return Ok(tokens),
            Ok(DevicePoll::Pending) => {}
            Ok(DevicePoll::SlowDown) => interval = (interval + 5).min(60),
            Err(e)
                if matches!(
                    e.code().as_str(),
                    "cloud_unavailable" | "cloud_rate_limited"
                ) => {}
            Err(e) => return Err(e),
        }
    }
}

/// A rotating refresh: the answer carries the next refresh token. `invalid_grant` (expired,
/// revoked, or reuse detected), `invalid_client`, `unauthorized_client` and a 401 are
/// `cloud_relink_required`: the refresh token is refused for good.
pub fn refresh(
    http: &Http,
    meta: &Metadata,
    config: &CloudConfig,
    refresh_token: &str,
) -> Result<TokenResponse, AppError> {
    token_call_with(
        http,
        meta,
        vec![
            pair("grant_type", "refresh_token"),
            pair("refresh_token", refresh_token),
            pair("client_id", &config.client_id),
        ],
        "cloud_relink_required",
        true,
    )
}

/// RFC 7009. Without a revocation endpoint there is nothing to call.
pub fn revoke(
    http: &Http,
    meta: &Metadata,
    config: &CloudConfig,
    refresh_token: &str,
) -> Result<(), AppError> {
    let Some(endpoint) = meta.revocation_endpoint.as_deref() else {
        return Ok(());
    };
    let call = Call::post(Endpoint::Revoke, "")
        .absolute(endpoint)
        .form(vec![
            pair("token", refresh_token),
            pair("token_type_hint", "refresh_token"),
            pair("client_id", &config.client_id),
        ])
        .lane(Lane::Interactive)
        .timeout(std::time::Duration::from_secs(5));
    http.send(&call).map(drop).map_err(|e| e.to_app())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UserInfo {
    #[serde(deserialize_with = "de_id")]
    pub sub: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

/// OIDC userinfo with the access token (the ID token is never used, so no JWKS code exists).
pub fn userinfo(
    http: &Http,
    meta: &Metadata,
    access_token: &str,
) -> Result<Option<UserInfo>, AppError> {
    let Some(endpoint) = meta.userinfo_endpoint.as_deref() else {
        return Ok(None);
    };
    let call = Call::get(Endpoint::UserInfo, "")
        .absolute(endpoint)
        .bearer(access_token)
        .lane(Lane::Interactive);
    http.send(&call)
        .and_then(|r| r.json())
        .map(Some)
        .map_err(|e| e.to_app())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cloud::http::ManualClock;
    use crate::cloud::testkit::MockDz;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, ResponseTemplate};

    /// RFC 8414 metadata of a mock issuer (every endpoint on the mock).
    pub(crate) fn metadata_json(issuer: &str) -> serde_json::Value {
        json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/oauth/authorize"),
            "token_endpoint": format!("{issuer}/oauth/token"),
            "device_authorization_endpoint": format!("{issuer}/oauth/device_authorization"),
            "revocation_endpoint": format!("{issuer}/oauth/revoke"),
            "userinfo_endpoint": format!("{issuer}/oauth/oidc/userinfo"),
            "code_challenge_methods_supported": ["S256"],
            "grant_types_supported": ["authorization_code", "refresh_token",
                                      "urn:ietf:params:oauth:grant-type:device_code"],
            "authorization_response_iss_parameter_supported": true
        })
    }

    pub(crate) fn metadata(issuer: &str) -> Metadata {
        serde_json::from_value(metadata_json(issuer)).unwrap()
    }

    fn query(url: &str) -> HashMap<String, String> {
        url::Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[test]
    fn discovery_checks_the_issuer_s256_and_every_endpoint() {
        let dz = MockDz::start();
        let well_known = "/.well-known/oauth-authorization-server";
        dz.mount(
            Mock::given(path(well_known))
                .respond_with(ResponseTemplate::new(200).set_body_json(metadata_json(&dz.uri()))),
        );
        let meta = discover(&dz.http(), &dz.config()).unwrap();
        assert_eq!(meta.token_endpoint, format!("{}/oauth/token", dz.uri()));

        let mut plain_only = metadata_json(&dz.uri());
        plain_only["code_challenge_methods_supported"] = json!(["plain"]);
        let mut foreign = metadata_json(&dz.uri());
        foreign["issuer"] = json!("https://evil.example");
        let mut http_endpoint = metadata_json(&dz.uri());
        http_endpoint["token_endpoint"] = json!("http://evil.example/oauth/token");
        for bad in [plain_only, foreign, http_endpoint] {
            dz.rt.block_on(dz.server.reset());
            dz.mount(
                Mock::given(path(well_known))
                    .respond_with(ResponseTemplate::new(200).set_body_json(bad)),
            );
            assert_eq!(
                discover(&dz.http(), &dz.config()).unwrap_err().code(),
                "cloud_discovery_invalid"
            );
        }
        dz.rt.block_on(dz.server.reset());
        dz.mount(Mock::given(path(well_known)).respond_with(ResponseTemplate::new(404)));
        assert_eq!(
            discover(&dz.http(), &dz.config()).unwrap_err().code(),
            "cloud_discovery_invalid"
        );
    }

    #[test]
    fn the_verifier_has_256_bits_and_the_challenge_is_s256() {
        let rfc = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(
            rfc.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            "RFC 7636 appendix B"
        );
        let (a, b) = (Pkce::generate().unwrap(), Pkce::generate().unwrap());
        assert_eq!(a.verifier().len(), 43, "32 random bytes");
        assert_ne!(a.verifier(), b.verifier());
        assert!(
            a.verifier()
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_eq!(random_token(32).unwrap().len(), 43);
    }

    #[test]
    fn the_authorization_url_is_a_public_pkce_request() {
        let meta = metadata("https://dzbuild.com");
        let config = CloudConfig::loopback("http://127.0.0.1:1");
        let request = authorization_url(
            &meta,
            &config,
            "http://127.0.0.1:53187/oauth/callback",
            Lang::Ar,
        )
        .unwrap();
        assert!(
            request
                .url
                .starts_with("https://dzbuild.com/oauth/authorize?")
        );
        let q = query(&request.url);
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["client_id"], "dzbuild-pos-windows");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:53187/oauth/callback");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["code_challenge"], request.pkce.challenge);
        assert_eq!(q["state"], request.state);
        assert_eq!(q["ui_locales"], "ar");
        assert!(q["scope"].split(' ').any(|s| s == "offline_access"));
        assert!(!q.contains_key("client_secret"));
        assert!(
            !request.url.contains(request.pkce.verifier()),
            "the verifier never leaves the PC"
        );
    }

    #[test]
    fn a_wrong_state_or_iss_is_refused() {
        let meta = metadata("https://dzbuild.com");
        let iss = "iss=https%3A%2F%2Fdzbuild.com";
        let ok = parse_callback(&format!("code=c1&state=S&{iss}"));
        assert_eq!(check_callback(&ok, "S", &meta).unwrap(), "c1");
        for (q, code) in [
            (format!("code=c1&state=T&{iss}"), "cloud_state_mismatch"),
            (format!("code=c1&{iss}"), "cloud_state_mismatch"),
            (
                "code=c1&state=S&iss=https%3A%2F%2Fevil.example".to_owned(),
                "cloud_state_mismatch",
            ),
            ("code=c1&state=S".to_owned(), "cloud_state_mismatch"),
            (
                format!("error=access_denied&state=S&{iss}"),
                "cloud_link_denied",
            ),
            (
                format!("error=server_error&state=S&{iss}"),
                "cloud_request_invalid",
            ),
            (format!("state=S&{iss}"), "cloud_request_invalid"),
        ] {
            assert_eq!(
                check_callback(&parse_callback(&q), "S", &meta)
                    .unwrap_err()
                    .code(),
                code,
                "{q}"
            );
        }
        let lenient = Metadata {
            authorization_response_iss_parameter_supported: false,
            ..meta
        };
        assert_eq!(
            check_callback(&parse_callback("code=c1&state=S"), "S", &lenient).unwrap(),
            "c1",
            "iss is optional when the server does not send it"
        );
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"abc", b"abc"));
    }

    #[test]
    fn the_code_exchange_proves_the_verifier_without_a_secret() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/oauth/token"))
                .and(body_string_contains("grant_type=authorization_code"))
                .and(body_string_contains("code=c1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "at_1", "token_type": "Bearer", "expires_in": 900,
                    "refresh_token": "rt_1", "scope": "offline_access store:read",
                    "stores": [{ "store_id": 141, "store_name": "Boutique Amel", "plan": "pro", "role": "owner" }]
                })))
                .up_to_n_times(1),
        );
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
        ));
        let pkce = Pkce::generate().unwrap();
        let redirect = "http://127.0.0.1:53187/oauth/callback";
        let tokens = exchange_code(&dz.http(), &meta, &dz.config(), "c1", &pkce, redirect).unwrap();
        assert_eq!(tokens.access_token, "at_1");
        assert_eq!(tokens.stores[0].store_id, "141");
        let form: HashMap<String, String> = url::form_urlencoded::parse(&dz.requests()[0].body)
            .into_owned()
            .collect();
        assert_eq!(form["code_verifier"], pkce.verifier());
        assert_eq!(form["client_id"], "dzbuild-pos-windows");
        assert_eq!(form["redirect_uri"], redirect);
        assert!(!form.contains_key("client_secret"));
        let reused =
            exchange_code(&dz.http(), &meta, &dz.config(), "c1", &pkce, redirect).unwrap_err();
        assert_eq!(
            reused.code(),
            "cloud_link_expired",
            "a code is single-use and lives 10 minutes"
        );
    }

    #[test]
    fn an_outage_is_not_a_bad_server_and_a_code_is_never_sent_twice() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        let http = dz.http_at(crate::cloud::http::ManualClock::at(0));
        dz.mount(
            Mock::given(path("/.well-known/oauth-authorization-server"))
                .respond_with(ResponseTemplate::new(503)),
        );
        dz.mount(Mock::given(path("/oauth/token")).respond_with(ResponseTemplate::new(503)));
        assert_eq!(
            discover(&http, &dz.config()).unwrap_err().code(),
            "cloud_unavailable"
        );
        let pkce = Pkce::generate().unwrap();
        let redirect = "http://127.0.0.1:53187/oauth/callback";
        let down = exchange_code(&http, &meta, &dz.config(), "c1", &pkce, redirect).unwrap_err();
        assert_eq!(down.code(), "cloud_unavailable");
        let sent = dz
            .requests()
            .iter()
            .filter(|r| r.url.path() == "/oauth/token")
            .count();
        assert_eq!(
            sent, 1,
            "a code that may have been redeemed is not replayed"
        );

        for (reply, code) in [
            (
                ResponseTemplate::new(400).set_body_json(json!({ "error": "access_denied" })),
                "cloud_link_denied",
            ),
            (
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "access_token": "at_1", "token_type": "mac" })),
                "cloud_response_invalid",
            ),
            (
                ResponseTemplate::new(200).set_body_string("<html>"),
                "cloud_response_invalid",
            ),
        ] {
            dz.rt.block_on(dz.server.reset());
            dz.mount(Mock::given(path("/oauth/token")).respond_with(reply));
            let error =
                exchange_code(&http, &meta, &dz.config(), "c1", &pkce, redirect).unwrap_err();
            assert_eq!(error.code(), code);
        }
    }

    #[test]
    fn device_flow_waits_slows_down_and_gets_tokens() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(
            Mock::given(path("/oauth/device_authorization"))
                .and(body_string_contains("client_id=dzbuild-pos-windows"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "device_code": "dc_1", "user_code": "DZKM-4Q7P",
                    "verification_uri": "https://dzbuild.com/device",
                    "verification_uri_complete": "https://dzbuild.com/device?code=DZKM-4Q7P",
                    "expires_in": 600, "interval": 5
                }))),
        );
        let grant = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code";
        for (answer, times) in [("authorization_pending", 1), ("slow_down", 1)] {
            dz.mount(
                Mock::given(path("/oauth/token"))
                    .and(body_string_contains(grant))
                    .respond_with(
                        ResponseTemplate::new(400).set_body_json(json!({ "error": answer })),
                    )
                    .up_to_n_times(times),
            );
        }
        dz.mount(
            Mock::given(path("/oauth/token")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_1", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_1"
            }))),
        );
        let clock = ManualClock::at(0);
        let http = dz.http_at(clock.clone());
        let auth = start_device(&http, &meta, &dz.config()).unwrap();
        assert_eq!(auth.user_code, "DZKM-4Q7P");
        let tokens =
            wait_device(&http, &meta, &dz.config(), &auth, &AtomicBool::new(false)).unwrap();
        assert_eq!(tokens.access_token, "at_1");
        assert_eq!(
            clock.slept(),
            [
                Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(10)
            ],
            "interval, interval, then +5 s after slow_down"
        );
    }

    #[test]
    fn a_denied_or_expired_device_code_stops_the_wait() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "access_denied" })),
        ));
        let auth = DeviceAuthorization {
            device_code: "dc".into(),
            user_code: "DZKM-4Q7P".into(),
            verification_uri: "https://dzbuild.com/device".into(),
            verification_uri_complete: None,
            expires_in: 600,
            interval: 5,
        };
        let http = dz.http_at(ManualClock::at(0));
        let denied =
            wait_device(&http, &meta, &dz.config(), &auth, &AtomicBool::new(false)).unwrap_err();
        assert_eq!(denied.code(), "cloud_link_denied");
        let short = DeviceAuthorization {
            expires_in: 3,
            ..auth
        };
        dz.rt.block_on(dz.server.reset());
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "authorization_pending" })),
        ));
        let expired =
            wait_device(&http, &meta, &dz.config(), &short, &AtomicBool::new(false)).unwrap_err();
        assert_eq!(expired.code(), "cloud_link_expired");
        let no_device = Metadata {
            device_authorization_endpoint: None,
            ..meta
        };
        assert_eq!(
            start_device(&http, &no_device, &dz.config())
                .unwrap_err()
                .code(),
            "cloud_not_ready"
        );
    }

    #[test]
    fn revoke_names_the_refresh_token_and_userinfo_is_optional() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(
            Mock::given(path("/oauth/revoke"))
                .and(body_string_contains("token=rt_1"))
                .and(body_string_contains("token_type_hint=refresh_token"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1),
        );
        dz.mount(
            Mock::given(path("/oauth/oidc/userinfo")).respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "sub": "84211", "name": "Amel", "locale": "ar-DZ" })),
            ),
        );
        let http = dz.http();
        revoke(&http, &meta, &dz.config(), "rt_1").unwrap();
        let info = userinfo(&http, &meta, "at_1").unwrap().unwrap();
        assert_eq!(
            (info.sub.as_str(), info.locale.as_deref()),
            ("84211", Some("ar-DZ"))
        );
        let bare = Metadata {
            userinfo_endpoint: None,
            revocation_endpoint: None,
            ..meta
        };
        assert!(userinfo(&http, &bare, "at_1").unwrap().is_none());
        revoke(&http, &bare, &dz.config(), "rt_1").unwrap();
        dz.verify();
    }
}

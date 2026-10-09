//! The link's tokens (spec §4.7): the access token (≈ 15 min) in memory only, the refresh token
//! (rotating) only in the secret store. One refresh at a time — with rotation and reuse
//! detection, two parallel refreshes would log the shop out —, proactive past 80 % of the
//! access token's life, forced after a 401. Offline, a still-valid access token keeps working;
//! a refused refresh forgets both tokens and asks for a new link. `Debug` prints no token.

use super::config::CloudConfig;
use super::http::{Clock, Http};
use super::oauth::{self, Metadata, TokenResponse};
use super::secrets::{SecretStore, refresh_account};
use crate::error::AppError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

struct AccessToken {
    value: String,
    issued_at: i64,
    expires_at: i64,
}

impl AccessToken {
    fn fresh(&self, now: i64) -> bool {
        now < self.issued_at + (self.expires_at - self.issued_at) * 8 / 10
    }
    fn usable(&self, now: i64) -> bool {
        now + 5_000 < self.expires_at
    }
}

pub struct TokenManager {
    account: String,
    secrets: Arc<dyn SecretStore>,
    clock: Arc<dyn Clock>,
    access: Mutex<Option<AccessToken>>,
    refreshing: Mutex<()>,
    /// DZBuild refused the refresh token for good: it is never sent again, even when the
    /// secret store could not delete it, until a new grant is stored.
    refused: AtomicBool,
    /// A rotated refresh token the secret store could not keep: used instead of the stored
    /// one (which DZBuild no longer accepts) for as long as this process runs.
    rotated: Mutex<Option<String>>,
}

impl std::fmt::Debug for TokenManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenManager")
            .field("account", &self.account)
            .field("expires_at", &self.expires_at())
            .finish_non_exhaustive()
    }
}

impl TokenManager {
    pub fn new(
        link_id: &str,
        secrets: Arc<dyn SecretStore>,
        clock: Arc<dyn Clock>,
    ) -> TokenManager {
        TokenManager {
            account: refresh_account(link_id),
            secrets,
            clock,
            access: Mutex::new(None),
            refreshing: Mutex::new(()),
            refused: AtomicBool::new(false),
            rotated: Mutex::new(None),
        }
    }

    pub fn account(&self) -> &str {
        &self.account
    }

    /// Keeps a grant: the refresh token in the secret store first (a grant without one is
    /// refused: `offline_access` is required), then the access token in memory.
    pub fn store(&self, grant: &TokenResponse) -> Result<(), AppError> {
        let refresh = grant
            .refresh_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AppError::sync("cloud_response_invalid"))?;
        self.secrets.set(&self.account, refresh)?;
        self.refused.store(false, Ordering::SeqCst);
        *self.rotated.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.set_access(grant);
        Ok(())
    }

    fn set_access(&self, grant: &TokenResponse) {
        let now = self.clock.now_ms();
        let life =
            i64::try_from(grant.expires_in.unwrap_or(900).clamp(60, 86_400)).unwrap_or(900) * 1_000;
        *self.access.lock().unwrap_or_else(PoisonError::into_inner) = Some(AccessToken {
            value: grant.access_token.clone(),
            issued_at: now,
            expires_at: now + life,
        });
    }

    fn with_access<T>(&self, f: impl FnOnce(&AccessToken, i64) -> Option<T>) -> Option<T> {
        let now = self.clock.now_ms();
        self.access
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(|t| f(t, now))
    }

    fn fresh(&self) -> Option<String> {
        self.with_access(|t, now| t.fresh(now).then(|| t.value.clone()))
    }

    fn usable(&self) -> Option<String> {
        self.with_access(|t, now| t.usable(now).then(|| t.value.clone()))
    }

    pub fn expires_at(&self) -> Option<i64> {
        self.with_access(|t, _| Some(t.expires_at))
    }

    /// A usable access token, refreshing when past 80 % of its life (single-flight).
    pub fn access_token(
        &self,
        http: &Http,
        meta: &Metadata,
        config: &CloudConfig,
    ) -> Result<String, AppError> {
        if let Some(token) = self.fresh() {
            return Ok(token);
        }
        let _one_at_a_time = self
            .refreshing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(token) = self.fresh() {
            return Ok(token); // another thread refreshed while this one waited
        }
        self.refresh_locked(http, meta, config)
    }

    /// After a 401 on `rejected`: refresh, unless another thread already replaced that token.
    pub fn force_refresh(
        &self,
        http: &Http,
        meta: &Metadata,
        config: &CloudConfig,
        rejected: &str,
    ) -> Result<String, AppError> {
        let _one_at_a_time = self
            .refreshing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(token) = self.usable().filter(|t| t != rejected) {
            return Ok(token);
        }
        *self.access.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.refresh_locked(http, meta, config)
    }

    fn refresh_locked(
        &self,
        http: &Http,
        meta: &Metadata,
        config: &CloudConfig,
    ) -> Result<String, AppError> {
        if self.refused.load(Ordering::SeqCst) {
            *self.access.lock().unwrap_or_else(PoisonError::into_inner) = None;
            return Err(AppError::sync("cloud_relink_required"));
        }
        let kept = self
            .rotated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let refresh = match kept {
            Some(token) => token,
            None => match self.secrets.get(&self.account)? {
                Some(token) => token,
                None => {
                    *self.access.lock().unwrap_or_else(PoisonError::into_inner) = None;
                    return Err(AppError::sync("cloud_relink_required"));
                }
            },
        };
        match oauth::refresh(http, meta, config, &refresh) {
            Ok(grant) => {
                if let Some(next) = grant.refresh_token.as_deref().filter(|t| !t.is_empty()) {
                    let mut rotated = self.rotated.lock().unwrap_or_else(PoisonError::into_inner);
                    match self.secrets.set(&self.account, next) {
                        Ok(()) => *rotated = None,
                        Err(error) => {
                            // DZBuild already retired the old one: keep the new one here, or the
                            // next refresh would present a token DZBuild refuses (reuse).
                            tracing::warn!(target: "cloud", code = %error.code(), "the rotated refresh token could not be kept; it stays in memory");
                            *rotated = Some(next.to_owned());
                        }
                    }
                }
                self.set_access(&grant);
                Ok(grant.access_token)
            }
            Err(e) if e.code() == "cloud_relink_required" => {
                self.refused.store(true, Ordering::SeqCst);
                *self.rotated.lock().unwrap_or_else(PoisonError::into_inner) = None;
                if let Err(error) = self.forget() {
                    // Never sent again all the same (`refused`); `link::suspend` says so.
                    tracing::warn!(target: "cloud", code = %error.code(), "the refused refresh token could not be deleted");
                }
                Err(e)
            }
            Err(e) => self.usable().ok_or(e),
        }
    }

    /// DZBuild refused the refresh token for good in this process.
    pub fn is_refused(&self) -> bool {
        self.refused.load(Ordering::SeqCst)
    }

    pub fn refresh_token(&self) -> Result<Option<String>, AppError> {
        let kept = self
            .rotated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match kept {
            Some(token) => Ok(Some(token)),
            None => self.secrets.get(&self.account),
        }
    }

    pub fn has_refresh(&self) -> Result<bool, AppError> {
        Ok(self.refresh_token()?.is_some())
    }

    /// Forgets the access token and deletes the refresh token from the secret store.
    pub fn forget(&self) -> Result<(), AppError> {
        *self.access.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.secrets.delete(&self.account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::http::{ManualClock, SystemClock};
    use crate::cloud::oauth::tests::metadata;
    use crate::cloud::oauth::{DeviceAuthorization, Pkce, TokenResponse, parse_callback};
    use crate::cloud::secrets::MemorySecrets;
    use crate::cloud::testkit::MockDz;
    use serde_json::json;
    use std::time::Duration;
    use wiremock::matchers::{body_string_contains, path};
    use wiremock::{Mock, ResponseTemplate};

    const LINK: &str = "00000000-0000-7000-8000-00000000c001";

    fn grant(access: &str, refresh: Option<&str>) -> TokenResponse {
        TokenResponse {
            access_token: access.into(),
            token_type: "Bearer".into(),
            expires_in: Some(900),
            refresh_token: refresh.map(str::to_owned),
            scope: None,
            stores: Vec::new(),
        }
    }

    fn tokens_at(clock: Arc<ManualClock>) -> (Arc<MemorySecrets>, TokenManager) {
        let secrets = Arc::new(MemorySecrets::default());
        let manager = TokenManager::new(LINK, secrets.clone(), clock);
        (secrets, manager)
    }

    #[test]
    fn a_grant_without_a_refresh_token_is_refused() {
        let (_, manager) = tokens_at(ManualClock::at(0));
        assert_eq!(
            manager.store(&grant("at_1", None)).unwrap_err().code(),
            "cloud_response_invalid"
        );
    }

    #[test]
    fn the_access_token_is_reused_until_80_percent_of_its_life() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(
            Mock::given(path("/oauth/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .and(body_string_contains("refresh_token=rt_1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "at_2", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_2"
                })))
                .expect(1),
        );
        let clock = ManualClock::at(0);
        let (secrets, manager) = tokens_at(clock.clone());
        manager.store(&grant("at_1", Some("rt_1"))).unwrap();
        let http = dz.http_at(clock.clone());
        clock.advance(11 * 60_000);
        assert_eq!(
            manager.access_token(&http, &meta, &dz.config()).unwrap(),
            "at_1",
            "11 of 15 minutes"
        );
        clock.advance(2 * 60_000);
        assert_eq!(
            manager.access_token(&http, &meta, &dz.config()).unwrap(),
            "at_2",
            "past 12 minutes"
        );
        assert_eq!(
            secrets.get(manager.account()).unwrap().as_deref(),
            Some("rt_2"),
            "rotated"
        );
        dz.verify();
    }

    #[test]
    fn concurrent_refreshes_rotate_once() {
        let dz = MockDz::start();
        let meta = Arc::new(metadata(&dz.uri()));
        dz.mount(
            Mock::given(path("/oauth/token"))
                .and(body_string_contains("refresh_token=rt_1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(300))
                        .set_body_json(json!({
                            "access_token": "at_2", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_2"
                        })),
                )
                .up_to_n_times(1),
        );
        // Reuse detection: a second refresh with rt_1 would log the till out.
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
        ));
        let secrets = Arc::new(MemorySecrets::default());
        secrets.set(&refresh_account(LINK), "rt_1").unwrap();
        let manager = Arc::new(TokenManager::new(
            LINK,
            secrets.clone(),
            Arc::new(SystemClock),
        ));
        let http = Arc::new(dz.http());
        let config = Arc::new(dz.config());
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let (manager, http, meta, config) =
                    (manager.clone(), http.clone(), meta.clone(), config.clone());
                std::thread::spawn(move || manager.access_token(&http, &meta, &config))
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().unwrap(), "at_2");
        }
        assert_eq!(dz.requests().len(), 1, "single-flight refresh");
        assert_eq!(
            secrets.get(&refresh_account(LINK)).unwrap().as_deref(),
            Some("rt_2")
        );
    }

    #[test]
    fn a_refused_refresh_forgets_every_token() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
        ));
        let clock = ManualClock::at(0);
        let (secrets, manager) = tokens_at(clock.clone());
        manager.store(&grant("at_1", Some("rt_1"))).unwrap();
        clock.advance(14 * 60_000);
        let err = manager
            .access_token(&dz.http_at(clock.clone()), &meta, &dz.config())
            .unwrap_err();
        assert_eq!(err.code(), "cloud_relink_required");
        assert_eq!(
            secrets.get(manager.account()).unwrap(),
            None,
            "the credential is deleted"
        );
        assert_eq!(manager.expires_at(), None, "the access token is forgotten");
    }

    /// A store whose delete fails (a locked Credential Manager).
    #[derive(Debug, Default)]
    struct StuckSecrets(MemorySecrets);

    impl SecretStore for StuckSecrets {
        fn get(&self, account: &str) -> Result<Option<String>, AppError> {
            self.0.get(account)
        }
        fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
            self.0.set(account, secret)
        }
        fn delete(&self, _: &str) -> Result<(), AppError> {
            Err(AppError::sync("cloud_secret_store_failed"))
        }
    }

    /// `invalid_client` and a bare 401 on the token endpoint end the refresh token as
    /// `invalid_grant` does: a relink, never the same token sent again forever.
    #[test]
    fn every_terminal_refusal_of_a_refresh_asks_for_a_relink() {
        for (status, body) in [
            (400, json!({ "error": "invalid_client" })),
            (401, json!({ "error": "invalid_client" })),
            (401, json!({})),
            (400, json!({ "error": "unauthorized_client" })),
        ] {
            let dz = MockDz::start();
            let meta = metadata(&dz.uri());
            dz.mount(
                Mock::given(path("/oauth/token"))
                    .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
                    .expect(1),
            );
            let clock = ManualClock::at(0);
            let (secrets, manager) = tokens_at(clock.clone());
            manager.store(&grant("at_1", Some("rt_1"))).unwrap();
            clock.advance(16 * 60_000);
            let http = dz.http_at(clock.clone());
            assert_eq!(
                manager
                    .access_token(&http, &meta, &dz.config())
                    .unwrap_err()
                    .code(),
                "cloud_relink_required",
                "{status} {body}"
            );
            assert_eq!(secrets.get(manager.account()).unwrap(), None);
            dz.verify();
        }
    }

    /// A refused refresh whose credential cannot be deleted is still a relink, and the refused
    /// token is never sent again.
    #[test]
    fn a_refused_token_the_store_cannot_delete_is_never_sent_again() {
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(
            Mock::given(path("/oauth/token"))
                .respond_with(
                    ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
                )
                .expect(1),
        );
        let clock = ManualClock::at(0);
        let secrets = Arc::new(StuckSecrets::default());
        let manager = TokenManager::new(LINK, secrets.clone(), clock.clone());
        manager.store(&grant("at_1", Some("rt_1"))).unwrap();
        clock.advance(16 * 60_000);
        let http = dz.http_at(clock.clone());
        for _ in 0..3 {
            assert_eq!(
                manager
                    .access_token(&http, &meta, &dz.config())
                    .unwrap_err()
                    .code(),
                "cloud_relink_required"
            );
        }
        assert!(manager.is_refused());
        dz.verify();
    }

    /// The rotated refresh token is kept in memory when the store refuses it: the next refresh
    /// presents it, not the retired one.
    #[test]
    fn a_rotated_token_the_store_cannot_keep_stays_in_memory() {
        #[derive(Debug, Default)]
        struct ReadOnly(MemorySecrets, std::sync::atomic::AtomicBool);
        impl SecretStore for ReadOnly {
            fn get(&self, account: &str) -> Result<Option<String>, AppError> {
                self.0.get(account)
            }
            fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
                if self.1.load(Ordering::SeqCst) {
                    return Err(AppError::sync("cloud_secret_store_failed"));
                }
                self.0.set(account, secret)
            }
            fn delete(&self, account: &str) -> Result<(), AppError> {
                self.0.delete(account)
            }
        }
        let dz = MockDz::start();
        let meta = metadata(&dz.uri());
        dz.mount(
            Mock::given(path("/oauth/token"))
                .and(body_string_contains("refresh_token=rt_1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "at_2", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_2"
                })))
                .expect(1),
        );
        dz.mount(
            Mock::given(path("/oauth/token"))
                .and(body_string_contains("refresh_token=rt_2"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "at_3", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_3"
                })))
                .expect(1),
        );
        let clock = ManualClock::at(0);
        let secrets = Arc::new(ReadOnly::default());
        let manager = TokenManager::new(LINK, secrets.clone(), clock.clone());
        manager.store(&grant("at_1", Some("rt_1"))).unwrap();
        secrets.1.store(true, Ordering::SeqCst);
        let http = dz.http_at(clock.clone());
        clock.advance(16 * 60_000);
        assert_eq!(
            manager.access_token(&http, &meta, &dz.config()).unwrap(),
            "at_2"
        );
        clock.advance(16 * 60_000);
        assert_eq!(
            manager.access_token(&http, &meta, &dz.config()).unwrap(),
            "at_3",
            "rt_2 from memory, not the retired rt_1"
        );
        dz.verify();
    }

    #[test]
    fn offline_keeps_a_still_valid_access_token() {
        let clock = ManualClock::at(0);
        let (_, manager) = tokens_at(clock.clone());
        manager.store(&grant("at_1", Some("rt_1"))).unwrap();
        clock.advance(13 * 60_000);
        let config = CloudConfig::loopback("http://127.0.0.1:9");
        let http = Http::new(&config, clock.clone(), "t".into()).unwrap();
        let meta = metadata("http://127.0.0.1:9");
        assert_eq!(
            manager.access_token(&http, &meta, &config).unwrap(),
            "at_1",
            "no network, 2 minutes left"
        );
        clock.advance(3 * 60_000);
        assert_eq!(
            manager
                .access_token(&http, &meta, &config)
                .unwrap_err()
                .code(),
            "cloud_unavailable"
        );
    }

    #[test]
    fn token_types_never_print_their_values() {
        let (_, manager) = tokens_at(ManualClock::at(0));
        manager
            .store(&grant("at_SECRET", Some("rt_SECRET")))
            .unwrap();
        let device = DeviceAuthorization {
            device_code: "dc_SECRET".into(),
            user_code: "DZKM-4Q7P".into(),
            verification_uri: "https://dzbuild.com/device".into(),
            verification_uri_complete: None,
            expires_in: 600,
            interval: 5,
        };
        for text in [
            format!("{:?}", grant("at_SECRET", Some("rt_SECRET"))),
            format!("{:?}", Pkce::from_verifier("v_SECRET_verifier".into())),
            format!("{device:?}"),
            format!(
                "{:?}",
                parse_callback("code=code_SECRET&state=state_SECRET")
            ),
            format!("{manager:?}"),
        ] {
            assert!(!text.contains("SECRET"), "{text}");
        }
    }
}

//! Linking a till to a DZBuild store (spec §4.7, research 06 §5.3–5.4). A link attempt — the
//! system browser with a loopback redirect, or the device grant approved from a phone — yields
//! tokens and the account's stores; with the store chosen: the refresh token goes to the secret
//! store, then the store profile, this till's device record (kept by a relink after a refused
//! refresh, registered anew after an unlink or a revocation) and location are fetched (each
//! tolerated when DZBuild has not shipped it yet), then the link row, the change-feed reader and
//! the audit row are written in one transaction. Unlink revokes and deletes; a refused refresh or
//! a revocation keeps the outbox and the map, so linking the same store again resumes.

use super::api::{self, Remote, ready};
use super::config::CloudConfig;
use super::http::{Call, CallError, Clock, Endpoint, Http, Lane};
use super::loopback::{Loopback, Pages, WAIT_LIMIT};
use super::oauth::{self, Metadata, RemoteStore, TokenResponse};
use super::secrets::SecretStore;
use super::store::{self, CloudLink, CloudNoticeKind, LinkStatus, NewLink};
use super::tokens::TokenManager;
use crate::audit::{self, Actor};
use crate::db::{self, Db};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::sync::changelog;
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

pub use super::api::DeviceInfo;

pub struct LinkContext {
    pub db: Db,
    pub http: Arc<Http>,
    pub config: Arc<CloudConfig>,
    pub secrets: Arc<dyn SecretStore>,
    pub device: DeviceInfo,
    /// Called after a link, an unlink, a relink request or a revocation: the engine reloads.
    pub on_change: Arc<dyn Fn() + Send + Sync>,
}

impl LinkContext {
    fn clock(&self) -> Arc<dyn Clock> {
        self.http.clock()
    }
    fn actor(&self) -> Actor {
        Actor::system(Some(self.device.terminal_id.clone()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AttemptKind {
    Browser,
    Device,
}

/// One attempt at a time, behind one mutex: the size of `Linked` costs nothing worth a box.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AttemptState {
    Waiting,
    ChooseStore { stores: Vec<RemoteStore> },
    Finishing,
    Linked { link: CloudLink },
    Failed { code: String },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeView {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttemptView {
    pub id: String,
    pub kind: AttemptKind,
    pub state: AttemptState,
    /// The browser flow's URL, shown with "copy" for a PC whose default browser will not open.
    pub authorize_url: Option<String>,
    pub device: Option<DeviceCodeView>,
    pub expires_at: i64,
}

/// Tokens and stores between the consent and the store choice (memory only, ≤ 10 minutes).
struct Granted {
    tokens: TokenResponse,
    meta: Metadata,
    stores: Vec<RemoteStore>,
    sub: String,
    name: Option<String>,
}

pub struct LinkAttempt {
    id: String,
    kind: AttemptKind,
    expires_at: i64,
    authorize_url: Option<String>,
    device: Option<DeviceCodeView>,
    state: Mutex<AttemptState>,
    cancel: Arc<AtomicBool>,
    granted: Mutex<Option<Granted>>,
    actor: Actor,
}

/// Redacted: the pending tokens and the authorization URL (it carries `state`) never print.
impl std::fmt::Debug for LinkAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkAttempt")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("expires_at", &self.expires_at)
            .field(
                "state",
                &*self.state.lock().unwrap_or_else(PoisonError::into_inner),
            )
            .finish_non_exhaustive()
    }
}

impl LinkAttempt {
    fn new(
        kind: AttemptKind,
        expires_at: i64,
        authorize_url: Option<String>,
        device: Option<DeviceCodeView>,
        actor: Actor,
    ) -> LinkAttempt {
        LinkAttempt {
            id: new_id(),
            kind,
            expires_at,
            authorize_url,
            device,
            state: Mutex::new(AttemptState::Waiting),
            cancel: Arc::new(AtomicBool::new(false)),
            granted: Mutex::new(None),
            actor,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn view(&self) -> AttemptView {
        AttemptView {
            id: self.id.clone(),
            kind: self.kind,
            state: self
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            authorize_url: self.authorize_url.clone(),
            device: self.device.clone(),
            expires_at: self.expires_at,
        }
    }

    fn set(&self, state: AttemptState) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }

    /// Stops the wait (the listener closes, the polling stops) and drops any pending tokens.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        *self.granted.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(
            *state,
            AttemptState::Waiting | AttemptState::ChooseStore { .. }
        ) {
            *state = AttemptState::Cancelled;
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(
            *self.state.lock().unwrap_or_else(PoisonError::into_inner),
            AttemptState::Linked { .. } | AttemptState::Failed { .. } | AttemptState::Cancelled
        )
    }

    fn settle(&self, ctx: &Arc<LinkContext>, result: Result<Granted, AppError>) {
        match result {
            Err(e) if e.code() == "cloud_link_cancelled" => self.set(AttemptState::Cancelled),
            Err(e) => self.set(AttemptState::Failed { code: e.code() }),
            Ok(granted) => {
                let only = (granted.stores.len() == 1).then(|| granted.stores[0].store_id.clone());
                let stores = granted.stores.clone();
                *self.granted.lock().unwrap_or_else(PoisonError::into_inner) = Some(granted);
                match only {
                    Some(store_id) => {
                        let _ = self.finish_with(ctx, &store_id);
                    }
                    None => self.set(AttemptState::ChooseStore { stores }),
                }
            }
        }
    }

    fn finish_with(&self, ctx: &Arc<LinkContext>, store_id: &str) -> Result<CloudLink, AppError> {
        if ctx.clock().now_ms() > self.expires_at {
            self.cancel();
            self.set(AttemptState::Failed {
                code: "cloud_link_expired".into(),
            });
            return Err(AppError::sync("cloud_link_expired"));
        }
        let granted = {
            let mut slot = self.granted.lock().unwrap_or_else(PoisonError::into_inner);
            let chosen = slot
                .as_ref()
                .and_then(|g| g.stores.iter().find(|s| s.store_id == store_id).cloned());
            let Some(store) = chosen else {
                return Err(AppError::sync("cloud_store_not_found"));
            };
            slot.take().map(|g| (g, store))
        };
        let Some((granted, store)) = granted else {
            return Err(AppError::sync("cloud_attempt_not_found"));
        };
        self.set(AttemptState::Finishing);
        match finish(ctx, &self.actor, granted, &store) {
            Ok(link) => {
                self.set(AttemptState::Linked { link: link.clone() });
                (ctx.on_change)();
                Ok(link)
            }
            Err(e) => {
                self.set(AttemptState::Failed { code: e.code() });
                Err(e)
            }
        }
    }
}

/// Linked already (and not waiting for a relink): refused.
fn refuse_if_linked(db: &Db) -> Result<(), AppError> {
    match db.read(store::active_link)? {
        Some(link) if link.status == LinkStatus::Linked => {
            Err(AppError::sync("cloud_already_linked"))
        }
        _ => Ok(()),
    }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> Result<(), AppError> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(f)
        .map(drop)
        .map_err(|e| AppError::Internal(format!("link thread: {e}")))
}

/// Opens the loopback listener and prepares the authorization URL; the caller opens it in the
/// system browser (never a WebView). The attempt settles on its own thread.
pub fn start_browser(
    ctx: Arc<LinkContext>,
    actor: Actor,
    lang: Lang,
    pages: Pages,
) -> Result<Arc<LinkAttempt>, AppError> {
    refuse_if_linked(&ctx.db)?;
    let meta = oauth::discover(&ctx.http, &ctx.config)?;
    let loopback = Loopback::bind()?;
    let request = oauth::authorization_url(&meta, &ctx.config, &loopback.redirect_uri(), lang)?;
    let expires_at =
        ctx.clock().now_ms() + i64::try_from(WAIT_LIMIT.as_millis()).unwrap_or(600_000);
    let attempt = Arc::new(LinkAttempt::new(
        AttemptKind::Browser,
        expires_at,
        Some(request.url.clone()),
        None,
        actor,
    ));
    let (worker, thread_ctx) = (attempt.clone(), ctx.clone());
    spawn("pos-cloud-link", move || {
        let result = loopback
            .wait(&pages, WAIT_LIMIT, &worker.cancel)
            .and_then(|params| oauth::check_callback(&params, &request.state, &meta))
            .and_then(|code| {
                oauth::exchange_code(
                    &thread_ctx.http,
                    &meta,
                    &thread_ctx.config,
                    &code,
                    &request.pkce,
                    &request.redirect_uri,
                )
            })
            .and_then(|tokens| granted(&thread_ctx, meta.clone(), tokens));
        worker.settle(&thread_ctx, result);
    })?;
    Ok(attempt)
}

/// The device grant: the owner approves on a phone (code and QR on screen).
pub fn start_device(ctx: Arc<LinkContext>, actor: Actor) -> Result<Arc<LinkAttempt>, AppError> {
    refuse_if_linked(&ctx.db)?;
    let meta = oauth::discover(&ctx.http, &ctx.config)?;
    let auth = oauth::start_device(&ctx.http, &meta, &ctx.config)?;
    let expires_at = ctx.clock().now_ms() + i64::try_from(auth.expires_in).unwrap_or(600) * 1_000;
    let view = DeviceCodeView {
        user_code: auth.user_code.clone(),
        verification_uri: auth.verification_uri.clone(),
        verification_uri_complete: auth.verification_uri_complete.clone(),
        expires_at,
    };
    let attempt = Arc::new(LinkAttempt::new(
        AttemptKind::Device,
        expires_at,
        None,
        Some(view),
        actor,
    ));
    let (worker, thread_ctx) = (attempt.clone(), ctx.clone());
    spawn("pos-cloud-link", move || {
        let result = oauth::wait_device(
            &thread_ctx.http,
            &meta,
            &thread_ctx.config,
            &auth,
            &worker.cancel,
        )
        .and_then(|tokens| granted(&thread_ctx, meta.clone(), tokens));
        worker.settle(&thread_ctx, result);
    })?;
    Ok(attempt)
}

/// Who signed in and which stores the account may link: `/v1/me`, else userinfo and the
/// token response's `stores` while DZBuild has not shipped `/v1/me`.
fn granted(ctx: &LinkContext, meta: Metadata, tokens: TokenResponse) -> Result<Granted, AppError> {
    let call = Call::get(Endpoint::Me, "/me")
        .bearer(&tokens.access_token)
        .lane(Lane::Interactive);
    let me = match ctx.http.send(&call).and_then(|r| r.data::<api::Me>()) {
        Ok(me) => Some(me),
        Err(CallError::NotReady { .. }) => None,
        Err(e) => return Err(e.to_app()),
    };
    let (sub, name, mut stores) = match me {
        Some(me) => (me.sub, me.name, me.stores),
        None => {
            let info = oauth::userinfo(&ctx.http, &meta, &tokens.access_token)?
                .ok_or_else(|| AppError::sync("cloud_response_invalid"))?;
            (info.sub, info.name, Vec::new())
        }
    };
    if stores.is_empty() {
        stores = tokens.stores.clone();
    }
    if stores.is_empty() {
        return Err(AppError::sync("cloud_store_not_found"));
    }
    Ok(Granted {
        tokens,
        meta,
        stores,
        sub,
        name,
    })
}

/// The owner picked a store in the list.
pub fn choose_store(
    ctx: &Arc<LinkContext>,
    attempt: &LinkAttempt,
    store_id: &str,
) -> Result<CloudLink, AppError> {
    if !matches!(attempt.view().state, AttemptState::ChooseStore { .. }) {
        return Err(AppError::sync("cloud_attempt_not_found"));
    }
    attempt.finish_with(ctx, store_id)
}

struct RemoteState {
    profile: Option<api::StoreProfile>,
    device_id: Option<String>,
    location_id: Option<String>,
}

/// How many of this till's DZBuild registrations have ended: the `cloud.unlinked` and
/// `cloud.revoked` rows of the append-only audit journal (indexed by action). It names the next
/// device registration in its idempotency key: a link after an unlink or a revocation takes a
/// new key, a retry of a failed attempt keeps its own.
fn registration_generation(conn: &rusqlite::Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM audit_log WHERE action IN ('cloud.unlinked', 'cloud.revoked')",
        [],
        |r| r.get(0),
    )?)
}

/// `kept_device`: a relink after a refused refresh keeps the device DZBuild still has.
fn register_remote(
    ctx: &LinkContext,
    remote: &Remote,
    kept_device: Option<String>,
) -> Result<RemoteState, AppError> {
    let (install_id, generation) = ctx.db.read(|c| {
        Ok((
            db::meta_get(c, db::META_INSTALL_ID)?.unwrap_or_default(),
            registration_generation(c)?,
        ))
    })?;
    let profile = ready(api::store_profile(remote))?;
    if profile
        .as_ref()
        .and_then(|p| p.currency.as_deref())
        .is_some_and(|c| c != "DZD")
    {
        return Err(AppError::sync("cloud_request_invalid"));
    }
    let device_id = match kept_device {
        Some(id) => Some(id),
        None => ready(api::register_device(
            remote,
            &ctx.device,
            &install_id,
            generation,
        ))?,
    };
    let shop = ctx
        .db
        .read(|c| Ok(crate::company::get(c)?.map(|co| co.name_fr)))?
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| ctx.device.terminal_code.clone());
    let location_id = ready(api::ensure_location(remote, &install_id, &shop))?;
    Ok(RemoteState {
        profile,
        device_id,
        location_id,
    })
}

fn finish(
    ctx: &Arc<LinkContext>,
    actor: &Actor,
    granted: Granted,
    chosen: &RemoteStore,
) -> Result<CloudLink, AppError> {
    let existing = ctx.db.read(store::active_link)?;
    let (link_id, relink) = match &existing {
        Some(l) if l.status == LinkStatus::Linked => {
            return Err(AppError::sync("cloud_already_linked"));
        }
        Some(l) if l.store_id == chosen.store_id => (l.id.clone(), true),
        Some(_) => return Err(AppError::sync("cloud_already_linked")),
        None => (new_id(), false),
    };
    // A refused refresh ended the grant, not the device: it is still registered on DZBuild. A
    // revocation ended both, so that relink registers a new device.
    let kept_device = existing
        .filter(|l| relink && l.status == LinkStatus::RelinkRequired)
        .and_then(|l| l.device_id);
    let tokens = Arc::new(TokenManager::new(
        &link_id,
        ctx.secrets.clone(),
        ctx.clock(),
    ));
    tokens.store(&granted.tokens)?;
    let meta = Arc::new(granted.meta.clone());
    let remote = Remote::new(
        ctx.http.clone(),
        tokens.clone(),
        meta,
        ctx.config.clone(),
        Some(chosen.store_id.clone()),
    );
    let outcome = register_remote(ctx, &remote, kept_device).and_then(|state| {
        let profile = state.profile.unwrap_or(api::StoreProfile {
            name: None,
            language: None,
            public_url: None,
            stock_deduction: None,
            currency: None,
            plan: None,
            plan_limits: None,
        });
        let new = NewLink {
            id: link_id.clone(),
            account_sub: granted.sub.clone(),
            account_name: granted.name.clone(),
            store_id: chosen.store_id.clone(),
            store_name: profile
                .name
                .clone()
                .unwrap_or_else(|| chosen.store_name.clone()),
            store_language: match profile.language.as_deref() {
                Some("fr") => Lang::Fr,
                _ => Lang::Ar,
            },
            store_plan: profile.plan.clone().or_else(|| chosen.plan.clone()),
            public_url: profile
                .public_url
                .clone()
                .or_else(|| chosen.public_url.clone()),
            location_id: state.location_id,
            device_id: state.device_id,
            scopes: granted
                .tokens
                .scope
                .as_deref()
                .map(|s| s.split(' ').map(str::to_owned).collect())
                .unwrap_or_else(|| ctx.config.scopes.clone()),
            stock_deduction: profile
                .stock_deduction
                .clone()
                .filter(|d| matches!(d.as_str(), "on_confirm" | "on_create" | "on_delivery")),
            active_products_cap: profile.plan_limits.as_ref().and_then(|l| l.active_products),
        };
        let now = ctx.clock().now_ms();
        let actor = actor.clone();
        ctx.db.write(move |tx| {
            let link = if relink {
                store::relink(tx, &new, now)?
            } else {
                store::insert_link(tx, &new, now)?
            };
            changelog::register(tx, changelog::CLOUD_CONSUMER, now)?;
            for kind in [CloudNoticeKind::Relink, CloudNoticeKind::Revoked] {
                store::clear_notice(tx, kind, "cloud_relink_required", "", now)?;
            }
            audit::append(
                tx,
                &actor.event(
                    "cloud.linked",
                    "cloud_link",
                    Some(&link.id),
                    json!({
                        "storeId": link.store_id,
                        "storeName": link.store_name,
                        "deviceId": link.device_id,
                        "scopes": link.scopes,
                        "relinked": relink,
                    }),
                ),
            )?;
            Ok(link)
        })
    });
    if outcome.is_err()
        && let Err(e) = tokens.forget()
    {
        tracing::warn!(target: "cloud", code = %e.code(), "could not drop the tokens of a failed link");
    }
    outcome
}

/// A `Remote` for the active link (the engine and the owner's commands).
pub fn remote_for(ctx: &LinkContext, link: &CloudLink, meta: Arc<Metadata>) -> Remote {
    let tokens = Arc::new(TokenManager::new(
        &link.id,
        ctx.secrets.clone(),
        ctx.clock(),
    ));
    Remote::new(
        ctx.http.clone(),
        tokens,
        meta,
        ctx.config.clone(),
        Some(link.store_id.clone()),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnlinkOutcome {
    /// DZBuild confirmed the revocation of the refresh token.
    pub revoked: bool,
    /// DZBuild deleted this device.
    pub device_removed: bool,
}

/// Revoke + delete (best effort, 5 s each), then locally: the credential, the link, the
/// change-feed reader; open outbox items are kept as `skipped`. Works offline (the owner is
/// told to remove the device on dzbuild.com).
pub fn unlink(ctx: &LinkContext, actor: &Actor) -> Result<UnlinkOutcome, AppError> {
    let link = ctx
        .db
        .read(store::active_link)?
        .ok_or_else(|| AppError::sync("cloud_not_linked"))?;
    let tokens = Arc::new(TokenManager::new(
        &link.id,
        ctx.secrets.clone(),
        ctx.clock(),
    ));
    let mut outcome = UnlinkOutcome {
        revoked: false,
        device_removed: false,
    };
    if tokens.has_refresh()? {
        match oauth::discover(&ctx.http, &ctx.config) {
            Ok(meta) => {
                let meta = Arc::new(meta);
                if let Some(device_id) = &link.device_id {
                    let remote = Remote::new(
                        ctx.http.clone(),
                        tokens.clone(),
                        meta.clone(),
                        ctx.config.clone(),
                        Some(link.store_id.clone()),
                    );
                    outcome.device_removed = api::delete_device(&remote, device_id).is_ok();
                }
                if let Some(refresh) = tokens.refresh_token()? {
                    outcome.revoked =
                        oauth::revoke(&ctx.http, &meta, &ctx.config, &refresh).is_ok();
                }
            }
            Err(e) => {
                tracing::warn!(target: "cloud", code = %e.code(), "unlinking offline: nothing revoked on DZBuild");
            }
        }
    }
    tokens.forget()?;
    let now = ctx.clock().now_ms();
    let (actor, id, store_id) = (actor.clone(), link.id.clone(), link.store_id.clone());
    ctx.db.write(move |tx| {
        store::set_status(tx, &id, LinkStatus::Unlinked, now)?;
        super::orders::close_unlinked_store(tx, &store_id, now)?;
        changelog::unregister(tx, changelog::CLOUD_CONSUMER)?;
        changelog::prune(tx)?;
        let skipped = tx.execute(
            "UPDATE sync_outbox SET status = 'skipped', last_error = 'unlinked'
             WHERE target = 'cloud' AND status IN ('pending', 'failed')",
            [],
        )?;
        audit::append(
            tx,
            &actor.event(
                "cloud.unlinked",
                "cloud_link",
                Some(&id),
                json!({ "revoked": outcome.revoked, "deviceRemoved": outcome.device_removed, "skipped": skipped }),
            ),
        )?;
        Ok(())
    })?;
    (ctx.on_change)();
    Ok(outcome)
}

/// The link stops: its status, the notice and the journal row first, then the credential, best
/// effort (the engine never sends a refused token again in any case). A credential the secret
/// store cannot delete is said in a notice of its own.
fn suspend(
    ctx: &LinkContext,
    link: &CloudLink,
    status: LinkStatus,
    kind: CloudNoticeKind,
    action: &'static str,
) -> Result<(), AppError> {
    let now = ctx.clock().now_ms();
    let (actor, id, name) = (ctx.actor(), link.id.clone(), link.store_name.clone());
    ctx.db.write(move |tx| {
        store::set_status(tx, &id, status, now)?;
        store::raise_notice(
            tx,
            kind,
            "cloud_relink_required",
            "",
            json!({ "storeName": name }),
            now,
        )?;
        audit::append(
            tx,
            &actor.event(
                action,
                "cloud_link",
                Some(&id),
                json!({ "storeName": name }),
            ),
        )?;
        Ok(())
    })?;
    if let Err(error) = TokenManager::new(&link.id, ctx.secrets.clone(), ctx.clock()).forget() {
        tracing::warn!(target: "cloud", code = %error.code(), "the link's credential could not be deleted");
        ctx.db.write(move |tx| {
            store::raise_notice(
                tx,
                CloudNoticeKind::Relink,
                "cloud_credential_kept",
                "",
                json!({}),
                now,
            )
            .map(drop)
        })?;
    }
    (ctx.on_change)();
    Ok(())
}

/// A refused refresh (`invalid_grant`, reuse detection): the tokens are gone, the sync stops,
/// the outbox and the map wait for the same store to be linked again.
pub fn mark_relink_required(ctx: &LinkContext, link: &CloudLink) -> Result<(), AppError> {
    suspend(
        ctx,
        link,
        LinkStatus::RelinkRequired,
        CloudNoticeKind::Relink,
        "cloud.relink_required",
    )
}

/// `device.revoked` / `link.revoked` / a heartbeat telling this device is gone.
pub fn mark_revoked(ctx: &LinkContext, link: &CloudLink) -> Result<(), AppError> {
    suspend(
        ctx,
        link,
        LinkStatus::Revoked,
        CloudNoticeKind::Revoked,
        "cloud.revoked",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::http::SystemClock;
    use crate::cloud::secrets::{MemorySecrets, refresh_account};
    use crate::cloud::testkit::{MockDz, browse, header, one_store};
    use serde_json::json;
    use std::time::{Duration, Instant};

    fn pages() -> Pages {
        Pages {
            done: "<p>ok</p>".into(),
            failed: "<p>no</p>".into(),
        }
    }

    fn context(dz: &MockDz, db: Db) -> (Arc<LinkContext>, Arc<MemorySecrets>) {
        let secrets = Arc::new(MemorySecrets::default());
        let ctx = Arc::new(LinkContext {
            db,
            http: Arc::new(dz.http()),
            config: Arc::new(dz.config()),
            secrets: secrets.clone(),
            device: DeviceInfo {
                name: "Boutique Amel — C1".into(),
                os: "Windows 11".into(),
                app_version: "0.7.0".into(),
                terminal_code: "C1".into(),
                terminal_id: "00000000-0000-7000-8000-000000000001".into(),
            },
            on_change: Arc::new(|| {}),
        });
        (ctx, secrets)
    }

    pub(crate) fn wait_until(
        attempt: &LinkAttempt,
        done: impl Fn(&AttemptState) -> bool,
    ) -> AttemptState {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let state = attempt.view().state;
            if done(&state) || Instant::now() > deadline {
                return state;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The whole browser flow against the mock: returns the link.
    pub(crate) fn link_with_browser(ctx: &Arc<LinkContext>) -> CloudLink {
        let attempt = start_browser(ctx.clone(), Actor::default(), Lang::Fr, pages()).unwrap();
        let page = browse(attempt.view().authorize_url.unwrap());
        let state = wait_until(&attempt, |s| {
            !matches!(s, AttemptState::Waiting | AttemptState::Finishing)
        });
        assert_eq!(page.join().unwrap(), "<p>ok</p>");
        match state {
            AttemptState::Linked { link } => link,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn linking_in_the_browser_keeps_the_link_the_device_and_the_refresh_token() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, secrets) = context(&dz, Db::open_memory().unwrap());
        let link = link_with_browser(&ctx);
        assert_eq!(
            (
                link.store_id.as_str(),
                link.device_id.as_deref(),
                link.location_id.as_deref()
            ),
            ("141", Some("dev_1"), Some("loc_1"))
        );
        assert_eq!(
            (link.store_language, link.active_products_cap),
            (Lang::Ar, Some(300))
        );
        assert_eq!(
            secrets.get(&refresh_account(&link.id)).unwrap().as_deref(),
            Some("rt_1")
        );
        assert!(
            ctx.db
                .read(|c| changelog::position(c, changelog::CLOUD_CONSUMER))
                .unwrap()
                .is_some()
        );
        // PKCE binding: the verifier sent to the token endpoint hashes to the challenge sent to
        // the browser.
        let requests = dz.requests();
        let authorize = requests
            .iter()
            .find(|r| r.url.path() == "/oauth/authorize")
            .unwrap();
        let challenge: String = authorize
            .url
            .query_pairs()
            .find(|(k, _)| k == "code_challenge")
            .unwrap()
            .1
            .into_owned();
        let token = requests
            .iter()
            .find(|r| r.url.path() == "/oauth/token")
            .unwrap();
        let form: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(&token.body)
                .into_owned()
                .collect();
        assert_eq!(
            crate::cloud::oauth::Pkce::from_verifier(form["code_verifier"].clone()).challenge,
            challenge
        );
        assert!(!form.contains_key("client_secret"));
        let device = requests
            .iter()
            .find(|r| r.url.path() == "/v1/devices")
            .unwrap();
        assert!(
            header(device, "idempotency-key")
                .unwrap()
                .starts_with("device:")
        );
        let audit: String = ctx
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'cloud.linked'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(
            audit.contains("\"storeId\":\"141\"")
                && !audit.contains("rt_1")
                && !audit.contains("at_1")
        );
    }

    #[test]
    fn the_refresh_token_lives_only_in_the_secret_store() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pos.db"), crate::db::DbOptions::default()).unwrap();
        let (ctx, _) = context(&dz, db);
        link_with_browser(&ctx);
        ctx.db.checkpoint().unwrap();
        for name in ["pos.db", "pos.db-wal"] {
            let Ok(bytes) = std::fs::read(dir.path().join(name)) else {
                continue;
            };
            // SQLite names a table's unique index `sqlite_autoindex_<table>_1` (M8's
            // `flexy_float` ends in `at_1`): a schema name is not a token.
            let schema_name = |at: usize| {
                let start = at.saturating_sub(40);
                bytes[start..at]
                    .windows(17)
                    .any(|w| w == b"sqlite_autoindex_")
            };
            for token in [&b"rt_1"[..], b"at_1", b"code_1"] {
                assert!(
                    !bytes
                        .windows(token.len())
                        .enumerate()
                        .any(|(at, w)| w == token && !schema_name(at)),
                    "{name} holds a token"
                );
            }
        }
    }

    #[test]
    fn several_stores_wait_for_the_owners_choice() {
        let dz = MockDz::start();
        dz.mount_link(json!([
            { "store_id": 141, "store_name": "Boutique Amel", "role": "owner" },
            { "store_id": 152, "store_name": "Amel Kids", "role": "owner" }
        ]));
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let attempt = start_browser(ctx.clone(), Actor::default(), Lang::Ar, pages()).unwrap();
        browse(attempt.view().authorize_url.unwrap());
        match wait_until(&attempt, |s| matches!(s, AttemptState::ChooseStore { .. })) {
            AttemptState::ChooseStore { stores } => assert_eq!(stores.len(), 2),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            choose_store(&ctx, &attempt, "999").unwrap_err().code(),
            "cloud_store_not_found"
        );
        let link = choose_store(&ctx, &attempt, "152").unwrap();
        assert_eq!(link.store_name, "Amel Kids");
        assert!(matches!(attempt.view().state, AttemptState::Linked { .. }));
    }

    #[test]
    fn a_denied_consent_fails_and_a_second_link_is_refused() {
        let dz = MockDz::start();
        dz.mount(
            wiremock::Mock::given(wiremock::matchers::path("/oauth/authorize")).respond_with(
                crate::cloud::testkit::AuthorizeRedirect {
                    issuer: dz.uri(),
                    error: Some("access_denied"),
                },
            ),
        );
        dz.mount_link(one_store());
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let attempt = start_browser(ctx.clone(), Actor::default(), Lang::Fr, pages()).unwrap();
        assert_eq!(
            browse(attempt.view().authorize_url.unwrap())
                .join()
                .unwrap(),
            "<p>no</p>"
        );
        let state = wait_until(&attempt, |s| matches!(s, AttemptState::Failed { .. }));
        assert_eq!(
            state,
            AttemptState::Failed {
                code: "cloud_link_denied".into()
            }
        );
        dz.rt.block_on(dz.server.reset());
        dz.mount_link(one_store());
        link_with_browser(&ctx);
        let again = start_browser(ctx.clone(), Actor::default(), Lang::Fr, pages()).unwrap_err();
        assert_eq!(again.code(), "cloud_already_linked");
    }

    #[test]
    fn the_device_grant_links_from_a_phone() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let attempt = start_device(ctx.clone(), Actor::default()).unwrap();
        let view = attempt.view();
        assert_eq!(view.device.as_ref().unwrap().user_code, "DZKM-4Q7P");
        assert!(view.authorize_url.is_none());
        assert!(matches!(
            wait_until(&attempt, |s| matches!(s, AttemptState::Linked { .. })),
            AttemptState::Linked { .. }
        ));
    }

    #[test]
    fn unlink_revokes_deletes_the_device_and_the_credential_and_keeps_local_data() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, secrets) = context(&dz, Db::open_memory().unwrap());
        let link = link_with_browser(&ctx);
        ctx.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
                     VALUES ('00000000-0000-7000-8000-00000000a001', 'cloud', '141', 'pos_sale', 'd1', 'sale:d1', 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        let outcome = unlink(&ctx, &Actor::default()).unwrap();
        assert_eq!(
            outcome,
            UnlinkOutcome {
                revoked: true,
                device_removed: true
            }
        );
        assert_eq!(secrets.get(&refresh_account(&link.id)).unwrap(), None);
        assert!(ctx.db.read(store::active_link).unwrap().is_none());
        assert!(
            ctx.db
                .read(|c| changelog::position(c, changelog::CLOUD_CONSUMER))
                .unwrap()
                .is_none()
        );
        let status: String = ctx
            .db
            .read(|c| Ok(c.query_row("SELECT status FROM sync_outbox", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(
            status, "skipped",
            "kept for the record, never sent after an unlink"
        );
        let revoke = dz
            .requests()
            .into_iter()
            .find(|r| r.url.path() == "/oauth/revoke")
            .unwrap();
        assert!(String::from_utf8_lossy(&revoke.body).contains("token_type_hint=refresh_token"));
    }

    #[test]
    fn unlink_offline_still_deletes_the_credential() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, secrets) = context(&dz, Db::open_memory().unwrap());
        let link = link_with_browser(&ctx);
        // DZBuild is gone: the same till (database, credential) now finds nothing listening.
        // Dropping the mock would not do: wiremock returns its servers to a pool, still
        // answering with their mocks mounted.
        let gone = CloudConfig::loopback("http://127.0.0.1:9");
        let offline = LinkContext {
            db: ctx.db.clone(),
            http: Arc::new(
                Http::new(
                    &gone,
                    Arc::new(SystemClock),
                    "DZBuildPOS/test (Windows)".into(),
                )
                .unwrap(),
            ),
            config: Arc::new(gone),
            secrets: secrets.clone(),
            device: ctx.device.clone(),
            on_change: Arc::new(|| {}),
        };
        let outcome = unlink(&offline, &Actor::default()).unwrap();
        assert_eq!(
            outcome,
            UnlinkOutcome {
                revoked: false,
                device_removed: false
            }
        );
        assert_eq!(secrets.get(&refresh_account(&link.id)).unwrap(), None);
        assert!(ctx.db.read(store::active_link).unwrap().is_none());
        let paths: Vec<String> = dz
            .requests()
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect();
        assert!(
            !paths
                .iter()
                .any(|p| p == "/oauth/revoke" || p.starts_with("/v1/devices/")),
            "{paths:?}"
        );
    }

    #[test]
    fn a_refused_refresh_unlinks_and_keeps_the_outbox() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, secrets) = context(&dz, Db::open_memory().unwrap());
        let link = link_with_browser(&ctx);
        ctx.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
                     VALUES ('00000000-0000-7000-8000-00000000a001', 'cloud', '141', 'pos_sale', 'd1', 'sale:d1', 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        mark_relink_required(&ctx, &link).unwrap();
        let after = ctx.db.read(store::active_link).unwrap().unwrap();
        assert_eq!(after.status, LinkStatus::RelinkRequired);
        assert_eq!(secrets.get(&refresh_account(&link.id)).unwrap(), None);
        let open = ctx.db.read(|c| store::notices(c, false)).unwrap();
        assert_eq!((open.len(), open[0].kind), (1, CloudNoticeKind::Relink));
        let relinked = link_with_browser(&ctx);
        assert_eq!(
            relinked.id, link.id,
            "the same store comes back with its outbox and map"
        );
        let pending: i64 = ctx
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM sync_outbox WHERE status = 'pending'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(pending, 1);
        assert!(
            ctx.db
                .read(|c| store::notices(c, false))
                .unwrap()
                .is_empty(),
            "the relink notice is cleared"
        );
        assert_eq!(
            (relinked.device_id.as_deref(), device_keys(&dz).len()),
            (Some("dev_1"), 1),
            "the device is still registered: it is kept, not posted again"
        );
    }

    /// The `Idempotency-Key` of every `POST /v1/devices` DZBuild received, in order.
    fn device_keys(dz: &MockDz) -> Vec<String> {
        dz.requests()
            .iter()
            .filter(|r| r.method.as_str() == "POST" && r.url.path() == "/v1/devices")
            .map(|r| header(r, "idempotency-key").unwrap())
            .collect()
    }

    /// The same till after an app update: the device registration's body changes.
    fn updated(ctx: &Arc<LinkContext>) -> Arc<LinkContext> {
        Arc::new(LinkContext {
            db: ctx.db.clone(),
            http: ctx.http.clone(),
            config: ctx.config.clone(),
            secrets: ctx.secrets.clone(),
            device: DeviceInfo {
                app_version: "0.7.1".into(),
                ..ctx.device.clone()
            },
            on_change: Arc::new(|| {}),
        })
    }

    #[test]
    fn a_link_after_an_unlink_registers_a_new_device_under_a_new_key() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let install = ctx
            .db
            .read(|c| db::meta_get(c, db::META_INSTALL_ID))
            .unwrap()
            .unwrap();
        let first = link_with_browser(&ctx);
        assert_eq!(
            unlink(&ctx, &Actor::default()).unwrap(),
            UnlinkOutcome {
                revoked: true,
                device_removed: true
            }
        );
        // Within DZBuild's 24 h replay window: the old key would bring back the deleted dev_1.
        let second = link_with_browser(&ctx);
        assert_eq!(
            (first.device_id.as_deref(), second.device_id.as_deref()),
            (Some("dev_1"), Some("dev_2")),
            "never the deleted device again"
        );
        assert_eq!(
            device_keys(&dz),
            [
                format!("device:{install}:C1:141:g0"),
                format!("device:{install}:C1:141:g1")
            ]
        );
        let bodies: Vec<serde_json::Value> = dz
            .requests()
            .iter()
            .filter(|r| r.method.as_str() == "POST" && r.url.path() == "/v1/devices")
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect();
        assert_eq!(
            bodies[1]["external_id"],
            json!(format!("{install}:C1")),
            "DZBuild can still tell it is the same till"
        );
    }

    #[test]
    fn a_relink_after_a_revocation_registers_a_new_device_even_after_an_update() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let link = link_with_browser(&ctx);
        mark_revoked(&ctx, &link).unwrap();
        // The old key with the new body would be `422 idempotency_key_reuse` for a day.
        let relinked = link_with_browser(&updated(&ctx));
        assert_eq!(
            (relinked.id.as_str(), relinked.device_id.as_deref()),
            (link.id.as_str(), Some("dev_2"))
        );
        let keys = device_keys(&dz);
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1]);
    }

    #[test]
    fn a_relink_after_a_refused_refresh_keeps_its_device_across_an_update() {
        let dz = MockDz::start();
        dz.mount_link(one_store());
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let link = link_with_browser(&ctx);
        mark_relink_required(&ctx, &link).unwrap();
        let relinked = link_with_browser(&updated(&ctx));
        assert_eq!(
            (relinked.status, relinked.device_id.as_deref()),
            (LinkStatus::Linked, Some("dev_1"))
        );
        assert_eq!(device_keys(&dz).len(), 1, "no second registration");
    }

    #[test]
    fn a_retried_attempt_gets_back_the_device_it_registered() {
        let dz = MockDz::start();
        // The first attempt registers the device, then DZBuild refuses the location once.
        dz.mount(
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path("/v1/locations"))
                .respond_with(wiremock::ResponseTemplate::new(400).set_body_json(json!({
                    "error": { "code": "validation_failed", "message": "name" }
                })))
                .up_to_n_times(1),
        );
        dz.mount_link(one_store());
        let (ctx, _) = context(&dz, Db::open_memory().unwrap());
        let attempt = start_browser(ctx.clone(), Actor::default(), Lang::Fr, pages()).unwrap();
        browse(attempt.view().authorize_url.unwrap())
            .join()
            .unwrap();
        assert_eq!(
            wait_until(&attempt, |s| matches!(s, AttemptState::Failed { .. })),
            AttemptState::Failed {
                code: "cloud_request_invalid".into()
            }
        );
        assert!(ctx.db.read(store::active_link).unwrap().is_none());
        let link = link_with_browser(&ctx);
        assert_eq!(
            link.device_id.as_deref(),
            Some("dev_1"),
            "DZBuild replayed the failed attempt's device: no twin"
        );
        let keys = device_keys(&dz);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], keys[1], "a retry reuses its key");
    }
}

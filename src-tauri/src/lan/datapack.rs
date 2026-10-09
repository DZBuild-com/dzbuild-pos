//! A till follows its main PC's regulatory data pack (PLT-024, final review): after a finance law
//! the owner installs the new signed pack on the main PC; its tills take it from there (verified
//! with the app's key, installed for their next start) instead of charging the old stamp duty or
//! ceilings while the main PC's host services use the new ones. Whether this till's pack differs
//! from the main PC's is kept for its status (`ClientStatus::datapack_gap`): restart, behind or
//! ahead.

use crate::commands::privacy::datapack_dir;
use crate::state::AppState;
use pos_core::audit::Actor;
use pos_core::datapack_update::{self, DATAPACK_PUBLIC_KEY, HostPack, InstalledPack};
use pos_core::error::AppError;
use serde_json::json;

/// The host service `datapack.active`: the pack this main PC runs with, its signed files when it
/// is an installed one.
pub fn active(st: &AppState) -> HostPack {
    datapack_update::host_pack(&datapack_dir(st), &st.datapack, DATAPACK_PUBLIC_KEY)
}

/// On a till: asks the main PC for its pack and installs a newer signed one for the next start
/// (journaled `datapack.installed`); notes both versions for the status. `Ok(None)` elsewhere,
/// or when nothing was installed.
pub fn follow_host(st: &AppState) -> Result<Option<InstalledPack>, AppError> {
    follow_host_with(st, DATAPACK_PUBLIC_KEY)
}

/// [`follow_host`] with the key that signs the packs (the app's, or the tests' signer).
pub(crate) fn follow_host_with(
    st: &AppState,
    public_key: &str,
) -> Result<Option<InstalledPack>, AppError> {
    if !st.lan.is_client() {
        return Ok(None);
    }
    let Some(offered) =
        crate::lan::route::forward_as_system::<HostPack>(st, "datapack.active", json!({}))?
    else {
        return Ok(None);
    };
    let installed = datapack_update::follow(&datapack_dir(st), &st.datapack, &offered, public_key)?;
    let (host, fetched) = (offered.version, installed.as_ref().map(|p| p.version));
    let journaled = installed.clone();
    let actor = Actor::system(Some(st.terminal.id.clone()));
    st.db.write(move |tx| {
        datapack_update::note_host(tx, host, fetched)?;
        if let Some(pack) = &journaled {
            datapack_update::record_installed(tx, &actor, pack)?;
        }
        Ok(())
    })?;
    if let Some(pack) = &installed {
        tracing::info!(id = %pack.id, version = pack.version, "the main PC's data pack is installed for the next start");
    }
    Ok(installed)
}

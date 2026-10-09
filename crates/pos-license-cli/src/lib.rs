//! `dzpos-license`, the owner's licence tool, never shipped in an installer: `keygen`
//! once, `issue` for each customer, `inspect` a key someone sends, `list` the ledger, `revoke`. The
//! key folder, outside any git work tree, holds the private key sealed by a passphrase (age,
//! scrypt), its public half, the issuance ledger and a copy of every `.dzlic` issued.
//! docs/release/licensing.md is the owner's guide.

pub mod args;
pub mod keyfile;
pub mod ledger;
#[cfg(test)]
mod tests;

use args::{Command, Parsed};
use ed25519_dalek::Signer;
use ledger::{ISSUED_DIR, Issued, Ledger};
use pos_license::device::parse_code;
use pos_license::key;
use pos_license::keys::Keyring;
use pos_license::payload::{
    DAY_MS, LicenseCustomer, LicenseKind, MAX_TRIAL_DAYS, Payload, SIGNING_CONTEXT, TRIAL_DAYS,
    VERSION,
};
use pos_license::verify::verify;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A message for the owner, printed after `dzpos-license: `.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError(pub String);

impl CliError {
    pub fn new(message: impl Into<String>) -> CliError {
        CliError(message.into())
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> CliError {
        CliError(format!("file error: {error}"))
    }
}

impl From<pos_core::error::AppError> for CliError {
    fn from(error: pos_core::error::AppError) -> CliError {
        CliError(format!("refused by the licence check: {}", error.code()))
    }
}

/// What the tool needs from outside: the console in `main.rs`, a fake in the tests.
pub trait Env {
    fn passphrase(&mut self, prompt: &str) -> Result<String, CliError>;
    fn now(&self) -> i64;
    fn out(&mut self) -> &mut dyn Write;
    fn default_keys_dir(&self) -> Option<PathBuf>;
    /// age's scrypt work factor: 18 (about a second on a PC); the tests use less.
    fn work_factor(&self) -> u8 {
        18
    }
    /// The licence keys this release already trusts (`pos_license::keys::PUBLIC_KEYS`): keygen
    /// never makes a second key under one of their ids, and its snippet appends to them.
    fn release_keys(&self) -> Vec<(String, [u8; 32])> {
        pos_license::keys::PUBLIC_KEYS
            .iter()
            .map(|k| (k.id.to_owned(), k.bytes))
            .collect()
    }
}

/// The passphrase written beside the key when it was made (`passphrase.txt`): a
/// reminder to move it into a password manager and delete it. The file is never read.
fn warn_passphrase_file(dir: &std::path::Path, env: &mut dyn Env) -> Result<(), CliError> {
    if dir.join(PASSPHRASE_FILE).exists() {
        writeln!(
            env.out(),
            "WARNING: {PASSPHRASE_FILE} is in the key folder. Put the passphrase in your password manager, then delete {PASSPHRASE_FILE} before copying the folder to the USB sticks (docs/release/licensing.md § k1)."
        )?;
    }
    Ok(())
}

/// The file the key's first run left the passphrase in.
pub const PASSPHRASE_FILE: &str = "passphrase.txt";

pub const HELP: &str =
    "dzpos-license — DZBuild POS licences (keep this tool with the licence key; never publish it)

  dzpos-license keygen [--keys <folder>] [--id k1]
      Makes the licence signing key once, sealed by a passphrase, outside any git folder.
  dzpos-license issue --device DZPOS-XXXX-XXXX-XXXX-XXXX --kind trial|lifetime
                      [--customer \"Name\"] [--phone \"0550 00 00 00\"] [--days 7]
                      [--replace <licence id>] [--out <file.dzlic>] [--keys <folder>]
      Signs a licence for one PC. One trial per PC. --replace moves a lifetime licence to a new PC.
  dzpos-license inspect <key or file.dzlic> [--keys <folder>]
  dzpos-license list [--device <code>] [--keys <folder>]
  dzpos-license revoke <licence id> [--keys <folder>]

The key folder is DZPOS_LICENSE_KEYS, else %USERPROFILE%\\DZBuild POS licence key.
The passphrase is DZPOS_LICENSE_PASSPHRASE, else it is asked without being shown.
Guide: docs/release/licensing.md";

pub fn run(argv: &[String], env: &mut dyn Env) -> Result<(), CliError> {
    let parsed = args::parse(argv)?;
    match parsed.command {
        Command::Help => {
            writeln!(env.out(), "{HELP}")?;
            Ok(())
        }
        Command::Keygen => keygen(&parsed, env),
        Command::Issue => issue(&parsed, env),
        Command::Inspect => inspect(&parsed, env),
        Command::List => list(&parsed, env),
        Command::Revoke => revoke(&parsed, env),
    }
}

fn keys_dir(p: &Parsed, env: &dyn Env) -> Result<PathBuf, CliError> {
    p.flag("keys")
        .map(PathBuf::from)
        .or_else(|| env.default_keys_dir())
        .ok_or_else(|| CliError::new("no key folder: --keys <folder>"))
}

/// The business day (Algiers) of `ms`, `YYYY-MM-DD`.
fn day(ms: i64) -> String {
    pos_core::time::business_date(ms)
        .format("%Y-%m-%d")
        .to_string()
}

fn kind_word(kind: LicenseKind) -> &'static str {
    match kind {
        LicenseKind::Trial => "trial",
        LicenseKind::Lifetime => "lifetime",
    }
}

fn customer_line(customer: Option<&LicenseCustomer>) -> String {
    let parts: Vec<&str> = customer
        .into_iter()
        .flat_map(|c| [c.name.as_deref(), c.phone.as_deref()])
        .flatten()
        .collect();
    if parts.is_empty() {
        "-".to_owned()
    } else {
        parts.join(", ")
    }
}

fn customer_of(p: &Parsed) -> Option<LicenseCustomer> {
    let clean = |name: &str| {
        p.flag(name)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    let (name, phone) = (clean("customer"), clean("phone"));
    (name.is_some() || phone.is_some()).then_some(LicenseCustomer { name, phone })
}

fn keygen(p: &Parsed, env: &mut dyn Env) -> Result<(), CliError> {
    let dir = keys_dir(p, env)?;
    // Final review: the release's keys are never made again (a second, different `k1` would
    // refuse every licence the first one signed); a new key is a rotation, under a new id.
    let existing = env.release_keys();
    let next = format!("k{}", existing.len() + 1);
    let key_id = match p.flag("id") {
        Some(id) => id.to_owned(),
        None if existing.is_empty() => "k1".to_owned(),
        None => {
            return Err(CliError::new(format!(
                "this release already has its licence key ({}): never run keygen again, except for a rotation with a new id (keygen --id {next}, docs/release/licensing.md § Rotation)",
                ids(&existing)
            )));
        }
    };
    if existing.iter().any(|(id, _)| *id == key_id) {
        return Err(CliError::new(format!(
            "{key_id} is already a key of this release (crates/pos-license/src/keys.rs): a second, different {key_id} would refuse every licence the first one signed. A rotation uses a new id (keygen --id {next})"
        )));
    }
    keyfile::check_target(&dir)?;
    let first = env.passphrase("New passphrase for the licence key (12 characters or more): ")?;
    let again = env.passphrase("Type it again: ")?;
    if first != again {
        return Err(CliError::new("the two passphrases differ"));
    }
    let public = keyfile::create(&dir, &key_id, &first, env.work_factor(), env.now())?;
    Ledger::new(&key_id).save(&dir)?;
    let out = env.out();
    writeln!(out, "Licence key {key_id} made in {}", dir.display())?;
    writeln!(out, "Public key: {}", hex::encode(public.to_bytes()))?;
    if existing.is_empty() {
        writeln!(
            out,
            "\nPaste into crates/pos-license/src/keys.rs (replacing PUBLIC_KEYS and KEYRING_SHA256), then release:\n"
        )?;
    } else {
        writeln!(
            out,
            "\nAppend this key to PUBLIC_KEYS in crates/pos-license/src/keys.rs (keep {} above it until every customer has a {key_id} licence) and set KEYRING_SHA256, then release:\n",
            ids(&existing)
        )?;
    }
    write!(
        out,
        "{}",
        keyfile::rust_snippet(&key_id, &public, &existing)
    )?;
    writeln!(
        out,
        "\nKeep this folder on two USB sticks in two places and the passphrase in a password manager: without them no licence can be issued again."
    )?;
    Ok(())
}

/// `k1, k2`: the ids of `keys`.
fn ids(keys: &[(String, [u8; 32])]) -> String {
    keys.iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn sign(signing: &ed25519_dalek::SigningKey, payload: &Payload) -> Result<String, CliError> {
    let bytes = payload.canonical_bytes()?;
    let mut message = SIGNING_CONTEXT.to_vec();
    message.extend_from_slice(&bytes);
    Ok(key::encode(&bytes, &signing.sign(&message).to_bytes()))
}

fn issue(p: &Parsed, env: &mut dyn Env) -> Result<(), CliError> {
    let typed = p
        .flag("device")
        .ok_or_else(|| CliError::new("--device DZPOS-XXXX-XXXX-XXXX-XXXX is required"))?;
    let device = parse_code(typed).ok_or_else(|| {
        CliError::new(format!(
            "`{typed}` is not a device code: check every character with the customer"
        ))
    })?;
    let kind = match p.flag("kind") {
        Some("trial") => LicenseKind::Trial,
        Some("lifetime") => LicenseKind::Lifetime,
        _ => return Err(CliError::new("--kind trial or --kind lifetime is required")),
    };
    let days = match (kind, p.flag("days")) {
        (LicenseKind::Trial, None) => TRIAL_DAYS,
        (LicenseKind::Trial, Some(text)) => text
            .parse::<i64>()
            .ok()
            .filter(|d| (1..=MAX_TRIAL_DAYS).contains(d))
            .ok_or_else(|| CliError::new(format!("--days: 1 to {MAX_TRIAL_DAYS}")))?,
        (LicenseKind::Lifetime, Some(_)) => {
            return Err(CliError::new("--days is for a trial only"));
        }
        (LicenseKind::Lifetime, None) => 0,
    };
    let customer = customer_of(p);
    if p.flag("dev").is_some() {
        return issue_dev(p, env, kind, device, days, customer);
    }
    let replace = p.flag("replace");
    let dir = keys_dir(p, env)?;
    warn_passphrase_file(&dir, env)?;
    let mut ledger = Ledger::load(&dir)?;
    match kind {
        LicenseKind::Trial => {
            if replace.is_some() {
                return Err(CliError::new(
                    "--replace moves a lifetime licence; a trial is never moved",
                ));
            }
            if let Some(earlier) = ledger.trial_of(&device) {
                return Err(CliError::new(format!(
                    "a trial was already issued to {device} on {} ({}): one trial per PC — issue a lifetime licence instead",
                    day(earlier.issued_at),
                    earlier.id
                )));
            }
        }
        LicenseKind::Lifetime => {
            if let Some(old_id) = replace {
                let old = ledger
                    .get(old_id)
                    .ok_or_else(|| CliError::new(format!("no licence {old_id} in the ledger")))?;
                if old.kind != LicenseKind::Lifetime
                    || old.revoked_at.is_some()
                    || old.replaced_by.is_some()
                {
                    return Err(CliError::new(format!(
                        "{old_id} is not a live lifetime licence (dzpos-license list)"
                    )));
                }
                if old.device_code == device {
                    return Err(CliError::new(
                        "--replace is for a new PC: this is the same device code",
                    ));
                }
            }
            if let Some(live) = ledger.live_lifetime_of(&device) {
                return Err(CliError::new(format!(
                    "{device} already has the lifetime licence {} (dzpos-license list --device {device} prints its key)",
                    live.id
                )));
            }
        }
    }
    let passphrase = env.passphrase("Passphrase of the licence key: ")?;
    let (key_id, signing) = keyfile::load(&dir, &passphrase)?;
    let now = env.now();
    let payload = Payload {
        v: VERSION,
        id: pos_core::ids::new_id(),
        kind,
        device_code: device.clone(),
        issued_at: now,
        expires_at: (kind == LicenseKind::Trial).then_some(now + days * DAY_MS),
        customer: customer.clone(),
        features: Vec::new(),
        key_id: key_id.clone(),
    };
    payload.validate().map_err(|_| {
        CliError::new("the licence breaks a v1 rule: a name of 80 characters at most, a phone of digits, spaces, + - .")
    })?;
    let text = sign(&signing, &payload)?;
    // The app's own check, with this key alone.
    verify(
        &text,
        &Keyring::default().with(&key_id, signing.verifying_key()),
        &device,
    )?;
    let dzlic = format!(
        "# DZBuild POS licence ({}) for {device}\n# licence {} issued {} with key {key_id}\n{text}\n",
        kind_word(kind),
        payload.id,
        day(now)
    );
    let file = dir.join(ISSUED_DIR).join(format!("{}.dzlic", payload.id));
    std::fs::create_dir_all(dir.join(ISSUED_DIR))?;
    std::fs::write(&file, dzlic.as_bytes())?;
    if let Some(out) = p.flag("out") {
        std::fs::write(out, dzlic.as_bytes())?;
    }
    ledger.licences.push(Issued {
        id: payload.id.clone(),
        kind,
        device_code: device.clone(),
        customer,
        issued_at: now,
        expires_at: payload.expires_at,
        key: text.clone(),
        replaces: replace.map(str::to_owned),
        replaced_by: None,
        revoked_at: None,
    });
    if let Some(old) = replace.and_then(|old_id| ledger.get_mut(old_id)) {
        old.replaced_by = Some(payload.id.clone());
    }
    ledger.save(&dir)?;
    let out = env.out();
    writeln!(
        out,
        "Licence {} ({}) for {device}",
        payload.id,
        kind_word(kind)
    )?;
    if let Some(end) = payload.expires_at {
        writeln!(out, "Works until {} ({days} days)", day(end))?;
    }
    writeln!(out, "File: {}", file.display())?;
    writeln!(out, "Key (send it as it is, or send the file):\n{text}")?;
    if let Some(old_id) = replace {
        writeln!(
            out,
            "The old licence {old_id} keeps working on the old PC until a release lists it in REVOKED (crates/pos-license/src/keys.rs):\n    \"{old_id}\","
        )?;
    }
    Ok(())
}

/// Debug builds only: a licence signed by the development key (`pos_license::keys::dev`), for a
/// developer's own `pnpm tauri dev`; no passphrase, no ledger. A release build never trusts it.
#[cfg(debug_assertions)]
fn issue_dev(
    p: &Parsed,
    env: &mut dyn Env,
    kind: LicenseKind,
    device: String,
    days: i64,
    customer: Option<LicenseCustomer>,
) -> Result<(), CliError> {
    use pos_license::keys::dev;
    let now = env.now();
    let payload = Payload {
        v: VERSION,
        id: pos_core::ids::new_id(),
        kind,
        device_code: device,
        issued_at: now,
        expires_at: (kind == LicenseKind::Trial).then_some(now + days * DAY_MS),
        customer,
        features: Vec::new(),
        key_id: dev::KEY_ID.to_owned(),
    };
    payload.validate()?;
    let text = sign(&dev::signing_key(), &payload)?;
    if let Some(out) = p.flag("out") {
        std::fs::write(out, format!("# DZBuild POS development licence\n{text}\n"))?;
    }
    writeln!(
        env.out(),
        "Development licence (debug builds only):\n{text}"
    )?;
    Ok(())
}

#[cfg(not(debug_assertions))]
fn issue_dev(
    _: &Parsed,
    _: &mut dyn Env,
    _: LicenseKind,
    _: String,
    _: i64,
    _: Option<LicenseCustomer>,
) -> Result<(), CliError> {
    Err(CliError::new(
        "--dev exists in a debug build of the tool only",
    ))
}

fn inspect(p: &Parsed, env: &mut dyn Env) -> Result<(), CliError> {
    let given = &p.positional[0];
    let text = if given.to_lowercase().ends_with(".dzlic") && Path::new(given).is_file() {
        if std::fs::metadata(given)?.len() > key::MAX_FILE_LEN {
            return Err(CliError::new("this file is too large to be a .dzlic"));
        }
        std::fs::read_to_string(given)?
    } else {
        given.clone()
    };
    let envelope = key::decode(&text)
        .map_err(|_| CliError::new("this is not a DZBuild POS licence key (DZL1.…)"))?;
    let payload: Payload = serde_json::from_slice(&envelope.payload)
        .map_err(|_| CliError::new("this key does not hold a v1 licence"))?;
    // The key folder's own key when the licence names it (the owner's tool, or a test folder whose
    // id shadows a built-in one), else the keys built into this release.
    let ring = match keys_dir(p, env).and_then(|dir| keyfile::read_public(&dir)) {
        Ok((id, public)) if id == payload.key_id => Keyring::default().with(&id, public),
        _ => Keyring::production(),
    };
    let verdict = match verify(&text, &ring, &payload.device_code) {
        Ok(_) => format!("valid (key {})", payload.key_id),
        Err(error) if error.code() == pos_license::codes::REVOKED => {
            "revoked by this release".to_owned()
        }
        Err(_) => "NOT VALID (wrong signature, unknown key or a broken rule)".to_owned(),
    };
    // The check symbol told apart from the other broken rules: a key for a code nobody's
    // PC shows.
    let device_note = if parse_code(&payload.device_code).as_deref() == Some(&payload.device_code) {
        ""
    } else {
        "  (NOT a device code: a wrong symbol or a wrong check symbol)"
    };
    let out = env.out();
    writeln!(out, "licence:   {}", payload.id)?;
    writeln!(out, "kind:      {}", kind_word(payload.kind))?;
    writeln!(out, "device:    {}{device_note}", payload.device_code)?;
    writeln!(out, "issued:    {}", day(payload.issued_at))?;
    writeln!(
        out,
        "expires:   {}",
        payload.expires_at.map_or_else(|| "never".to_owned(), day)
    )?;
    writeln!(
        out,
        "customer:  {}",
        customer_line(payload.customer.as_ref())
    )?;
    writeln!(out, "signature: {verdict}")?;
    Ok(())
}

fn list(p: &Parsed, env: &mut dyn Env) -> Result<(), CliError> {
    let dir = keys_dir(p, env)?;
    let ledger = Ledger::load(&dir)?;
    let device = match p.flag("device") {
        Some(code) => Some(
            parse_code(code)
                .ok_or_else(|| CliError::new(format!("`{code}` is not a device code")))?,
        ),
        None => None,
    };
    let out = env.out();
    let mut shown = 0;
    for l in ledger
        .licences
        .iter()
        .filter(|l| device.as_ref().is_none_or(|d| &l.device_code == d))
    {
        let status = match (&l.revoked_at, &l.replaced_by) {
            (Some(at), _) => format!("revoked {}", day(*at)),
            (None, Some(by)) => format!("replaced by {by}"),
            (None, None) => "live".to_owned(),
        };
        writeln!(
            out,
            "{}  {:<8}  {}  issued {}  expires {}  {}  {status}",
            l.id,
            kind_word(l.kind),
            l.device_code,
            day(l.issued_at),
            l.expires_at.map_or_else(|| "never".to_owned(), day),
            customer_line(l.customer.as_ref()),
        )?;
        if device.is_some() {
            writeln!(out, "    {}", l.key)?;
        }
        shown += 1;
    }
    if shown == 0 {
        writeln!(out, "no licence")?;
    }
    Ok(())
}

fn revoke(p: &Parsed, env: &mut dyn Env) -> Result<(), CliError> {
    let dir = keys_dir(p, env)?;
    let mut ledger = Ledger::load(&dir)?;
    let id = p.positional[0].clone();
    let now = env.now();
    let licence = ledger
        .get_mut(&id)
        .ok_or_else(|| CliError::new(format!("no licence {id} in the ledger")))?;
    if licence.revoked_at.is_some() {
        return Err(CliError::new(format!("{id} is already revoked")));
    }
    licence.revoked_at = Some(now);
    ledger.save(&dir)?;
    writeln!(
        env.out(),
        "Revoked in the ledger. Add this line to REVOKED in crates/pos-license/src/keys.rs and ship a release (a PC refuses the key once it runs that release):\n    \"{id}\","
    )?;
    Ok(())
}

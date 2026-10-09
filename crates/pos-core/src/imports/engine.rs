//! The import engine (spec §12 import rules, IMX-003…008, IMX-017). A dry run gives the exact
//! counts and every problem and keeps nothing; the apply runs the same row code in 500-row
//! transactions with one savepoint per row, journals each transaction (`import.applied` with the
//! job id), and records each change so the latest import of the day can be undone. Imports never
//! overwrite silently: create-only skips existing records, upsert keeps their previous state for
//! undo, and nothing is applied without the fingerprint of a dry run of the same options and rows.

use super::rows::{self, Env, Outcome};
use super::spec::{FieldSpec, ImportEntity, ImportPolicy, PROBLEM_CODES, Problem, fields};
use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::catalog::products;
use crate::db::Db;
use crate::error::AppError;
use crate::ids::new_id;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use crate::time::business_date;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const BATCH_ROWS: usize = 500;
pub const MAX_PROBLEMS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub entity: ImportEntity,
    pub policy: ImportPolicy,
    pub file_name: String,
    /// 0-based index of the header row in the sheet.
    pub header_row: u32,
    /// The field each column feeds; `None` leaves the column out.
    pub mapping: Vec<Option<String>>,
    /// Opening stock without a location column goes here (default location when absent).
    #[serde(default)]
    #[specta(optional)]
    pub location_id: Option<String>,
    /// The open count that receives the counted quantities.
    #[serde(default)]
    #[specta(optional)]
    pub count_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    /// The spreadsheet's row number (1-based).
    pub number: u32,
    /// Field key → non-empty cell text.
    pub cells: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DryRun {
    pub total: u32,
    pub create: u32,
    pub update: u32,
    pub skip: u32,
    pub fail: u32,
    /// Products a `sync` would archive (absent from the file).
    pub archive: u32,
    /// Products absent from the file that a `sync` keeps because they hold stock (a product is
    /// never archived with stock on hand): their total, and the first of them by name.
    pub stock_kept: u32,
    pub stock_kept_names: Vec<String>,
    pub problems: Vec<Problem>,
    pub problems_truncated: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Done,
    Failed,
    Undone,
}

impl JobStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Failed => "failed",
            JobStatus::Undone => "undone",
        }
    }

    pub fn parse(s: &str) -> Option<JobStatus> {
        [
            JobStatus::Running,
            JobStatus::Done,
            JobStatus::Failed,
            JobStatus::Undone,
        ]
        .into_iter()
        .find(|j| j.as_str() == s)
    }
}

sql_text_enum!(JobStatus);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportJob {
    pub id: String,
    pub kind: ImportEntity,
    pub file_name: String,
    pub policy: ImportPolicy,
    pub status: JobStatus,
    pub total: u32,
    pub created: u32,
    pub updated: u32,
    pub skipped: u32,
    pub failed: u32,
    pub problems: Vec<Problem>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub user_id: Option<String>,
    /// The latest job not undone, created today, finished or failed.
    pub undoable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub job_id: String,
    pub done: u32,
    pub total: u32,
    pub created: u32,
    pub updated: u32,
    pub skipped: u32,
    pub failed: u32,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Refuses a mapping that names an unknown field or a field twice, leaves a required field out,
/// posts without an identifying field, syncs something else than products by their codes, or
/// imports count lines without a count.
pub fn check_mapping(fields: &[FieldSpec], opts: &ImportOptions) -> Result<(), AppError> {
    let bad = || AppError::validation("mapping", "import_mapping_invalid");
    let mut mapped = BTreeSet::new();
    for key in opts.mapping.iter().flatten() {
        if !fields.iter().any(|f| &f.key == key) || !mapped.insert(key.as_str()) {
            return Err(bad());
        }
    }
    if fields
        .iter()
        .any(|f| f.required && !mapped.contains(f.key.as_str()))
    {
        return Err(bad());
    }
    let identified = fields
        .iter()
        .any(|f| f.identifies && mapped.contains(f.key.as_str()));
    if opts.entity.posts() && !identified {
        return Err(bad());
    }
    if opts.policy == ImportPolicy::Sync && (opts.entity != ImportEntity::Products || !identified) {
        return Err(bad());
    }
    if opts.entity == ImportEntity::CountLines && opts.count_id.is_none() {
        return Err(bad());
    }
    Ok(())
}

/// The permission a field needs when an import may update existing records, the same the typed
/// commands ask: prices (`products.price`), the CMUP (`reports.profit`), a customer's credit limit
/// (`credit.view`). A create-only import sets them on new records only, like `products_create`.
fn field_permission(entity: ImportEntity, key: &str) -> Option<Permission> {
    match (entity, key) {
        (ImportEntity::Products, "sellPrice") => Some(Permission::ProductsPrice),
        (ImportEntity::Products, k) if k.starts_with("price.") => Some(Permission::ProductsPrice),
        (ImportEntity::Products, "costPrice") => Some(Permission::ReportsProfit),
        (ImportEntity::Customers, "creditLimit") => Some(Permission::CreditView),
        _ => None,
    }
}

/// Refuses (`insufficient_permission`) an upsert or sync mapping that names a field the user may
/// not change on an existing record: a storekeeper cannot reprice the catalogue or overwrite the
/// CMUP through an import, a customer desk without `credit.view` cannot set limits.
pub fn check_field_permissions(ctx: &TillContext, opts: &ImportOptions) -> Result<(), AppError> {
    if opts.policy == ImportPolicy::CreateOnly {
        return Ok(());
    }
    for key in opts.mapping.iter().flatten() {
        if let Some(permission) = field_permission(opts.entity, key)
            && !ctx.allows(permission)
        {
            return Err(AppError::Permission {
                permission: permission.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

/// The mapped cells of every non-empty row after the header, numbered like the spreadsheet.
pub fn rows_of(sheet_rows: &[Vec<String>], opts: &ImportOptions) -> Vec<ImportRow> {
    let start = usize::try_from(opts.header_row)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    sheet_rows
        .iter()
        .enumerate()
        .skip(start)
        .filter_map(|(i, row)| {
            let mut cells = BTreeMap::new();
            for (col, key) in opts.mapping.iter().enumerate() {
                if let (Some(key), Some(value)) = (key, row.get(col)) {
                    let value = value.trim();
                    if !value.is_empty() {
                        cells.insert(key.clone(), value.to_owned());
                    }
                }
            }
            (!cells.is_empty()).then(|| ImportRow {
                number: count(i + 1),
                cells,
            })
        })
        .collect()
}

/// SHA-256 of the options and the rows: `apply` runs only what a dry run checked.
pub fn fingerprint(opts: &ImportOptions, rows: &[ImportRow]) -> String {
    let bytes = serde_json::to_vec(&json!({ "options": opts, "rows": rows })).unwrap_or_default();
    hex::encode(Sha256::digest(&bytes))
}

/// Rows repeating an identifying value of an earlier row (compared without case and with
/// Western digits) within the same `scope` value (opening stock: the same product may load one
/// row per location): reported, never applied.
fn duplicates(
    fields: &[FieldSpec],
    scope: Option<&str>,
    rows: &[ImportRow],
) -> BTreeMap<u32, Problem> {
    let identifying: Vec<&FieldSpec> = fields.iter().filter(|f| f.identifies).collect();
    let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
    let norm = |v: &str| crate::party::western_digits(v).trim().to_lowercase();
    let mut out = BTreeMap::new();
    for row in rows {
        let within = scope
            .and_then(|s| row.cells.get(s))
            .map(|v| norm(v))
            .unwrap_or_default();
        for field in &identifying {
            let Some(value) = row.cells.get(&field.key) else {
                continue;
            };
            let key = (field.key.clone(), norm(value), within.clone());
            if !seen.insert(key) {
                out.insert(
                    row.number,
                    Problem {
                        row: row.number,
                        field: Some(field.key.clone()),
                        value: Some(value.clone()),
                        code: "duplicate_in_file".into(),
                    },
                );
                break;
            }
        }
    }
    out
}

/// A refusal of one row, in the closed list of problem codes.
pub fn problem_of(row: &ImportRow, field: Option<&str>, err: &AppError) -> Problem {
    let payload = err.payload();
    let code = if PROBLEM_CODES.contains(&payload.code.as_str()) {
        payload.code.clone()
    } else {
        match err {
            AppError::Validation { .. } => "value_invalid".to_owned(),
            AppError::NotFound { .. } => "not_found".to_owned(),
            _ => "rejected".to_owned(),
        }
    };
    let field = field
        .map(str::to_owned)
        .or_else(|| payload.field.clone().filter(|f| row.cells.contains_key(f)));
    let value = field.as_ref().and_then(|f| row.cells.get(f).cloned());
    Problem {
        row: row.number,
        field,
        value,
        code,
    }
}

#[derive(Default)]
struct Tally {
    create: u32,
    update: u32,
    skip: u32,
    fail: u32,
    problems: Vec<Problem>,
    truncated: bool,
    /// Products the file names or creates (a sync archives the others).
    touched: BTreeSet<String>,
    /// `(row, entity id, action, what undo needs)` for `import_job_rows`.
    records: Vec<(u32, String, &'static str, Option<Value>)>,
}

impl Tally {
    fn problem(&mut self, p: Problem) {
        if self.problems.len() < MAX_PROBLEMS {
            self.problems.push(p);
        } else {
            self.truncated = true;
        }
    }

    fn absorb(&mut self, other: Tally) {
        self.create += other.create;
        self.update += other.update;
        self.skip += other.skip;
        self.fail += other.fail;
        self.truncated |= other.truncated;
        for p in other.problems {
            self.problem(p);
        }
        self.touched.extend(other.touched);
    }
}

/// Each row in its own savepoint: a refused row leaves nothing and the batch goes on; an
/// internal error (disk full) stops the batch, whose transaction then rolls back.
fn run_rows(
    tx: &Transaction<'_>,
    env: &Env<'_>,
    rows: &[ImportRow],
    dupes: &BTreeMap<u32, Problem>,
    tally: &mut Tally,
) -> Result<(), AppError> {
    for row in rows {
        if env.opts.policy == ImportPolicy::Sync {
            // In the file whatever becomes of its row: a refused row never archives its product.
            tally.touched.extend(rows::named_product(tx, row)?);
        }
        if let Some(p) = dupes.get(&row.number) {
            tally.fail += 1;
            tally.problem(p.clone());
            continue;
        }
        tx.execute_batch("SAVEPOINT import_row")?;
        match rows::apply(tx, env, row) {
            Ok(Ok(outcome)) => {
                tx.execute_batch("RELEASE import_row")?;
                match outcome {
                    Outcome::Created(id) => {
                        tally.create += 1;
                        tally.touched.insert(id.clone());
                        tally.records.push((row.number, id, "created", None));
                    }
                    Outcome::Updated(id, before) => {
                        tally.update += 1;
                        tally.touched.insert(id.clone());
                        tally
                            .records
                            .push((row.number, id, "updated", Some(before)));
                    }
                    Outcome::Posted(id, undo) => {
                        tally.create += 1;
                        tally.records.push((row.number, id, "posted", Some(undo)));
                    }
                    Outcome::Skipped(code, id) => {
                        tally.skip += 1;
                        tally.touched.extend(id);
                        tally.problem(Problem {
                            row: row.number,
                            field: None,
                            value: None,
                            code: code.into(),
                        });
                    }
                }
            }
            Ok(Err(problem)) => {
                tx.execute_batch("ROLLBACK TO import_row; RELEASE import_row")?;
                tally.fail += 1;
                tally.problem(problem);
            }
            Err(fatal) => {
                tx.execute_batch("ROLLBACK TO import_row; RELEASE import_row")?;
                return Err(fatal);
            }
        }
    }
    Ok(())
}

fn spec_of(db: &Db, entity: ImportEntity) -> Result<Vec<FieldSpec>, AppError> {
    db.read(move |c| fields(c, entity))
}

/// Active products a sync would archive: those the file does not name. Variant models are never
/// in a file (the export leaves them out, an import cannot create them), so a sync leaves them.
/// A product holding stock is never archived (`products::archive` refuses it): the sync keeps it
/// and the dry run lists it (`stocked`).
fn untouched(conn: &Connection, touched: &BTreeSet<String>) -> Result<Vec<String>, AppError> {
    Ok(absent(conn, touched)?
        .into_iter()
        .filter(|(_, _, stocked)| !stocked)
        .map(|(id, _, _)| id)
        .collect())
}

/// The active products the file does not name: `(id, label, holds stock)`.
fn absent(
    conn: &Connection,
    touched: &BTreeSet<String>,
) -> Result<Vec<(String, String, bool)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT p.id,
                COALESCE(p.reference || ' ', '') || COALESCE(NULLIF(p.name_fr, ''), NULLIF(p.name_ar, ''), p.name_en),
                EXISTS (SELECT 1 FROM stock_levels l WHERE l.product_id = p.id AND l.qty <> 0)
         FROM products p WHERE p.archived_at IS NULL AND p.kind <> 'variant_parent'
         ORDER BY p.rowid",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<(String, String, bool)>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(id, _, _)| !touched.contains(id))
        .collect())
}

/// Products listed by name in a dry run.
const STOCK_KEPT_NAMES: usize = 50;

/// Archives what a sync leaves out, each recorded after the file's last row for undo.
fn archive_untouched(
    tx: &Transaction<'_>,
    actor: &Actor,
    job_id: &str,
    touched: &BTreeSet<String>,
    first_free_row: u32,
) -> Result<u32, AppError> {
    let mut archived = 0_u32;
    for product_id in untouched(tx, touched)? {
        products::archive(tx, actor, &product_id)?;
        tx.execute(
            "INSERT INTO import_job_rows (job_id, row_no, entity_id, action, before)
             VALUES (?1, ?2, ?3, 'updated', ?4)",
            params![
                job_id,
                first_free_row.saturating_add(archived),
                product_id,
                json!({ "archivedBySync": true }).to_string()
            ],
        )?;
        archived += 1;
    }
    Ok(archived)
}

pub fn dry_run(
    db: &Db,
    ctx: &TillContext,
    opts: &ImportOptions,
    rows: &[ImportRow],
    choices: &BTreeMap<String, String>,
) -> Result<DryRun, AppError> {
    let spec = spec_of(db, opts.entity)?;
    check_mapping(&spec, opts)?;
    check_field_permissions(ctx, opts)?;
    let dupes = duplicates(&spec, opts.entity.duplicate_scope(), rows);
    let mut all = Tally::default();
    for chunk in rows.chunks(BATCH_ROWS) {
        let (ctx, opts, choices, spec, chunk, dupes) = (
            ctx.clone(),
            opts.clone(),
            choices.clone(),
            spec.clone(),
            chunk.to_vec(),
            dupes.clone(),
        );
        let tally = db.dry_run(move |tx| {
            let env = Env {
                ctx: &ctx,
                opts: &opts,
                choices: &choices,
                fields: &spec,
            };
            let mut tally = Tally::default();
            run_rows(tx, &env, &chunk, &dupes, &mut tally)?;
            Ok(tally)
        })?;
        all.absorb(tally);
    }
    // A sync that would create or update nothing is refused by `apply`: it archives nothing.
    let (archive, stock_kept, stock_kept_names) =
        if opts.policy == ImportPolicy::Sync && all.create + all.update > 0 {
            let touched = all.touched.clone();
            let left = db.read(move |c| absent(c, &touched))?;
            let (kept, gone): (Vec<_>, Vec<_>) = left.into_iter().partition(|(_, _, s)| *s);
            let names = kept
                .iter()
                .take(STOCK_KEPT_NAMES)
                .map(|(_, label, _)| label.clone())
                .collect();
            (count(gone.len()), count(kept.len()), names)
        } else {
            (0, 0, Vec::new())
        };
    Ok(DryRun {
        total: count(rows.len()),
        create: all.create,
        update: all.update,
        skip: all.skip,
        fail: all.fail,
        archive,
        stock_kept,
        stock_kept_names,
        problems: all.problems,
        problems_truncated: all.truncated,
        fingerprint: fingerprint(opts, rows),
    })
}

/// Applies `rows` in transactions of `BATCH_ROWS`, each journaled with the job id; the last one
/// also archives what a sync leaves out and finishes the job. A batch that fails on an internal
/// error rolls back alone: the job becomes `failed`, the batches before it stay (undoable).
/// A sync that creates or updates nothing (an empty file, every row refused) would archive the
/// whole catalogue: it is refused (`import_mapping_invalid`), and the job, if any, fails.
pub fn apply(
    db: &Db,
    ctx: &TillContext,
    opts: &ImportOptions,
    rows: &[ImportRow],
    choices: &BTreeMap<String, String>,
    checked: &str,
    progress: &mut dyn FnMut(&ImportProgress),
) -> Result<ImportJob, AppError> {
    if fingerprint(opts, rows) != checked {
        return Err(AppError::conflict("import_mapping_invalid"));
    }
    let spec = spec_of(db, opts.entity)?;
    check_mapping(&spec, opts)?;
    check_field_permissions(ctx, opts)?;
    let refusal = || AppError::validation("mapping", "import_mapping_invalid");
    if opts.policy == ImportPolicy::Sync && rows.is_empty() {
        return Err(refusal());
    }
    let dupes = duplicates(&spec, opts.entity.duplicate_scope(), rows);
    let job_id = new_id();
    let total = count(rows.len());
    {
        let (id, o, c) = (job_id.clone(), opts.clone(), ctx.clone());
        db.write(move |tx| {
            tx.execute(
                "INSERT INTO import_jobs (id, kind, file_name, policy, status, mapping, total,
                                          created_at, user_id, terminal_id)
                 VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    o.entity,
                    o.file_name,
                    o.policy,
                    serde_json::to_string(&o)?,
                    total,
                    c.now,
                    c.session.user_id,
                    c.session.terminal_id
                ],
            )?;
            Ok(())
        })?;
    }
    // A file without rows still runs one empty batch: it finishes (and journals) the job.
    let chunks: Vec<&[ImportRow]> = if rows.is_empty() {
        vec![&[]]
    } else {
        rows.chunks(BATCH_ROWS).collect()
    };
    let batches = chunks.len();
    let first_free_row = rows.last().map_or(1, |r| r.number.saturating_add(1));
    let mut all = Tally::default();
    let mut done = 0_u32;
    let mut finished = None;
    let mut sync_refused = false;
    for (index, chunk) in chunks.into_iter().enumerate() {
        let last = index + 1 == batches;
        let (c, o, ch, sp, rows_now, d, id) = (
            ctx.clone(),
            opts.clone(),
            choices.clone(),
            spec.clone(),
            chunk.to_vec(),
            dupes.clone(),
            job_id.clone(),
        );
        let so_far: Vec<Problem> = all.problems.clone();
        let sync = last && opts.policy == ImportPolicy::Sync;
        let touched_before: BTreeSet<String> = if sync {
            all.touched.clone()
        } else {
            BTreeSet::new()
        };
        let changed_before = all.create + all.update;
        let batch = db.write(move |tx| {
            let env = Env {
                ctx: &c,
                opts: &o,
                choices: &ch,
                fields: &sp,
            };
            let actor = c.actor();
            let mut tally = Tally::default();
            run_rows(tx, &env, &rows_now, &d, &mut tally)?;
            for (row, entity_id, action, undo) in &tally.records {
                tx.execute(
                    "INSERT INTO import_job_rows (job_id, row_no, entity_id, action, before)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        id,
                        row,
                        entity_id,
                        action,
                        undo.as_ref().map(Value::to_string)
                    ],
                )?;
            }
            // Every row refused (a column mapped to the wrong field): nothing archived, job failed.
            let refused = sync && changed_before + tally.create + tally.update == 0;
            let archived = if sync && !refused {
                let mut touched = touched_before;
                touched.extend(tally.touched.iter().cloned());
                archive_untouched(tx, &actor, &id, &touched, first_free_row)?
            } else {
                0
            };
            let mut problems = so_far;
            problems.extend(tally.problems.iter().cloned());
            problems.truncate(MAX_PROBLEMS);
            let (first_row, last_row) = (
                rows_now.first().map_or(0, |r| r.number),
                rows_now.last().map_or(0, |r| r.number),
            );
            tx.execute(
                "UPDATE import_jobs SET created = created + ?2, updated = updated + ?3,
                        skipped = skipped + ?4, failed = failed + ?5, next_row = ?6, errors = ?7
                 WHERE id = ?1",
                params![
                    id,
                    tally.create,
                    tally.update + archived,
                    tally.skip,
                    tally.fail,
                    last_row,
                    serde_json::to_string(&problems)?
                ],
            )?;
            if last {
                let status = if refused {
                    JobStatus::Failed
                } else {
                    JobStatus::Done
                };
                tx.execute(
                    "UPDATE import_jobs SET status = ?3, finished_at = ?2 WHERE id = ?1",
                    params![id, c.now, status.as_str()],
                )?;
            }
            audit::append(
                tx,
                &actor.event(
                    "import.applied",
                    "import_job",
                    Some(&id),
                    json!({ "jobId": id, "kind": o.entity, "fileName": o.file_name,
                            "policy": o.policy, "firstRow": first_row, "lastRow": last_row,
                            "created": tally.create, "updated": tally.update,
                            "skipped": tally.skip, "failed": tally.fail, "archived": archived,
                            "finished": last }),
                ),
            )?;
            let job = if last {
                Some(job(tx, &id, &c.business_day())?)
            } else {
                None
            };
            Ok((tally, job, refused))
        });
        match batch {
            Ok((tally, job, refused)) => {
                all.absorb(tally);
                finished = job;
                sync_refused = refused;
            }
            Err(e) => {
                let (id, now) = (job_id.clone(), ctx.now);
                db.write(move |tx| {
                    tx.execute(
                        "UPDATE import_jobs SET status = 'failed', finished_at = ?2 WHERE id = ?1",
                        params![id, now],
                    )?;
                    Ok(())
                })?;
                return Err(e);
            }
        }
        done += count(chunk.len());
        progress(&ImportProgress {
            job_id: job_id.clone(),
            done,
            total,
            created: all.create,
            updated: all.update,
            skipped: all.skip,
            failed: all.fail,
        });
    }
    if sync_refused {
        return Err(refusal());
    }
    finished.ok_or_else(|| AppError::Internal("import finished without its last batch".into()))
}

const JOB_COLUMNS: &str = "id, kind, file_name, policy, status, total, created, updated, skipped, \
                           failed, errors, created_at, finished_at, user_id";

fn latest_open(conn: &Connection) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT id FROM import_jobs WHERE status <> 'undone'
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

fn map_job(r: &rusqlite::Row<'_>) -> rusqlite::Result<ImportJob> {
    let errors: String = r.get(10)?;
    Ok(ImportJob {
        id: r.get(0)?,
        kind: r.get(1)?,
        file_name: r.get(2)?,
        policy: r.get(3)?,
        status: r.get(4)?,
        total: r.get(5)?,
        created: r.get(6)?,
        updated: r.get(7)?,
        skipped: r.get(8)?,
        failed: r.get(9)?,
        problems: serde_json::from_str(&errors).unwrap_or_default(),
        created_at: r.get(11)?,
        finished_at: r.get(12)?,
        user_id: r.get(13)?,
        undoable: false,
    })
}

fn with_undoable(mut job: ImportJob, latest: Option<&str>, today: &str) -> ImportJob {
    job.undoable = latest == Some(job.id.as_str())
        && matches!(job.status, JobStatus::Done | JobStatus::Failed)
        && business_date(job.created_at).format("%Y-%m-%d").to_string() == today;
    job
}

pub fn job(conn: &Connection, id: &str, today: &str) -> Result<ImportJob, AppError> {
    let found = conn
        .query_row(
            &format!("SELECT {JOB_COLUMNS} FROM import_jobs WHERE id = ?1"),
            [id],
            map_job,
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("import"))?;
    Ok(with_undoable(found, latest_open(conn)?.as_deref(), today))
}

pub fn jobs(conn: &Connection, today: &str, limit: u32) -> Result<Vec<ImportJob>, AppError> {
    let latest = latest_open(conn)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {JOB_COLUMNS} FROM import_jobs ORDER BY created_at DESC, rowid DESC LIMIT ?1"
    ))?;
    let found = stmt
        .query_map([i64::from(limit.clamp(1, 200))], map_job)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(found
        .into_iter()
        .map(|j| with_undoable(j, latest.as_deref(), today))
        .collect())
}

/// Jobs a crash left `running` become `failed` (their applied batches stay, undoable).
pub fn abandon_running(tx: &Transaction<'_>, now: i64) -> Result<usize, AppError> {
    Ok(tx.execute(
        "UPDATE import_jobs SET status = 'failed', finished_at = ?1 WHERE status = 'running'",
        [now],
    )?)
}

/// Undoes the latest import not undone, created on the operator's business day
/// (`import_undo_not_allowed` otherwise): created records archived, updated ones restored from
/// their saved input, postings reversed by counter-entries, newest row first.
pub fn undo(tx: &Transaction<'_>, ctx: &TillContext, job_id: &str) -> Result<ImportJob, AppError> {
    let today = ctx.business_day();
    let found = job(tx, job_id, &today)?;
    if !found.undoable {
        return Err(AppError::conflict("import_undo_not_allowed"));
    }
    let mut stmt = tx.prepare(
        "SELECT entity_id, action, before FROM import_job_rows WHERE job_id = ?1
         ORDER BY row_no DESC",
    )?;
    let records = stmt
        .query_map([job_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<(String, String, Option<String>)>, _>>()?;
    for (entity_id, action, before) in &records {
        rows::undo_row(
            tx,
            ctx,
            job_id,
            found.kind,
            entity_id,
            action,
            before.as_deref(),
        )?;
    }
    tx.execute(
        "UPDATE import_jobs SET status = 'undone' WHERE id = ?1",
        [job_id],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "import.undone",
            "import_job",
            Some(job_id),
            json!({ "jobId": job_id, "kind": found.kind, "rows": records.len() }),
        ),
    )?;
    job(tx, job_id, &today)
}

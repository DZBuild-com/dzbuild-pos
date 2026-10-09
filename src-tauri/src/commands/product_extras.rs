//! Product extras (M2): brand and references, floor price, quantity breaks (PRC-004), internal
//! codes, the variants matrix (PRD-020), bulk prices and edits (PRD-062/070), the margin helper
//! (PRD-060/061) and photos (PRD-004, IMX-018). Photos are picked by a Rust file dialog: the
//! webview never supplies a path. Every product answered goes through `redact::product`.

use super::{blocking, redact};
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission, Session};
use pos_core::catalog::breaks::{self, PriceBreak, PriceBreakInput};
use pos_core::catalog::bulk::{self, BulkEdit, BulkPriceInput, PriceChangeRow};
use pos_core::catalog::extras::{self, ProductExtras, ProductExtrasInput};
use pos_core::catalog::pricing::{self, PriceHelp, PriceHelpInput};
use pos_core::catalog::products::{self, Product};
use pos_core::catalog::variants::{self, VariantsInput};
use pos_core::catalog::{codes, images};
use pos_core::error::AppError;
use pos_core::time::now_ms;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State, Window};
use tauri_plugin_dialog::DialogExt;

const READ: &[Permission] = &[Permission::ProductsView];
/// The margin helper takes a cost: for those who price or see the margins.
const PRICE_HELP: &[Permission] = &[Permission::ProductsPrice, Permission::ReportsProfit];
/// What a photo may be; the filter's name is its extensions, the same in every language.
const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];
const IMAGE_FILTER: &str = "jpg, jpeg, png, webp";

/// IMX-018: a folder of photos named by barcode or reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImagesImported {
    /// Photos now shown on their product.
    pub matched: u32,
    /// File names whose name is no product's barcode or reference.
    pub unmatched: Vec<String>,
    /// File names of a product whose picture was refused (not an image, or too large).
    pub refused: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn products_extras_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<ProductExtras, AppError> {
    let st = state.inner().clone();
    blocking(move || get_extras(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_extras_set(
    state: State<'_, SharedState>,
    id: String,
    input: ProductExtrasInput,
) -> Result<ProductExtras, AppError> {
    let st = state.inner().clone();
    blocking(move || set_extras(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_breaks_list(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Vec<PriceBreak>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_breaks(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_breaks_replace(
    state: State<'_, SharedState>,
    id: String,
    breaks: Vec<PriceBreakInput>,
) -> Result<Vec<PriceBreak>, AppError> {
    let st = state.inner().clone();
    blocking(move || replace_breaks(&st, id, breaks)).await
}

/// Takes the next in-store EAN-13 (the counter moves even if the product is not saved).
#[tauri::command]
#[specta::specta]
pub async fn products_next_ean13(state: State<'_, SharedState>) -> Result<String, AppError> {
    let st = state.inner().clone();
    blocking(move || next_ean13(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_next_reference(state: State<'_, SharedState>) -> Result<String, AppError> {
    let st = state.inner().clone();
    blocking(move || next_reference(&st)).await
}

/// The combinations that do not exist yet; the existing ones are skipped.
#[tauri::command]
#[specta::specta]
pub async fn products_variants_generate(
    state: State<'_, SharedState>,
    parent_id: String,
    input: VariantsInput,
) -> Result<Vec<Product>, AppError> {
    let st = state.inner().clone();
    blocking(move || generate_variants(&st, parent_id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_bulk_price_preview(
    state: State<'_, SharedState>,
    input: BulkPriceInput,
) -> Result<Vec<PriceChangeRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || bulk_price_preview(&st, input)).await
}

/// The number of prices that moved.
#[tauri::command]
#[specta::specta]
pub async fn products_bulk_price_apply(
    state: State<'_, SharedState>,
    input: BulkPriceInput,
) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || bulk_price_apply(&st, input)).await
}

/// The number of products edited.
#[tauri::command]
#[specta::specta]
pub async fn products_bulk_edit(
    state: State<'_, SharedState>,
    edits: Vec<BulkEdit>,
) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || bulk_edit(&st, edits)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_price_help(
    state: State<'_, SharedState>,
    input: PriceHelpInput,
) -> Result<PriceHelp, AppError> {
    let st = state.inner().clone();
    blocking(move || price_help(&st, input)).await
}

/// Opens a file dialog; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn products_pick_image(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    id: String,
) -> Result<Option<Product>, AppError> {
    let st = state.inner().clone();
    may_edit_images(&st)?;
    blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter(IMAGE_FILTER, &IMAGE_EXTENSIONS)
            .set_parent(&window)
            .blocking_pick_file();
        let Some(file) = picked else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|_| AppError::validation("image", "image_invalid"))?;
        let bytes = read_image_file(&path)?;
        set_image_bytes(&st, Path::new(&st.paths.media), id, &bytes).map(Some)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn products_clear_image(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || clear_image(&st, Path::new(&st.paths.media), id)).await
}

/// Opens a folder dialog; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn products_import_images(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<ImagesImported>, AppError> {
    let st = state.inner().clone();
    may_edit_images(&st)?;
    blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_parent(&window)
            .blocking_pick_folder();
        let Some(folder) = picked else {
            return Ok(None);
        };
        let folder = folder
            .into_path()
            .map_err(|_| AppError::Io("the folder cannot be read".into()))?;
        import_images_from(&st, Path::new(&st.paths.media), &folder).map(Some)
    })
    .await
}

fn shown(mut product: Product, session: &Session) -> Product {
    redact::product(&mut product, session);
    product
}

pub(crate) fn get_extras(st: &AppState, id: String) -> Result<ProductExtras, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| {
        products::get(c, &id)?;
        extras::get(c, &id)
    })
}

/// Without `products.price` the floor price stays as stored (like `products_update` keeps the
/// sale prices): the storekeeper edits the references, not the prices.
pub(crate) fn set_extras(
    st: &AppState,
    id: String,
    mut input: ProductExtrasInput,
) -> Result<ProductExtras, AppError> {
    let session = st.require(Permission::ProductsEdit)?;
    let actor = session.actor();
    let keep_floor = !session.has(Permission::ProductsPrice);
    st.db.write(move |tx| {
        products::get(tx, &id)?;
        if keep_floor {
            input.min_price = extras::get(tx, &id)?.min_price;
        }
        extras::set(tx, &actor, &id, input)
    })
}

pub(crate) fn list_breaks(st: &AppState, id: String) -> Result<Vec<PriceBreak>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| breaks::list(c, &id))
}

pub(crate) fn replace_breaks(
    st: &AppState,
    id: String,
    list: Vec<PriceBreakInput>,
) -> Result<Vec<PriceBreak>, AppError> {
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db
        .write(move |tx| breaks::replace(tx, &actor, &id, list))
}

pub(crate) fn next_ean13(st: &AppState) -> Result<String, AppError> {
    st.require(Permission::ProductsEdit)?;
    st.db.write(|tx| codes::next_internal_ean13(tx))
}

pub(crate) fn next_reference(st: &AppState) -> Result<String, AppError> {
    st.require(Permission::ProductsEdit)?;
    st.db.write(|tx| codes::next_reference(tx))
}

/// New products: `products.edit` may set their first prices, as `products_create` does.
pub(crate) fn generate_variants(
    st: &AppState,
    parent_id: String,
    input: VariantsInput,
) -> Result<Vec<Product>, AppError> {
    let session = st.require(Permission::ProductsEdit)?;
    let actor = session.actor();
    let made = st
        .db
        .write(move |tx| variants::generate(tx, &actor, &parent_id, input))?;
    Ok(made.into_iter().map(|p| shown(p, &session)).collect())
}

pub(crate) fn bulk_price_preview(
    st: &AppState,
    input: BulkPriceInput,
) -> Result<Vec<PriceChangeRow>, AppError> {
    st.require(Permission::ProductsPrice)?;
    st.db.read(move |c| bulk::preview_prices(c, &input))
}

pub(crate) fn bulk_price_apply(st: &AppState, input: BulkPriceInput) -> Result<u32, AppError> {
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db
        .write(move |tx| bulk::apply_prices(tx, &actor, &input))
}

/// A sale price in the edits needs `products.price`, a cost needs `reports.profit` (USR-012).
pub(crate) fn bulk_edit(st: &AppState, edits: Vec<BulkEdit>) -> Result<u32, AppError> {
    let session = st.require(Permission::ProductsEdit)?;
    if edits.iter().any(|e| e.sell_price.is_some()) {
        auth::require(&session, Permission::ProductsPrice)?;
    }
    let actor = session.actor();
    let may_edit_costs = redact::sees_costs(&session);
    st.db
        .write(move |tx| bulk::apply_edits(tx, &actor, &edits, may_edit_costs))
}

pub(crate) fn price_help(st: &AppState, input: PriceHelpInput) -> Result<PriceHelp, AppError> {
    st.require_any(PRICE_HELP)?;
    st.db.read(move |c| pricing::help(c, &input))
}

/// The guard of the photo commands, checked before any dialog opens.
pub(crate) fn may_edit_images(st: &AppState) -> Result<Session, AppError> {
    st.require(Permission::ProductsEdit)
}

/// A picked file, refused before it is read when it is too large.
fn read_image_file(path: &Path) -> Result<Vec<u8>, AppError> {
    let meta =
        std::fs::metadata(path).map_err(|_| AppError::validation("image", "image_invalid"))?;
    if meta.len() > pos_docs::media::MAX_IMAGE_BYTES as u64 {
        return Err(AppError::validation("image", "image_too_large"));
    }
    std::fs::read(path).map_err(|_| AppError::validation("image", "image_invalid"))
}

/// Resizes `bytes`, writes the files, points the product at them, then deletes the previous
/// photo (after the commit: a failed write leaves the old photo in place and removes the new).
pub(crate) fn set_image_bytes(
    st: &AppState,
    media: &Path,
    id: String,
    bytes: &[u8],
) -> Result<Product, AppError> {
    let session = may_edit_images(st)?;
    let name = crate::media::save_product_image(media, &id, now_ms(), bytes)?;
    let actor = session.actor();
    let (pid, new_name) = (id.clone(), name.clone());
    match st
        .db
        .write(move |tx| images::set(tx, &actor, &pid, Some(&new_name)))
    {
        Ok(previous) => {
            if let Some(old) = previous.filter(|old| *old != name) {
                crate::media::remove(media, &old);
            }
            let product = st.db.read(move |c| products::get(c, &id))?;
            Ok(shown(product, &session))
        }
        Err(e) => {
            crate::media::remove(media, &name);
            Err(e)
        }
    }
}

/// The product shows no photo any more; its files are deleted after the commit.
pub(crate) fn clear_image(st: &AppState, media: &Path, id: String) -> Result<Product, AppError> {
    let session = may_edit_images(st)?;
    let actor = session.actor();
    let pid = id.clone();
    let previous = st.db.write(move |tx| images::set(tx, &actor, &pid, None))?;
    if let Some(old) = previous {
        crate::media::remove(media, &old);
    }
    let product = st.db.read(move |c| products::get(c, &id))?;
    Ok(shown(product, &session))
}

/// The photos of `folder` (not its subfolders), in name order: a file named after a product's
/// barcode or reference becomes its photo, one journaled change per product.
pub(crate) fn import_images_from(
    st: &AppState,
    media: &Path,
    folder: &Path,
) -> Result<ImagesImported, AppError> {
    may_edit_images(st)?;
    let entries =
        std::fs::read_dir(folder).map_err(|e| AppError::Io(format!("photos folder: {e}")))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image_name(path))
        .collect();
    files.sort();
    let mut done = ImagesImported::default();
    for path in files {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().trim().to_owned())
            .unwrap_or_default();
        let Some(product_id) = st.db.read(move |c| images::find_by_code(c, &stem))? else {
            done.unmatched.push(file_name);
            continue;
        };
        let saved =
            read_image_file(&path).and_then(|bytes| set_image_bytes(st, media, product_id, &bytes));
        match saved {
            Ok(_) => done.matched += 1,
            Err(AppError::Validation { .. }) => done.refused.push(file_name),
            Err(other) => return Err(other),
        }
    }
    Ok(done)
}

fn is_image_name(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
}

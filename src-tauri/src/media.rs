//! Product photos on disk (PRD-009): `<data>/media/products/<id>-<version>.jpg` and its
//! thumbnail, written only here, served read-only by the asset protocol scoped to `media/`.

use pos_core::catalog::images;
use pos_core::error::AppError;
use std::path::Path;

fn io(e: std::io::Error) -> AppError {
    AppError::Internal(format!("media: {e}"))
}

/// Re-encodes `bytes` (`pos_docs::media`) and writes the photo and its thumbnail under `root`;
/// returns the photo's media name. Nothing is written when the picture is refused.
pub(crate) fn save_product_image(
    root: &Path,
    product_id: &str,
    version: i64,
    bytes: &[u8],
) -> Result<String, AppError> {
    let pictures = pos_docs::media::product_images(bytes)?;
    let name = images::file_name(product_id, version);
    let thumb = images::thumb_name(&name).ok_or_else(|| AppError::Internal("media name".into()))?;
    let path = root.join(&name);
    let dir = path
        .parent()
        .ok_or_else(|| AppError::Internal("media path".into()))?;
    std::fs::create_dir_all(dir).map_err(io)?;
    let written = std::fs::write(&path, pictures.full)
        .and_then(|()| std::fs::write(root.join(&thumb), pictures.thumb));
    if let Err(error) = written {
        remove(root, &name);
        return Err(io(error));
    }
    Ok(name)
}

/// Deletes a photo and its thumbnail; a name that is not a media name is ignored.
pub(crate) fn remove(root: &Path, name: &str) {
    if let Some(thumb) = images::thumb_name(name) {
        let _ = std::fs::remove_file(root.join(name));
        let _ = std::fs::remove_file(root.join(thumb));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "01923f4e-8a1b-7c2d-9e3f-4a5b6c7d8e9f";

    fn png() -> Vec<u8> {
        let mut out = Vec::new();
        image::DynamicImage::new_rgb8(64, 48)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn a_photo_and_its_thumbnail_land_under_products() {
        let dir = tempfile::tempdir().unwrap();
        let name = save_product_image(dir.path(), ID, 7, &png()).unwrap();
        assert_eq!(name, format!("products/{ID}-7.jpg"));
        assert!(dir.path().join(&name).is_file());
        assert!(
            dir.path()
                .join(format!("products/{ID}-7.thumb.jpg"))
                .is_file()
        );
        assert_eq!(
            save_product_image(dir.path(), ID, 8, b"GIF89a")
                .unwrap_err()
                .code(),
            "image_invalid"
        );
        assert!(!dir.path().join(format!("products/{ID}-8.jpg")).exists());
        remove(dir.path(), &name);
        assert_eq!(
            std::fs::read_dir(dir.path().join("products"))
                .unwrap()
                .count(),
            0
        );
    }

    /// A name read back from the database never deletes anything outside the media folder.
    #[test]
    fn only_media_names_are_ever_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media");
        std::fs::create_dir_all(root.join("products")).unwrap();
        std::fs::write(dir.path().join("pos.db"), b"data").unwrap();
        std::fs::write(root.join("products/notes.txt"), b"keep").unwrap();
        for name in [
            "../pos.db",
            "products/../../pos.db",
            "products/notes.txt",
            "C:\\pos.db",
        ] {
            remove(&root, name);
        }
        assert!(dir.path().join("pos.db").exists());
        assert!(root.join("products/notes.txt").exists());
    }
}

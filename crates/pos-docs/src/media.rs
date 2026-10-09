//! Product photos (PRD-004): any PNG, JPEG or WebP a phone or a scanner produces, re-encoded
//! as a JPEG of at most 512 px and a 128 px thumbnail, so a 4 GB till never loads a 12 MP photo
//! into its product grid. Oversized files and decompression bombs are refused before decoding.

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader, Limits};
use pos_core::error::AppError;
use std::io::Cursor;

pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SIDE: u32 = 8_000;

#[derive(Debug)]
pub struct ProductImages {
    pub full: Vec<u8>,
    pub thumb: Vec<u8>,
}

fn too_large() -> AppError {
    AppError::validation("image", "image_too_large")
}

fn jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, AppError> {
    let mut out = Vec::new();
    img.to_rgb8()
        .write_with_encoder(JpegEncoder::new_with_quality(&mut out, quality))
        .map_err(|_| AppError::validation("image", "image_invalid"))?;
    Ok(out)
}

pub fn product_images(bytes: &[u8]) -> Result<ProductImages, AppError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(too_large());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| AppError::validation("image", "image_invalid"))?;
    if reader.format().is_none() {
        return Err(AppError::validation("image", "image_invalid"));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => too_large(),
        _ => AppError::validation("image", "image_invalid"),
    })?;
    let full = if img.width() > 512 || img.height() > 512 {
        img.resize(512, 512, image::imageops::FilterType::Triangle)
    } else {
        img.clone()
    };
    let thumb = img.thumbnail(128, 128);
    Ok(ProductImages {
        full: jpeg(&full, 85)?,
        thumb: jpeg(&thumb, 80)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbImage::new(width, height)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn photos_become_a_512_jpeg_and_a_thumbnail() {
        let images = product_images(&png(2_048, 1_200)).unwrap();
        let full = image::load_from_memory(&images.full).unwrap();
        let thumb = image::load_from_memory(&images.thumb).unwrap();
        assert_eq!((full.width(), full.height()), (512, 300));
        assert_eq!((thumb.width(), thumb.height()), (128, 75));
        assert_eq!(&images.full[..2], &[0xFF, 0xD8], "JPEG");
        let small = product_images(&png(200, 100)).unwrap();
        assert_eq!(
            image::load_from_memory(&small.full).unwrap().width(),
            200,
            "never enlarged"
        );
    }

    #[test]
    fn garbage_and_giants_are_refused() {
        assert_eq!(
            product_images(b"not an image").unwrap_err().code(),
            "image_invalid"
        );
        assert_eq!(
            product_images(&vec![0_u8; MAX_IMAGE_BYTES + 1])
                .unwrap_err()
                .code(),
            "image_too_large"
        );
        assert_eq!(
            product_images(&png(9_000, 10)).unwrap_err().code(),
            "image_too_large"
        );
    }
}

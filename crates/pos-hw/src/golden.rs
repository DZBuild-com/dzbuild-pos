//! Golden images: a rendered bitmap must equal `golden/<name>.png` dot for dot. Run the tests
//! with `UPDATE_GOLDEN=1` to (re)write the files, then look at them before committing.

use crate::bitmap::Bitmap;
use std::path::PathBuf;

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("golden")
        .join(format!("{name}.png"))
}

/// 1-bit grayscale PNG: 0 = black ink, 1 = paper.
pub(crate) fn to_png(bitmap: &Bitmap) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, bitmap.width(), bitmap.height());
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::One);
        let mut writer = encoder.write_header().unwrap();
        let inverted: Vec<u8> = bitmap.rows(0, bitmap.height()).iter().map(|b| !b).collect();
        writer.write_image_data(&inverted).unwrap();
    }
    out
}

fn from_png(bytes: &[u8]) -> Bitmap {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    let mut bitmap = Bitmap::new(info.width, info.height);
    let row = info.line_size;
    for y in 0..info.height {
        for x in 0..info.width {
            let byte = buffer[y as usize * row + (x / 8) as usize];
            bitmap.set(x, y, byte & (0x80 >> (x % 8)) == 0);
        }
    }
    bitmap
}

pub(crate) fn assert_golden(name: &str, bitmap: &Bitmap) {
    let file = path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, to_png(bitmap)).unwrap();
        return;
    }
    let golden = std::fs::read(&file).unwrap_or_else(|_| {
        panic!(
            "missing {}: run with UPDATE_GOLDEN=1 and review it",
            file.display()
        )
    });
    if from_png(&golden) != *bitmap {
        let actual = file.with_extension("actual.png");
        std::fs::write(&actual, to_png(bitmap)).unwrap();
        panic!(
            "{name} differs from its golden image; see {}",
            actual.display()
        );
    }
}

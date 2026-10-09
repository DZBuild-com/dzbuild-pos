//! 1-bit images in printer order (`GS v 0`): rows top to bottom, 8 dots per byte, most
//! significant bit first, a set bit is a black dot.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

fn row_bytes(width: u32) -> usize {
    width.div_ceil(8) as usize
}

impl Bitmap {
    /// A white image.
    pub fn new(width: u32, height: u32) -> Bitmap {
        Bitmap {
            width,
            height,
            data: vec![0; row_bytes(width) * height as usize],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn row_bytes(&self) -> usize {
        row_bytes(self.width)
    }

    fn index(&self, x: u32, y: u32) -> Option<(usize, u8)> {
        (x < self.width && y < self.height).then(|| {
            (
                y as usize * self.row_bytes() + (x / 8) as usize,
                0x80_u8 >> (x % 8),
            )
        })
    }

    /// Dots outside the image are ignored.
    pub fn set(&mut self, x: u32, y: u32, black: bool) {
        if let Some((i, mask)) = self.index(x, y)
            && let Some(byte) = self.data.get_mut(i)
        {
            if black {
                *byte |= mask;
            } else {
                *byte &= !mask;
            }
        }
    }

    pub fn get(&self, x: u32, y: u32) -> bool {
        self.index(x, y)
            .and_then(|(i, mask)| self.data.get(i).map(|b| b & mask != 0))
            .unwrap_or(false)
    }

    /// The packed bytes of `count` rows starting at `from`, clamped to the image.
    pub fn rows(&self, from: u32, count: u32) -> &[u8] {
        let from = from.min(self.height) as usize;
        let to = from
            .saturating_add(count as usize)
            .min(self.height as usize);
        let row = self.row_bytes();
        self.data.get(from * row..to * row).unwrap_or(&[])
    }

    pub fn black_dots(&self) -> usize {
        self.data.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// `other` drawn with its top-left corner at (`x`, `y`); black dots only, clipped.
    pub fn paste(&mut self, other: &Bitmap, x: u32, y: u32) {
        for oy in 0..other.height {
            for ox in 0..other.width {
                if other.get(ox, oy) {
                    // saturating: a dot past u32::MAX is outside the image, never wrapped back in
                    self.set(x.saturating_add(ox), y.saturating_add(oy), true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_are_packed_msb_first() {
        let mut b = Bitmap::new(10, 2);
        assert_eq!(b.row_bytes(), 2);
        b.set(0, 0, true);
        b.set(9, 0, true);
        b.set(7, 1, true);
        assert_eq!(b.rows(0, 1), [0b1000_0000, 0b0100_0000]);
        assert_eq!(b.rows(1, 1), [0b0000_0001, 0]);
        assert!(b.get(9, 0) && !b.get(8, 0));
        assert_eq!(b.black_dots(), 3);
        b.set(99, 99, true); // outside: ignored
        assert_eq!(b.black_dots(), 3);
    }

    #[test]
    fn paste_places_and_clips() {
        let mut logo = Bitmap::new(3, 2);
        for x in 0..3 {
            logo.set(x, 0, true);
        }
        let mut page = Bitmap::new(8, 4);
        page.paste(&logo, 6, 3);
        assert!(page.get(6, 3) && page.get(7, 3));
        assert_eq!(
            page.black_dots(),
            2,
            "the third dot and the second row fall outside"
        );
    }
}

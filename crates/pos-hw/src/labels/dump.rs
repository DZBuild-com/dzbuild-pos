//! TSPL and ZPL as readable text (snapshots, support logs): commands as they are, bitmap
//! payloads summarised by size and an FNV-1a hash, so a changed dot changes the snapshot.

use std::fmt::Write as _;

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn number(bytes: &[u8]) -> Option<usize> {
    std::str::from_utf8(bytes).ok()?.trim().parse().ok()
}

/// The positions of the first `n` commas of `bytes`.
fn commas(bytes: &[u8], n: usize) -> Vec<usize> {
    bytes
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b',')
        .map(|(p, _)| p)
        .take(n)
        .collect()
}

pub fn dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        // BITMAP x,y,width_bytes,height,mode,<data>
        if rest.starts_with(b"BITMAP ")
            && let [_, c2, c3, c4, c5] = commas(rest, 5)[..]
            && let (Some(wb), Some(h)) = (number(&rest[c2 + 1..c3]), number(&rest[c3 + 1..c4]))
        {
            let data_start = c5 + 1;
            let data_end = data_start
                .saturating_add(wb.saturating_mul(h))
                .min(rest.len());
            out.push_str(&String::from_utf8_lossy(&rest[..data_start]));
            let _ = write!(
                out,
                "<bitmap {wb}x{h} fnv {:016x}>",
                fnv(&rest[data_start..data_end])
            );
            i += data_end;
            continue;
        }
        // ^GFA,total,total,row_bytes,<hex>^FS
        if rest.starts_with(b"^GFA,")
            && let Some(&c4) = commas(rest, 4).get(3)
            && let Some(end) = rest.windows(3).position(|w| w == b"^FS")
            && end > c4
        {
            let hex = &rest[c4 + 1..end];
            out.push_str(&String::from_utf8_lossy(&rest[..=c4]));
            let _ = write!(out, "<hex {} fnv {:016x}>", hex.len(), fnv(hex));
            i += end;
            continue;
        }
        let b = bytes[i];
        if b == b'\n' || b == b'\r' || (0x20..0x7F).contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "<{b:02X}>");
        }
        i += 1;
    }
    out
}

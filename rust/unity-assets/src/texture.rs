//! Texture2D pixel formats and decoding of mip 0 to RGBA8.

use crate::{format_err, unsupported, Result};
use texture2ddecoder as t2d;

/// Unity `TextureFormat` name, for listings.
pub fn format_name(f: i32) -> &'static str {
    match f {
        1 => "Alpha8",
        2 => "ARGB4444",
        3 => "RGB24",
        4 => "RGBA32",
        5 => "ARGB32",
        7 => "RGB565",
        9 => "R16",
        10 => "DXT1",
        12 => "DXT5",
        13 => "RGBA4444",
        14 => "BGRA32",
        15 => "RHalf",
        16 => "RGHalf",
        17 => "RGBAHalf",
        18 => "RFloat",
        19 => "RGFloat",
        20 => "RGBAFloat",
        22 => "RGB9e5Float",
        24 => "BC6H",
        25 => "BC7",
        26 => "BC4",
        27 => "BC5",
        28 => "DXT1Crunched",
        29 => "DXT5Crunched",
        62 => "RG16",
        63 => "R8",
        _ => "Unknown",
    }
}

/// Byte size of mip 0, when the format has a fixed size (not crunched).
pub fn mip0_size(format: i32, w: usize, h: usize) -> Option<usize> {
    let blocks = w.div_ceil(4) * h.div_ceil(4);
    Some(match format {
        1 | 63 => w * h,
        2 | 7 | 9 | 13 | 15 | 62 => w * h * 2,
        3 => w * h * 3,
        4 | 5 | 14 | 16 | 18 | 22 => w * h * 4,
        17 | 19 => w * h * 8,
        20 => w * h * 16,
        10 | 26 => blocks * 8,
        12 | 24 | 25 | 27 => blocks * 16,
        _ => return None,
    })
}

/// Decodes mip 0 of `data` to RGBA8, rows top to bottom (Unity stores them bottom-up).
pub fn decode_rgba8(format: i32, w: usize, h: usize, data: &[u8]) -> Result<Vec<u8>> {
    if w == 0 || h == 0 {
        // A 0-sized texture (streaming placeholder / empty slot) is a valid object with no pixels;
        // it decodes to an empty image rather than being an error.
        return Ok(Vec::new());
    }
    let src = match mip0_size(format, w, h) {
        Some(n) if data.len() < n => return format_err(format!("{} needs {n} bytes, have {}", format_name(format), data.len())),
        Some(n) => &data[..n],
        None => data,
    };
    let mut out = vec![0u8; w * h * 4];
    let px = |out: &mut [u8], f: &dyn Fn(usize) -> [u8; 4], bpp: usize| {
        for (i, o) in out.chunks_exact_mut(4).enumerate() {
            o.copy_from_slice(&f(i * bpp));
        }
    };
    match format {
        1 => px(&mut out, &|i| [255, 255, 255, src[i]], 1),
        63 => px(&mut out, &|i| [src[i], 0, 0, 255], 1),
        62 => px(&mut out, &|i| [src[i], src[i + 1], 0, 255], 2),
        9 => px(&mut out, &|i| [src[i + 1], 0, 0, 255], 2),
        3 => px(&mut out, &|i| [src[i], src[i + 1], src[i + 2], 255], 3),
        4 => out.copy_from_slice(src),
        5 => px(&mut out, &|i| [src[i + 1], src[i + 2], src[i + 3], src[i]], 4),
        14 => px(&mut out, &|i| [src[i + 2], src[i + 1], src[i], src[i + 3]], 4),
        7 => px(
            &mut out,
            &|i| {
                let v = u16::from_le_bytes([src[i], src[i + 1]]);
                let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8, 255]
            },
            2,
        ),
        2 | 13 => px(
            &mut out,
            &|i| {
                let v = u16::from_le_bytes([src[i], src[i + 1]]);
                let n = |s: u16| ((v >> s) & 15) as u8 * 17;
                if format == 2 { [n(8), n(4), n(0), n(12)] } else { [n(12), n(8), n(4), n(0)] }
            },
            2,
        ),
        10 | 12 | 24 | 25 | 26 | 27 | 28 | 29 => {
            let mut img = vec![0u32; w * h];
            let res = match format {
                10 => t2d::decode_bc1(src, w, h, &mut img),
                12 => t2d::decode_bc3(src, w, h, &mut img),
                24 => t2d::decode_bc6_unsigned(src, w, h, &mut img),
                25 => t2d::decode_bc7(src, w, h, &mut img),
                26 => t2d::decode_bc4(src, w, h, &mut img),
                27 => t2d::decode_bc5(src, w, h, &mut img),
                _ => t2d::decode_unity_crunch(src, w, h, &mut img),
            };
            res.map_err(|e| crate::Error::Format(format!("{}: {e}", format_name(format))))?;
            for (o, p) in out.chunks_exact_mut(4).zip(img) {
                let [b, g, r, a] = p.to_le_bytes();
                o.copy_from_slice(&[r, g, b, a]);
            }
        }
        _ => return unsupported(format!("texture format {} ({format})", format_name(format))),
    }
    let row = w * 4;
    for y in 0..h / 2 {
        let (a, b) = out.split_at_mut((h - 1 - y) * row);
        a[y * row..(y + 1) * row].swap_with_slice(&mut b[..row]);
    }
    Ok(out)
}

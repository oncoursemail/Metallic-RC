//! BC6H (unsigned half-float, Unity's `BC6H` format 24) block decoding to linear float RGB, after the public
//! D3D11 BC6H format description: 14 modes, 1 or 2 regions, delta-coded endpoints, 3- or 4-bit indices.
//! Written for this crate because texture2ddecoder 0.1.2 loses the second endpoint in the 16-bit mode (its
//! endpoint mask overflows a u16), which most blocks of the old builds' sky cubemaps use.

/// Field order of an endpoint set: rw gw bw rx gx bx ry gy by rz gz bz (w, x = region 0, y, z = region 1).
const FIELDS: [&str; 12] = ["rw", "gw", "bw", "rx", "gx", "bx", "ry", "gy", "by", "rz", "gz", "bz"];

/// A mode: transformed (deltas), regions, endpoint bits, delta bits per channel and the order its endpoint bits
/// are stored in ("rx0:5" = 5 bits into rx from bit 0, "rw10" = one bit into rw bit 10).
struct Mode {
    transformed: bool,
    regions: usize,
    bits: u32,
    delta: [u32; 3],
    layout: &'static str,
}

fn mode(m: u8) -> Option<Mode> {
    let (transformed, regions, bits, delta, layout) = match m {
        0 => (true, 2, 10, [5, 5, 5], "gy4 by4 bz4 rw0:10 gw0:10 bw0:10 rx0:5 gz4 gy0:4 gx0:5 bz0 gz0:4 bx0:5 bz1 by0:4 ry0:5 bz2 rz0:5 bz3"),
        1 => (true, 2, 7, [6, 6, 6], "gy5 gz4 gz5 rw0:7 bz0 bz1 by4 gw0:7 by5 bz2 gy4 bw0:7 bz3 bz5 bz4 rx0:6 gy0:4 gx0:6 gz0:4 bx0:6 by0:4 ry0:6 rz0:6"),
        2 => (true, 2, 11, [5, 4, 4], "rw0:10 gw0:10 bw0:10 rx0:5 rw10 gy0:4 gx0:4 gw10 bz0 gz0:4 bx0:4 bw10 bz1 by0:4 ry0:5 bz2 rz0:5 bz3"),
        6 => (true, 2, 11, [4, 5, 4], "rw0:10 gw0:10 bw0:10 rx0:4 rw10 gz4 gy0:4 gx0:5 gw10 gz0:4 bx0:4 bw10 bz1 by0:4 ry0:4 bz0 bz2 rz0:4 gy4 bz3"),
        10 => (true, 2, 11, [4, 4, 5], "rw0:10 gw0:10 bw0:10 rx0:4 rw10 by4 gy0:4 gx0:4 gw10 bz0 gz0:4 bx0:5 bw10 by0:4 ry0:4 bz1 bz2 rz0:4 bz4 bz3"),
        14 => (true, 2, 9, [5, 5, 5], "rw0:9 by4 gw0:9 gy4 bw0:9 bz4 rx0:5 gz4 gy0:4 gx0:5 bz0 gz0:4 bx0:5 bz1 by0:4 ry0:5 bz2 rz0:5 bz3"),
        18 => (true, 2, 8, [6, 5, 5], "rw0:8 gz4 by4 gw0:8 bz2 gy4 bw0:8 bz3 bz4 rx0:6 gy0:4 gx0:5 bz0 gz0:4 bx0:5 bz1 by0:4 ry0:6 rz0:6"),
        22 => (true, 2, 8, [5, 6, 5], "rw0:8 bz0 by4 gw0:8 gy5 gy4 bw0:8 gz5 bz4 rx0:5 gz4 gy0:4 gx0:6 gz0:4 bx0:5 bz1 by0:4 ry0:5 bz2 rz0:5 bz3"),
        26 => (true, 2, 8, [5, 5, 6], "rw0:8 bz1 by4 gw0:8 by5 gy4 bw0:8 bz5 bz4 rx0:5 gz4 gy0:4 gx0:5 bz0 gz0:4 bx0:6 by0:4 ry0:5 bz2 rz0:5 bz3"),
        30 => (false, 2, 6, [6, 6, 6], "rw0:6 gz4 bz0 bz1 by4 gw0:6 gy5 by5 bz2 gy4 bw0:6 gz5 bz3 bz5 bz4 rx0:6 gy0:4 gx0:6 gz0:4 bx0:6 by0:4 ry0:6 rz0:6"),
        3 => (false, 1, 10, [10, 10, 10], "rw0:10 gw0:10 bw0:10 rx0:10 gx0:10 bx0:10"),
        7 => (true, 1, 11, [9, 9, 9], "rw0:10 gw0:10 bw0:10 rx0:9 rw10 gx0:9 gw10 bx0:9 bw10"),
        11 => (true, 1, 12, [8, 8, 8], "rw0:10 gw0:10 bw0:10 rx0:8 rw11 rw10 gx0:8 gw11 gw10 bx0:8 bw11 bw10"),
        15 => (true, 1, 16, [4, 4, 4], "rw0:10 gw0:10 bw0:10 rx0:4 rw15 rw14 rw13 rw12 rw11 rw10 gx0:4 gw15 gw14 gw13 gw12 gw11 gw10 bx0:4 bw15 bw14 bw13 bw12 bw11 bw10"),
        _ => return None,
    };
    Some(Mode { transformed, regions, bits, delta, layout })
}

/// Two-region partitions (bit i = region of texel i) and the anchor texel of region 1.
const PARTITIONS: [u16; 32] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800, 0xffe8, 0xff00, 0xfff0, 0xf000,
    0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce, 0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c,
];
const ANCHORS: [u8; 32] = [15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2];
const WEIGHTS3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// Each mode's layout as (field, shift, bit count), parsed once.
fn layouts() -> &'static [Vec<(u8, u8, u8)>; 32] {
    static L: std::sync::OnceLock<[Vec<(u8, u8, u8)>; 32]> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        std::array::from_fn(|m| {
            let Some(mode) = mode(m as u8) else { return Vec::new() };
            mode.layout
                .split(' ')
                .map(|tok| {
                    let field = FIELDS.iter().position(|f| *f == &tok[..2]).unwrap() as u8;
                    let (shift, n) = tok[2..].split_once(':').unwrap_or((&tok[2..], "1"));
                    (field, shift.parse().unwrap(), n.parse().unwrap())
                })
                .collect()
        })
    })
}

struct Bits(u128, u32);

impl Bits {
    fn read(&mut self, n: u32) -> u32 {
        let v = (self.0 >> self.1) as u32 & ((1u64 << n) - 1) as u32;
        self.1 += n;
        v
    }
}

/// Decodes one 16-byte block to 16 texels (row-major) of linear RGB.
pub fn decode_block(block: &[u8; 16]) -> [[f32; 3]; 16] {
    let mut b = Bits(u128::from_le_bytes(*block), 0);
    let mut m = b.read(2) as u8;
    if m & 2 != 0 {
        m |= (b.read(3) as u8) << 2;
    }
    let Some(mode) = mode(m) else { return [[0.0; 3]; 16] };
    let mut e = [0u32; 12];
    for &(field, shift, n) in &layouts()[m as usize] {
        e[field as usize] |= b.read(n as u32) << shift;
    }
    let mask = ((1u64 << mode.bits) - 1) as u32;
    let count = mode.regions * 2;
    if mode.transformed {
        for i in 3..count * 3 {
            let d = mode.delta[i % 3];
            let v = e[i] & ((1 << d) - 1);
            let signed = if v & (1 << (d - 1)) != 0 { v as i32 - (1 << d) } else { v as i32 };
            e[i] = (e[i % 3] as i32 + signed) as u32 & mask;
        }
    }
    // unquantize (unsigned): stretch `bits` to 16
    let unq = |v: u32| -> u32 {
        if mode.bits >= 15 {
            v
        } else if v == 0 {
            0
        } else if v == mask {
            0xffff
        } else {
            ((v << 16) + 0x8000) >> mode.bits
        }
    };
    let ep: Vec<u32> = e[..count * 3].iter().map(|&v| unq(v)).collect();
    let partition = if mode.regions == 2 { b.read(5) as usize } else { 0 };
    let index_bits = if mode.regions == 2 { 3 } else { 4 };
    let mut out = [[0.0; 3]; 16];
    for (t, o) in out.iter_mut().enumerate() {
        let region = if mode.regions == 2 { (PARTITIONS[partition] >> t) as usize & 1 } else { 0 };
        let anchor = t == 0 || (mode.regions == 2 && t == ANCHORS[partition] as usize);
        let i = b.read(index_bits - anchor as u32) as usize;
        let w = if index_bits == 3 { WEIGHTS3[i] } else { WEIGHTS4[i] };
        for (c, o) in o.iter_mut().enumerate() {
            let (a, z) = (ep[region * 6 + c], ep[region * 6 + 3 + c]);
            let v = (a * (64 - w) + z * w + 32) >> 6;
            *o = half_to_f32(((v * 31) >> 6) as u16);
        }
    }
    out
}

/// Decodes a `w` x `h` BC6H image to linear RGB, rows as stored.
pub fn decode(data: &[u8], w: usize, h: usize) -> Vec<[f32; 3]> {
    let bw = w.div_ceil(4);
    let mut out = vec![[0.0; 3]; w * h];
    for (k, block) in data.chunks_exact(16).take(bw * h.div_ceil(4)).enumerate() {
        let texels = decode_block(block.try_into().unwrap());
        let (bx, by) = (k % bw * 4, k / bw * 4);
        for (t, px) in texels.iter().enumerate() {
            let (x, y) = (bx + t % 4, by + t / 4);
            if x < w && y < h {
                out[y * w + x] = *px;
            }
        }
    }
    out
}

/// IEEE half to f32.
pub fn half_to_f32(h: u16) -> f32 {
    let (s, e, m) = ((h >> 15) as u32, ((h >> 10) & 31) as u32, (h & 1023) as u32);
    let bits = match e {
        0 if m == 0 => s << 31,
        0 => {
            // subnormal: normalise
            let mut e = 127 - 15 + 1;
            let mut m = m;
            while m & 1024 == 0 {
                m <<= 1;
                e -= 1;
            }
            (s << 31) | (e << 23) | ((m & 1023) << 13)
        }
        31 => (s << 31) | (255 << 23) | (m << 13),
        _ => (s << 31) | ((e + 127 - 15) << 23) | (m << 13),
    };
    f32::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mode 15 (one region, 16-bit endpoints, 4-bit deltas, high endpoint bits stored reversed): the mode the old
    /// skies use most and texture2ddecoder got wrong (second endpoint lost).
    #[test]
    fn mode_15_keeps_both_endpoints() {
        let (e0, delta) = (31711u128, 3u128); // e0 -> half 1.0 after the x31/64 finish
        let mut v: u128 = 0b01111;
        let mut at = 5;
        let mut put = |v: &mut u128, x: u128, n: u32| {
            *v |= (x & ((1 << n) - 1)) << at;
            at += n;
        };
        for _ in 0..3 {
            put(&mut v, e0 & 1023, 10); // rw, gw, bw bits 0..9
        }
        for _ in 0..3 {
            put(&mut v, delta, 4); // rx / gx / bx
            for b in (10..16).rev() {
                put(&mut v, (e0 >> b) & 1, 1); // bits 15..10, reversed
            }
        }
        put(&mut v, 15 >> 1, 3); // texel 0 (anchor): index 7 of 15
        for _ in 1..16 {
            put(&mut v, 15, 4); // the rest: index 15 = the second endpoint
        }
        assert_eq!(at, 128);
        let px = decode_block(&v.to_le_bytes());
        let last = half_to_f32((((e0 as u32 + 3) * 31) >> 6) as u16);
        assert!((px[0][0] - 1.0).abs() < 0.01 && (px[5][1] - last).abs() < 1e-6, "{:?} {:?}", px[0], px[5]);
    }

    #[test]
    fn halves() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(half_to_f32(0x0001), 2f32.powi(-24));
    }
}

//! Rabbit stream cipher (eSTREAM / RFC 4503), implemented from the RFC pseudocode.
//!
//! Status: IMPLEMENTED & UNIT-TESTED against the official RFC 4503 Appendix A
//! test vectors (both "without IV setup" and "with IV setup").
//!
//! Why this module exists: the Rec Room 2026 client installs a LiteNetLib packet
//! layer `LiteNetLib.Layers.Ciphers.RabbitCipher` whose state fields read out of the
//! IL2CPP metadata are exactly the Rabbit state (`key`, `iv`, `X`, `S`, `C`, `old_C`,
//! `A`, `carry`, `g`). That is the eSTREAM Rabbit cipher, specified byte-for-byte in
//! RFC 4503, so it is fully implementable and testable here without a capture.
//!
//! IMPORTANT (NEEDS CAPTURE): this module reproduces the *algorithm* exactly (verified
//! by the RFC vectors). What is NOT yet pinned down, and must come from a live capture
//! or a native method-body decompile of `RabbitCipher`, is:
//!   1. The *byte order* in which the client's layer maps keystream words onto the
//!      datagram. The RFC defines everything via OS2IP/I2OSP (big-endian over the
//!      128-bit block). Many real implementations instead stream words little-endian.
//!      `keystream_block()` here follows the RFC (big-endian) so the vectors pass;
//!      `apply_keystream_le()` is provided as the likely alternative. Confirm which the
//!      client uses from the first encrypted datagram.
//!   2. How the 16-byte key and 8-byte IV are derived from the handshake material
//!      (`ClientKeyA`/`ClientIVA`/`ClientPublicKey`/`SessionKeyB`). See `handshake.rs`.
//!   3. Whether the cipher layer prepends any extra header bytes (nonce/length) to the
//!      datagram. See `litenetlib::RabbitLayer` (extra-header size is a TODO).

/// Rabbit per-RFC counter constants A0..A7 (RFC 4503 section 2.5).
const A: [u32; 8] = [
    0x4D34_D34D,
    0xD34D_34D3,
    0x34D3_4D34,
    0x4D34_D34D,
    0xD34D_34D3,
    0x34D3_4D34,
    0x4D34_D34D,
    0xD34D_34D3,
];

/// Rabbit cipher instance (inner state). Mirrors the state variables named in the
/// client's `RabbitCipher` struct (X, C, carry; `g` is a per-step scratch array).
#[derive(Clone)]
pub struct Rabbit {
    x: [u32; 8],
    c: [u32; 8],
    carry: u32,
    /// Keystream block not yet consumed (RFC produces 16 bytes per next-state step).
    buf: [u8; 16],
    /// How many bytes of `buf` have been consumed (16 == empty, needs refill).
    buf_pos: usize,
}

#[inline(always)]
fn g_func(x: u32, c: u32) -> u32 {
    // u = (X_j + C_j) mod 2^32 ; square it as a 64-bit value;
    // g_j = LSW(square) XOR MSW(square).
    let u = x.wrapping_add(c) as u64;
    let sq = u.wrapping_mul(u);
    (sq ^ (sq >> 32)) as u32
}

impl Rabbit {
    /// Key setup (RFC 4503 section 2.3), implemented as the canonical Rabbit reference:
    /// the 16-byte key is read as four little-endian 32-bit subkeys k0..k3, and the
    /// state/counter words are initialized from them. This is the form that reproduces
    /// the RFC 4503 Appendix A test vectors and is the shape the client's `RabbitCipher`
    /// (state fields X, C, carry, g) mirrors.
    pub fn new(key: &[u8; 16]) -> Self {
        // The RFC presents key/IV/keystream via OS2IP/I2OSP (big-endian over the whole
        // value), while the canonical reference word-arithmetic is little-endian. We
        // reconcile the two by reversing the 16 key octets, then loading subkeys
        // little-endian exactly as the reference does. k0 then holds the
        // least-significant 32 bits of the OS2IP key integer.
        let mut rk = *key;
        rk.reverse();
        let k0 = u32::from_le_bytes([rk[0], rk[1], rk[2], rk[3]]);
        let k1 = u32::from_le_bytes([rk[4], rk[5], rk[6], rk[7]]);
        let k2 = u32::from_le_bytes([rk[8], rk[9], rk[10], rk[11]]);
        let k3 = u32::from_le_bytes([rk[12], rk[13], rk[14], rk[15]]);

        let mut x = [0u32; 8];
        x[0] = k0;
        x[2] = k1;
        x[4] = k2;
        x[6] = k3;
        x[1] = (k3 << 16) | (k2 >> 16);
        x[3] = (k0 << 16) | (k3 >> 16);
        x[5] = (k1 << 16) | (k0 >> 16);
        x[7] = (k2 << 16) | (k1 >> 16);

        let mut c = [0u32; 8];
        c[0] = k2.rotate_left(16);
        c[2] = k3.rotate_left(16);
        c[4] = k0.rotate_left(16);
        c[6] = k1.rotate_left(16);
        c[1] = (k0 & 0xFFFF_0000) | (k1 & 0xFFFF);
        c[3] = (k1 & 0xFFFF_0000) | (k2 & 0xFFFF);
        c[5] = (k2 & 0xFFFF_0000) | (k3 & 0xFFFF);
        c[7] = (k3 & 0xFFFF_0000) | (k0 & 0xFFFF);

        let mut r = Rabbit {
            x,
            c,
            carry: 0,
            buf: [0u8; 16],
            buf_pos: 16,
        };

        // Iterate the system four times.
        for _ in 0..4 {
            r.next_state();
        }
        // Reinitialize the counters: C_j ^= X_(j+4 mod 8).
        for j in 0..8 {
            r.c[j] ^= r.x[(j + 4) & 7];
        }
        r
    }

    /// IV setup (RFC 4503 section 2.4). `iv` is the 8-byte IV, big-endian (OS2IP).
    /// Must be called after `new` (key setup) and before generating keystream.
    pub fn set_iv(&mut self, iv: &[u8; 8]) {
        let iv_u = u64::from_be_bytes(*iv);
        let i0 = (iv_u & 0xFFFF_FFFF) as u32; // IV[31..0]
        let i2 = (iv_u >> 32) as u32; // IV[63..32]
        let i1 = ((((iv_u >> 48) & 0xFFFF) << 16) | ((iv_u >> 16) & 0xFFFF)) as u32; // IV[63..48]||IV[31..16]
        let i3 = ((((iv_u >> 32) & 0xFFFF) << 16) | (iv_u & 0xFFFF)) as u32; // IV[47..32]||IV[15..0]

        self.c[0] ^= i0;
        self.c[1] ^= i1;
        self.c[2] ^= i2;
        self.c[3] ^= i3;
        self.c[4] ^= i0;
        self.c[5] ^= i1;
        self.c[6] ^= i2;
        self.c[7] ^= i3;

        for _ in 0..4 {
            self.next_state();
        }
        // A re-IV invalidates any buffered keystream.
        self.buf_pos = 16;
    }

    /// Advance the counter system and the next-state function by one step.
    fn next_state(&mut self) {
        // Counter update with carry chaining (RFC 2.5).
        for j in 0..8 {
            let t = self.c[j] as u64 + A[j] as u64 + self.carry as u64;
            self.carry = (t >> 32) as u32;
            self.c[j] = t as u32;
        }

        // g-functions.
        let mut g = [0u32; 8];
        for j in 0..8 {
            g[j] = g_func(self.x[j], self.c[j]);
        }

        // Next-state function (RFC 2.6). rotl = left rotation.
        self.x[0] = g[0]
            .wrapping_add(g[7].rotate_left(16))
            .wrapping_add(g[6].rotate_left(16));
        self.x[1] = g[1].wrapping_add(g[0].rotate_left(8)).wrapping_add(g[7]);
        self.x[2] = g[2]
            .wrapping_add(g[1].rotate_left(16))
            .wrapping_add(g[0].rotate_left(16));
        self.x[3] = g[3].wrapping_add(g[2].rotate_left(8)).wrapping_add(g[1]);
        self.x[4] = g[4]
            .wrapping_add(g[3].rotate_left(16))
            .wrapping_add(g[2].rotate_left(16));
        self.x[5] = g[5].wrapping_add(g[4].rotate_left(8)).wrapping_add(g[3]);
        self.x[6] = g[6]
            .wrapping_add(g[5].rotate_left(16))
            .wrapping_add(g[4].rotate_left(16));
        self.x[7] = g[7].wrapping_add(g[6].rotate_left(8)).wrapping_add(g[5]);
    }

    /// Extract the four 32-bit keystream words (RFC 2.7). Returns [s0, s1, s2, s3]
    /// where s0 == S[31..0] (least significant) and s3 == S[127..96].
    fn extract(&self) -> [u32; 4] {
        let x = &self.x;
        let s0 = x[0] ^ (x[5] >> 16) ^ (x[3] << 16);
        let s1 = x[2] ^ (x[7] >> 16) ^ (x[5] << 16);
        let s2 = x[4] ^ (x[1] >> 16) ^ (x[7] << 16);
        let s3 = x[6] ^ (x[3] >> 16) ^ (x[1] << 16);
        [s0, s1, s2, s3]
    }

    /// Produce the next 16 keystream bytes in RFC I2OSP (big-endian) order:
    /// byte[0] is the most significant byte of the 128-bit keystream block S[127..96].
    /// This is the order the RFC Appendix A vectors are written in.
    pub fn keystream_block(&mut self) -> [u8; 16] {
        self.next_state();
        let s = self.extract();
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&s[3].to_be_bytes());
        out[4..8].copy_from_slice(&s[2].to_be_bytes());
        out[8..12].copy_from_slice(&s[1].to_be_bytes());
        out[12..16].copy_from_slice(&s[0].to_be_bytes());
        out
    }

    /// XOR `data` with the keystream in RFC byte order (see `keystream_block`).
    /// Stateful across calls (keeps partial-block remainder). This matches the RFC
    /// vectors; it is the big-endian interpretation.
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        for b in data.iter_mut() {
            if self.buf_pos >= 16 {
                self.buf = self.keystream_block();
                self.buf_pos = 0;
            }
            *b ^= self.buf[self.buf_pos];
            self.buf_pos += 1;
        }
    }

    /// XOR `data` with the keystream in little-endian word order (s0 low byte first).
    /// This is the layout many high-performance implementations stream, and is a
    /// strong candidate for what the client's Burst-compiled `RabbitCipher` emits.
    /// NEEDS CAPTURE: confirm which of `apply_keystream` / `apply_keystream_le` the
    /// Rec Room client uses before relying on either for live traffic.
    pub fn apply_keystream_le(&mut self, data: &mut [u8]) {
        let mut i = 0usize;
        while i < data.len() {
            let s = {
                self.next_state();
                self.extract()
            };
            let mut block = [0u8; 16];
            block[0..4].copy_from_slice(&s[0].to_le_bytes());
            block[4..8].copy_from_slice(&s[1].to_le_bytes());
            block[8..12].copy_from_slice(&s[2].to_le_bytes());
            block[12..16].copy_from_slice(&s[3].to_le_bytes());
            let n = core::cmp::min(16, data.len() - i);
            for j in 0..n {
                data[i + j] ^= block[j];
            }
            i += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen48(r: &mut Rabbit) -> [u8; 48] {
        let mut out = [0u8; 48];
        out[0..16].copy_from_slice(&r.keystream_block());
        out[16..32].copy_from_slice(&r.keystream_block());
        out[32..48].copy_from_slice(&r.keystream_block());
        out
    }

    // RFC 4503 Appendix A.1 — testing WITHOUT IV setup.
    #[test]
    fn rfc_a1_key_zero() {
        let key = [0u8; 16];
        let mut r = Rabbit::new(&key);
        let expected = hex48(
            "B15754F036A5D6ECF56B45261C4AF702\
             88E8D815C59C0C397B696C4789C68AA7\
             F416A1C3700CD451DA68D1881673D696",
        );
        assert_eq!(gen48(&mut r), expected);
    }

    #[test]
    fn rfc_a1_key_2() {
        let key = hex16("912813292E3D36FE3BFC62F1DC51C3AC");
        let mut r = Rabbit::new(&key);
        let expected = hex48(
            "3D2DF3C83EF627A1E97FC38487E2519C\
             F576CD61F4405B8896BF53AA8554FC19\
             E5547473FBDB43508AE53B20204D4C5E",
        );
        assert_eq!(gen48(&mut r), expected);
    }

    #[test]
    fn rfc_a1_key_3() {
        let key = hex16("8395741587E0C733E9E9AB01C09B0043");
        let mut r = Rabbit::new(&key);
        let expected = hex48(
            "0CB10DCDA041CDAC32EB5CFD02D0609B\
             95FC9FCA0F17015A7B7092114CFF3EAD\
             9649E5DE8BFC7F3F924147AD3A947428",
        );
        assert_eq!(gen48(&mut r), expected);
    }

    // RFC 4503 Appendix A.2 — testing WITH IV setup (master key all zero).
    #[test]
    fn rfc_a2_iv_zero() {
        let key = [0u8; 16];
        let iv = [0u8; 8];
        let mut r = Rabbit::new(&key);
        r.set_iv(&iv);
        let expected = hex48(
            "C6A7275EF85495D87CCD5D376705B7ED\
             5F29A6AC04F5EFD47B8F293270DC4A8D\
             2ADE822B29DE6C1EE52BDB8A47BF8F66",
        );
        assert_eq!(gen48(&mut r), expected);
    }

    #[test]
    fn rfc_a2_iv_2() {
        let key = [0u8; 16];
        let iv = hex8("C373F575C1267E59");
        let mut r = Rabbit::new(&key);
        r.set_iv(&iv);
        let expected = hex48(
            "1FCD4EB9580012E2E0DCCC9222017D6D\
             A75F4E10D12125017B2499FFED936F2E\
             EBC112C393E738392356BDD012029BA7",
        );
        assert_eq!(gen48(&mut r), expected);
    }

    #[test]
    fn rfc_a2_iv_3() {
        let key = [0u8; 16];
        let iv = hex8("A6EB561AD2F41727");
        let mut r = Rabbit::new(&key);
        r.set_iv(&iv);
        let expected = hex48(
            "445AD8C805858DBF70B6AF23A151104D\
             96C8F27947F42C5BAEAE67C6ACC35B03\
             9FCBFC895FA71C17313DF034F01551CB",
        );
        assert_eq!(gen48(&mut r), expected);
    }

    // apply_keystream must be the XOR inverse of itself over a copy of the keystream.
    #[test]
    fn xor_roundtrip() {
        let key = hex16("912813292E3D36FE3BFC62F1DC51C3AC");
        let mut enc = Rabbit::new(&key);
        let mut dec = Rabbit::new(&key);
        let plain = b"Rec Room Tachyon realtime hello, dorm!";
        let mut buf = plain.to_vec();
        enc.apply_keystream(&mut buf);
        assert_ne!(&buf[..], &plain[..]);
        dec.apply_keystream(&mut buf);
        assert_eq!(&buf[..], &plain[..]);
    }

    // Streaming in byte-at-a-time chunks must equal one-shot (buffer bookkeeping).
    #[test]
    fn streaming_matches_oneshot() {
        let key = [0u8; 16];
        let mut a = Rabbit::new(&key);
        let mut b = Rabbit::new(&key);
        let mut data_a = vec![0u8; 40];
        let mut data_b = vec![0u8; 40];
        a.apply_keystream(&mut data_a);
        for chunk in data_b.chunks_mut(7) {
            b.apply_keystream(chunk);
        }
        assert_eq!(data_a, data_b);
    }

    // --- hex helpers ---
    fn hex_nib(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => panic!("bad hex"),
        }
    }
    fn hexn(s: &str, n: usize) -> Vec<u8> {
        let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        assert_eq!(clean.len(), n * 2, "expected {} hex bytes", n);
        (0..n)
            .map(|i| (hex_nib(clean[2 * i]) << 4) | hex_nib(clean[2 * i + 1]))
            .collect()
    }
    fn hex16(s: &str) -> [u8; 16] {
        hexn(s, 16).try_into().unwrap()
    }
    fn hex8(s: &str) -> [u8; 8] {
        hexn(s, 8).try_into().unwrap()
    }
    fn hex48(s: &str) -> [u8; 48] {
        hexn(s, 48).try_into().unwrap()
    }
}

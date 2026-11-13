//! The Kiwii realtime net protocol — the Rust-native, server-authoritative wire (client side).
//!
//! Authoritative spec: `rr-capture/docs/kiwii-net-protocol.md` (owned by the realtime-server session). This module
//! is the client's codec for it. Transport underneath is [`crate::lnl`] (LiteNetLib, ProtocolId 13) + the Rabbit
//! cipher; app messages ride Channeled packets on reliable channel 2 with an envelope `[msg_type: u8][body]`.
//!
//! Implemented here: the §5 ReplicationFrame (0x20) decode/encode — the locked, server→client per-tick entity-delta
//! that drives remote-avatar rendering. (Handshake (0x01/0x02 X25519), Input (0x10) and the Rabbit cipher land next,
//! validated byte-for-byte against the realtime-server's serve binary.)

/// Message-envelope type bytes (`[msg_type: u8][body]`).
pub mod msg {
    pub const CLIENT_HELLO: u8 = 0x01;
    pub const SERVER_HELLO: u8 = 0x02;
    pub const INPUT: u8 = 0x10;
    pub const REPLICATION_FRAME: u8 = 0x20;
}

/// Default world position range (metres); positions quantize over `[-POS_RANGE, POS_RANGE]`. Per-world tunable.
pub const POS_RANGE: f32 = 256.0;

// --- Handshake: device profile + ephemeral X25519 + Rabbit-key derivation (mirrors the server's netproto.rs) ------

/// A client's hard device budget (the overload/crash bound), carried in ClientHello. Derived from the engine quality
/// tier + platform; the server clamps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceProfile {
    pub max_entities: u16,
    pub max_hz: u8,
    pub max_bytes: u32,
}

impl DeviceProfile {
    /// Reference tiers (spec §4): the client sends the one matching its quality/platform.
    pub const MOBILE_LOW: DeviceProfile = DeviceProfile { max_entities: 40, max_hz: 10, max_bytes: 48_000 };
    pub const QUEST: DeviceProfile = DeviceProfile { max_entities: 80, max_hz: 20, max_bytes: 128_000 };
    pub const PC: DeviceProfile = DeviceProfile { max_entities: 250, max_hz: 30, max_bytes: 512_000 };
}

/// An ephemeral X25519 keypair (one per connection), matching the server's.
pub struct EphemeralKeypair {
    secret: x25519_dalek::EphemeralSecret,
    pub public: [u8; 32],
}

/// Generate a fresh ephemeral X25519 keypair.
pub fn new_ephemeral() -> EphemeralKeypair {
    let secret = x25519_dalek::EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let public = x25519_dalek::PublicKey::from(&secret).to_bytes();
    EphemeralKeypair { secret, public }
}

impl EphemeralKeypair {
    /// Consume this keypair to compute the X25519 shared secret with the peer's (server's) public key.
    pub fn shared(self, peer_pub: &[u8; 32]) -> [u8; 32] {
        self.secret.diffie_hellman(&x25519_dalek::PublicKey::from(*peer_pub)).to_bytes()
    }
}

/// Derive the 16-byte Rabbit key from the X25519 shared secret: `SHA256(shared || "kiwii-rabbit-v1")[0..16]`. Both
/// sides compute the same value; the per-packet IV comes from the cipher trailer (see rr_net::rabbit).
pub fn derive_rabbit_key(shared: &[u8; 32]) -> [u8; 16] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(shared);
    h.update(b"kiwii-rabbit-v1");
    let d = h.finalize();
    let mut k = [0u8; 16];
    k.copy_from_slice(&d[..16]);
    k
}

/// ClientHello (0x01) body: `[0x01][pubkey:32][maxEntities:u16 LE][maxHz:u8][maxBytes:u32 LE]`.
pub fn encode_client_hello(pubkey: &[u8; 32], p: &DeviceProfile) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + 32 + 7);
    v.push(msg::CLIENT_HELLO);
    v.extend_from_slice(pubkey);
    v.extend_from_slice(&p.max_entities.to_le_bytes());
    v.push(p.max_hz);
    v.extend_from_slice(&p.max_bytes.to_le_bytes());
    v
}

/// ServerHello (0x02) body: `[0x02][pubkey:32]` → the server's X25519 public key.
pub fn decode_server_hello(payload: &[u8]) -> Option<[u8; 32]> {
    if payload.first() != Some(&msg::SERVER_HELLO) {
        return None;
    }
    payload.get(1..33)?.try_into().ok()
}

/// Input (0x10) body: `[0x10][pos: 3×f32 LE][yaw: f32 LE]` (16-byte body). Sent each input tick, encrypted.
pub fn encode_input(pos: [f32; 3], yaw: f32) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + 16);
    v.push(msg::INPUT);
    for c in pos {
        v.extend_from_slice(&c.to_le_bytes());
    }
    v.extend_from_slice(&yaw.to_le_bytes());
    v
}

/// Parse an Input (0x10) body back to `(pos, yaw)` — for round-trip tests / a local echo.
pub fn decode_input(payload: &[u8]) -> Option<([f32; 3], f32)> {
    if payload.first() != Some(&msg::INPUT) {
        return None;
    }
    let b = &payload[1..];
    let f = |i: usize| -> Option<f32> { Some(f32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
    Some(([f(0)?, f(4)?, f(8)?], f(12)?))
}

// --- Cipher layer: the per-datagram Rabbit trailer (marker = LAST byte), matching the server's litenetlib.rs -------

/// Plaintext trailer marker (pre-key): the packet rides in the clear with `0x11` appended.
pub const TRAILER_PLAIN: u8 = 0x11;
/// Encrypted trailer marker: `[ciphertext][IV:8 in the clear][0x2a]`.
pub const TRAILER_ENCRYPTED: u8 = 0x2a;

/// Wrap a built LiteNetLib packet for the clear (pre-key) path: append the `0x11` trailer byte.
pub fn cipher_outbound_plain(packet: &[u8]) -> Vec<u8> {
    let mut v = packet.to_vec();
    v.push(TRAILER_PLAIN);
    v
}

/// Encrypt a built LiteNetLib packet with Rabbit (fresh per-packet 8-byte `iv`, random in real use) and append the
/// `[IV:8 in the clear][0x2a]` trailer. Keystream is LITTLE-ENDIAN (`apply_keystream_le`); the IV seeds the cipher
/// and is NOT itself keystreamed. Wire = `[ciphertext(packet.len())][IV:8][0x2a]`.
pub fn cipher_outbound_encrypted(packet: &[u8], key: &[u8; 16], iv: &[u8; 8]) -> Vec<u8> {
    let mut out = packet.to_vec();
    let mut r = crate::rabbit::Rabbit::new(key);
    r.set_iv(iv);
    r.apply_keystream_le(&mut out);
    out.extend_from_slice(iv);
    out.push(TRAILER_ENCRYPTED);
    out
}

/// Strip the cipher trailer from an inbound datagram, decrypting if needed. `0x11` → the leading bytes are the
/// plaintext packet; `0x2a` → the last 9 bytes are `[IV:8][0x2a]`, Rabbit-decrypt the rest with a fresh cipher.
/// Returns the plaintext LiteNetLib packet, or `None` on a bad/short/unknown-marker datagram.
pub fn cipher_inbound(wire: &[u8], key: &[u8; 16]) -> Option<Vec<u8>> {
    match *wire.last()? {
        TRAILER_PLAIN => Some(wire.get(..wire.len() - 1)?.to_vec()),
        TRAILER_ENCRYPTED => {
            let payload_len = wire.len().checked_sub(9)?;
            let iv: [u8; 8] = wire.get(payload_len..payload_len + 8)?.try_into().ok()?;
            let mut out = wire.get(..payload_len)?.to_vec();
            let mut r = crate::rabbit::Rabbit::new(key);
            r.set_iv(&iv);
            r.apply_keystream_le(&mut out);
            Some(out)
        }
        _ => None,
    }
}

/// Level of detail — drives position precision (Near=16-bit, Mid=12-bit, Far=8-bit per axis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    Near = 0,
    Mid = 1,
    Far = 2,
}

impl Lod {
    fn from_bits(b: u64) -> Lod {
        match b {
            0 => Lod::Near,
            1 => Lod::Mid,
            _ => Lod::Far,
        }
    }
    /// Bits per position axis at this LOD.
    pub fn pos_bits(self) -> u32 {
        match self {
            Lod::Near => 16,
            Lod::Mid => 12,
            Lod::Far => 8,
        }
    }
}

/// One entity's delta in a [`ReplicationFrame`]: only the fields the server sent this tick are `Some`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityDelta {
    pub id: u32,
    pub lod: Lod,
    pub pos: Option<[f32; 3]>,
    pub yaw: Option<f32>,
    pub state: Option<u16>,
}

/// A server→client replication frame (msg 0x20 body): the server tick and the per-entity deltas, in priority order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ReplicationFrame {
    pub server_tick: u32,
    pub entities: Vec<EntityDelta>,
}

/// LSB-first bit reader over a byte slice (the frame is a bit stream, not byte-aligned).
struct BitReader<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        BitReader { bytes, bit: 0 }
    }
    /// Reads `n` bits (n <= 32) LSB-first, or `None` if the stream runs out.
    fn read(&mut self, n: u32) -> Option<u64> {
        let mut out = 0u64;
        for i in 0..n {
            let byte = self.bytes.get(self.bit >> 3)?;
            let b = (byte >> (self.bit & 7)) & 1;
            out |= (b as u64) << i;
            self.bit += 1;
        }
        Some(out)
    }
}

/// LSB-first bit writer (matches [`BitReader`]); used for round-trip tests and local-echo.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit: usize,
}

impl BitWriter {
    fn write(&mut self, value: u64, n: u32) {
        for i in 0..n {
            if self.bit >> 3 == self.bytes.len() {
                self.bytes.push(0);
            }
            if (value >> i) & 1 != 0 {
                self.bytes[self.bit >> 3] |= 1 << (self.bit & 7);
            }
            self.bit += 1;
        }
    }
}

/// Dequantize an unsigned `bits`-wide value over `[-range, range]`.
fn dequant(q: u64, bits: u32, range: f32) -> f32 {
    q as f32 / ((1u64 << bits) - 1) as f32 * 2.0 * range - range
}

/// Quantize a value over `[-range, range]` to `bits` bits (clamped), the inverse of [`dequant`].
fn quant(v: f32, bits: u32, range: f32) -> u64 {
    let t = ((v + range) / (2.0 * range)).clamp(0.0, 1.0);
    (t * ((1u64 << bits) - 1) as f32).round() as u64
}

impl ReplicationFrame {
    /// Decodes a §5 ReplicationFrame body (the bytes AFTER the `0x20` envelope type). `None` if the stream is short.
    pub fn decode(body: &[u8], pos_range: f32) -> Option<ReplicationFrame> {
        let mut r = BitReader::new(body);
        let server_tick = r.read(32)? as u32;
        let count = r.read(16)? as u16;
        let mut entities = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = r.read(32)? as u32;
            let lod = Lod::from_bits(r.read(2)?);
            let flags = r.read(3)?;
            let bits = lod.pos_bits();
            let pos = if flags & 0b001 != 0 {
                Some([
                    dequant(r.read(bits)?, bits, pos_range),
                    dequant(r.read(bits)?, bits, pos_range),
                    dequant(r.read(bits)?, bits, pos_range),
                ])
            } else {
                None
            };
            let yaw = if flags & 0b010 != 0 {
                Some(r.read(12)? as f32 / ((1u32 << 12) - 1) as f32 * std::f32::consts::TAU)
            } else {
                None
            };
            let state = if flags & 0b100 != 0 { Some(r.read(16)? as u16) } else { None };
            entities.push(EntityDelta { id, lod, pos, yaw, state });
        }
        Some(ReplicationFrame { server_tick, entities })
    }

    /// Encodes to a §5 ReplicationFrame body (mirrors [`decode`]); the realtime-server owns the authoritative encoder,
    /// this is for round-trip tests + any local echo.
    pub fn encode(&self, pos_range: f32) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.write(self.server_tick as u64, 32);
        w.write(self.entities.len() as u64, 16);
        for e in &self.entities {
            w.write(e.id as u64, 32);
            w.write(e.lod as u64, 2);
            let flags = (e.pos.is_some() as u64) | ((e.yaw.is_some() as u64) << 1) | ((e.state.is_some() as u64) << 2);
            w.write(flags, 3);
            let bits = e.lod.pos_bits();
            if let Some(p) = e.pos {
                for v in p {
                    w.write(quant(v, bits, pos_range), bits);
                }
            }
            if let Some(y) = e.yaw {
                let t = y.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
                w.write((t * ((1u32 << 12) - 1) as f32).round() as u64, 12);
            }
            if let Some(s) = e.state {
                w.write(s as u64, 16);
            }
        }
        w.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trips_through_the_bit_stream() {
        let frame = ReplicationFrame {
            server_tick: 0xDEAD_BEEF,
            entities: vec![
                EntityDelta { id: 1, lod: Lod::Near, pos: Some([0.0, 1.5, -3.25]), yaw: Some(1.0), state: Some(0x1234) },
                EntityDelta { id: 7, lod: Lod::Mid, pos: Some([10.0, 0.0, 200.0]), yaw: None, state: None },
                EntityDelta { id: 42, lod: Lod::Far, pos: None, yaw: Some(5.5), state: None },
            ],
        };
        let bytes = frame.encode(POS_RANGE);
        let back = ReplicationFrame::decode(&bytes, POS_RANGE).unwrap();
        assert_eq!(back.server_tick, frame.server_tick);
        assert_eq!(back.entities.len(), 3);
        // ids, lod, flags survive exactly
        assert_eq!((back.entities[0].id, back.entities[1].id, back.entities[2].id), (1, 7, 42));
        assert_eq!(back.entities[2].lod, Lod::Far);
        assert!(back.entities[1].yaw.is_none() && back.entities[1].state.is_none());
        assert_eq!(back.entities[0].state, Some(0x1234));
        // Near (16-bit) position is accurate to ~POS_RANGE/65535 ≈ 8 mm
        let p = back.entities[0].pos.unwrap();
        for (a, b) in p.iter().zip([0.0, 1.5, -3.25]) {
            assert!((a - b).abs() < 0.01, "{a} vs {b}");
        }
        // yaw within one 12-bit step (~0.0015 rad)
        assert!((back.entities[0].yaw.unwrap() - 1.0).abs() < 0.002);
    }

    #[test]
    fn far_lod_is_coarser_than_near() {
        let near = EntityDelta { id: 1, lod: Lod::Near, pos: Some([100.0, 0.0, 0.0]), yaw: None, state: None };
        let far = EntityDelta { id: 1, lod: Lod::Far, pos: Some([100.0, 0.0, 0.0]), yaw: None, state: None };
        let f = |e: EntityDelta| {
            let fr = ReplicationFrame { server_tick: 0, entities: vec![e] };
            ReplicationFrame::decode(&fr.encode(POS_RANGE), POS_RANGE).unwrap().entities[0].pos.unwrap()[0]
        };
        let (en, ef) = (f(near), f(far));
        // both near 100 m, but Far (8-bit over ±256 m, ~2 m steps) is much coarser than Near (16-bit, ~8 mm)
        assert!((en - 100.0).abs() < 0.02, "near {en}");
        assert!((ef - 100.0).abs() < 2.1, "far {ef}");
        assert!((ef - 100.0).abs() > (en - 100.0).abs());
    }

    #[test]
    fn empty_frame_decodes() {
        let fr = ReplicationFrame { server_tick: 5, entities: vec![] };
        let back = ReplicationFrame::decode(&fr.encode(POS_RANGE), POS_RANGE).unwrap();
        assert_eq!(back, fr);
    }

    #[test]
    fn x25519_both_sides_derive_the_same_rabbit_key() {
        let client = new_ephemeral();
        let server = new_ephemeral();
        let (cpub, spub) = (client.public, server.public);
        let cs = client.shared(&spub);
        let ss = server.shared(&cpub);
        assert_eq!(cs, ss, "ECDH shared secret must match on both sides");
        assert_eq!(derive_rabbit_key(&cs), derive_rabbit_key(&ss));
    }

    #[test]
    fn client_hello_matches_the_server_layout() {
        let pk = [7u8; 32];
        let h = encode_client_hello(&pk, &DeviceProfile::PC);
        assert_eq!(h[0], msg::CLIENT_HELLO);
        assert_eq!(&h[1..33], &pk);
        assert_eq!(u16::from_le_bytes([h[33], h[34]]), 250);
        assert_eq!(h[35], 30);
        assert_eq!(u32::from_le_bytes([h[36], h[37], h[38], h[39]]), 512_000);
        assert_eq!(h.len(), 40);
    }

    #[test]
    fn server_hello_and_input_round_trip() {
        let pub_bytes = new_ephemeral().public;
        let mut sh = vec![msg::SERVER_HELLO];
        sh.extend_from_slice(&pub_bytes);
        assert_eq!(decode_server_hello(&sh), Some(pub_bytes));

        let inp = encode_input([1.0, -2.5, 3.0], 1.25);
        assert_eq!(inp[0], msg::INPUT);
        assert_eq!(inp.len(), 17);
        assert_eq!(decode_input(&inp), Some(([1.0, -2.5, 3.0], 1.25)));
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    /// Byte-exact interop against the realtime server's `cipher_interop_vector` (its live code = source of truth).
    #[test]
    fn cipher_layer_matches_the_server_interop_vector() {
        let key = [0x42u8; 16];
        let iv = [0x01u8; 8];

        // 1. The little-endian keystream (first 16 bytes) must match the server's.
        let mut ks = [0u8; 16];
        let mut r = crate::rabbit::Rabbit::new(&key);
        r.set_iv(&iv);
        r.apply_keystream_le(&mut ks);
        assert_eq!(ks.to_vec(), hex("480a84ceb4778f0c3af93815b5545bfd"), "LE keystream drift");

        // 2. The full encrypted datagram (ciphertext + [IV:8 clear][0x2a]) must match byte-for-byte.
        let mut body = vec![0x01u8];
        body.extend_from_slice(b"kiwii-interop");
        let wire = cipher_outbound_encrypted(&body, &key, &iv);
        assert_eq!(wire, hex("4961edb9dd1ea265548d5d67da2401010101010101012a"), "cipher wire drift");

        // 3. Inbound strips + decrypts back to the plaintext body.
        assert_eq!(cipher_inbound(&wire, &key), Some(body));
    }

    #[test]
    fn cipher_plain_and_encrypted_round_trip() {
        let key = [0x11u8; 16];
        let pkt = b"a channeled packet body".to_vec();
        // plaintext path
        let p = cipher_outbound_plain(&pkt);
        assert_eq!(*p.last().unwrap(), TRAILER_PLAIN);
        assert_eq!(cipher_inbound(&p, &key), Some(pkt.clone()));
        // encrypted path with a distinct IV
        let e = cipher_outbound_encrypted(&pkt, &key, &[0x09; 8]);
        assert_ne!(&e[..pkt.len()], &pkt[..]);
        assert_eq!(cipher_inbound(&e, &key), Some(pkt));
    }
}

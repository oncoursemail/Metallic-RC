//! Avatar movement (realtime.md section 6): event 28 carries, 15 times a second, one bit-packed record per synced
//! object. A screen-mode player sends PlayerAvatarData: tracking space (the feet), head and (ours) the two hands.
//!
//! Field order follows the spec; bit order (least significant first), the 10-bit ranges and the record header are our
//! reading (open item 4) and are checked against the stock client later.
use crate::events::{In, Malformed, Out};

/// Seconds between movement events.
pub const SEND_INTERVAL: f64 = 1.0 / 15.0;
/// `NetworkSynchronizationType::PlayerAvatarData`
pub const PLAYER_AVATAR_DATA: u8 = 1;
/// The largest quaternion component left out of smallest-three lies above this, so the other three lie within +-it.
const SMALLEST_THREE: f32 = std::f32::consts::FRAC_1_SQRT_2;
const VELOCITY_RANGES: [f32; 3] = [1.0, 8.0, 64.0];
/// how far past the last sample a remote avatar is carried forward, seconds
const MAX_EXTRAPOLATE: f64 = 0.25;
/// how fast the shown pose closes on the target, per second
const EASE: f64 = 12.0;

#[derive(Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    bits: usize,
}

impl BitWriter {
    pub fn bits(&mut self, v: u32, n: usize) {
        for i in 0..n {
            if self.bits % 8 == 0 {
                self.bytes.push(0);
            }
            if v >> i & 1 != 0 {
                *self.bytes.last_mut().unwrap() |= 1 << (self.bits % 8);
            }
            self.bits += 1;
        }
    }
    pub fn bit(&mut self, v: bool) {
        self.bits(v as u32, 1);
    }
    pub fn f32(&mut self, v: f32) {
        self.bits(v.to_bits(), 32);
    }
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

pub struct BitReader<'a> {
    bytes: &'a [u8],
    bits: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        BitReader { bytes, bits: 0 }
    }
    pub fn bits(&mut self, n: usize) -> Result<u32, Malformed> {
        let mut v = 0;
        for i in 0..n {
            let byte = self.bytes.get(self.bits / 8).ok_or(Malformed)?;
            v |= ((byte >> (self.bits % 8) & 1) as u32) << i;
            self.bits += 1;
        }
        Ok(v)
    }
    pub fn bit(&mut self) -> Result<bool, Malformed> {
        Ok(self.bits(1)? != 0)
    }
    pub fn f32(&mut self) -> Result<f32, Malformed> {
        Ok(f32::from_bits(self.bits(32)?))
    }
}

fn quantise(v: f32, range: f32) -> u32 {
    (((v.clamp(-range, range) + range) / (2.0 * range)) * 1023.0).round() as u32
}

fn dequantise(q: u32, range: f32) -> f32 {
    q as f32 / 1023.0 * 2.0 * range - range
}

/// One synced transform: world position, rotation (x, y, z, w) and velocity in m/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
}

impl Default for Pose {
    fn default() -> Self {
        Pose { position: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], velocity: [0.0; 3] }
    }
}

impl Pose {
    fn write(&self, w: &mut BitWriter) {
        let sleeping = self.velocity.iter().all(|v| v.abs() < 1e-3);
        w.bit(false); // redundant resend
        w.bit(sleeping);
        w.bit(true); // compressed rotation
        w.bits(0, 6); // continuous movement id
        let q = self.rotation;
        let identity = q[3].abs() > 0.99999;
        w.bit(identity);
        if !identity {
            let big = (0..4).max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs())).unwrap();
            let sign = if q[big] < 0.0 { -1.0 } else { 1.0 };
            w.bits(big as u32, 2);
            for i in (0..4).filter(|&i| i != big) {
                w.bits(quantise(q[i] * sign, SMALLEST_THREE), 10);
            }
        }
        w.bit(false); // no scale
        if !sleeping {
            let top = self.velocity.iter().fold(0f32, |m, v| m.max(v.abs()));
            let range = VELOCITY_RANGES.iter().position(|&r| top <= r).unwrap_or(2);
            w.bits(range as u32, 2);
            for v in self.velocity {
                w.bits(quantise(v, VELOCITY_RANGES[range]), 10);
            }
        }
        w.bit(false); // position not compressed
        self.position.iter().for_each(|&v| w.f32(v));
    }

    fn read(r: &mut BitReader) -> Result<Pose, Malformed> {
        let redundant = r.bit()?;
        let sleeping = r.bit()?;
        let compressed = r.bit()?;
        r.bits(6)?;
        let rotation = if !compressed {
            [r.f32()?, r.f32()?, r.f32()?, r.f32()?]
        } else if r.bit()? {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            let big = r.bits(2)? as usize;
            let mut q = [0.0; 4];
            for i in (0..4).filter(|&i| i != big) {
                q[i] = dequantise(r.bits(10)?, SMALLEST_THREE);
            }
            q[big] = (1.0 - q.iter().map(|v| v * v).sum::<f32>()).max(0.0).sqrt();
            q
        };
        if r.bit()? {
            // scale is not part of an avatar pose
            return Err(Malformed);
        }
        let mut velocity = [0.0; 3];
        if !redundant && !sleeping {
            let range = *VELOCITY_RANGES.get(r.bits(2)? as usize).ok_or(Malformed)?;
            for v in &mut velocity {
                *v = dequantise(r.bits(10)?, range);
            }
        }
        if r.bit()? {
            return Err(Malformed);
        }
        Ok(Pose { position: [r.f32()?, r.f32()?, r.f32()?], rotation, velocity })
    }
}

/// PlayerAvatarData: tracking space, left hand, right hand, head, each sent or not; then the four providers (hand
/// poses, face, movement), none of which a screen player sends yet.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct AvatarData {
    pub tracking_space: Option<Pose>,
    pub left_hand: Option<Pose>,
    pub right_hand: Option<Pose>,
    pub head: Option<Pose>,
}

impl AvatarData {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = BitWriter::default();
        for p in [self.tracking_space, self.left_hand, self.right_hand, self.head] {
            w.bit(p.is_some());
            if let Some(p) = p {
                p.write(&mut w);
            }
        }
        w.bits(0, 4);
        w.finish()
    }

    pub fn decode(b: &[u8]) -> Result<AvatarData, Malformed> {
        let mut r = BitReader::new(b);
        let mut parts = [None; 4];
        for p in &mut parts {
            if r.bit()? {
                *p = Some(Pose::read(&mut r)?);
            }
        }
        if r.bits(4)? != 0 {
            return Err(Malformed);
        }
        let [tracking_space, left_hand, right_hand, head] = parts;
        Ok(AvatarData { tracking_space, left_hand, right_hand, head })
    }
}

/// The content of one event 28: server tick, sender actor, then `(view id, sync type, record)` entries, each record
/// prefixed with its byte length.
pub fn write_buffer(tick: i32, actor: i32, records: &[(i32, u8, Vec<u8>)]) -> Vec<u8> {
    let mut o = Out::default();
    o.i32(tick);
    o.i32(actor);
    for (view, kind, bytes) in records {
        o.i32(*view);
        o.u8(*kind);
        o.0.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        o.0.extend_from_slice(bytes);
    }
    o.0
}

pub type Records<'a> = Vec<(i32, u8, &'a [u8])>;

pub fn read_buffer(b: &[u8]) -> Result<(i32, i32, Records<'_>), Malformed> {
    let mut r = In::new(b);
    let (tick, actor) = (r.i32()?, r.i32()?);
    let mut records = vec![];
    while r.at < b.len() {
        let (view, kind) = (r.i32()?, r.u8()?);
        let n = u16::from_le_bytes([r.u8()?, r.u8()?]) as usize;
        records.push((view, kind, r.take(n)?));
    }
    Ok((tick, actor, records))
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Normalised lerp along the shorter way round.
fn nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let s = if a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>() < 0.0 { -1.0 } else { 1.0 };
    let q: Vec<f32> = (0..4).map(|i| a[i] + (b[i] * s - a[i]) * t).collect();
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

/// A remote transform as shown: the newest sample carried forward by its velocity, eased toward from where it was
/// last shown, so late or lost packets do not make it jump.
#[derive(Clone, Debug, Default)]
pub struct Smoother {
    last: Option<(f64, Pose)>,
    shown: Option<Pose>,
}

impl Smoother {
    /// A sample sent at server time `sent`; older than the newest one already held, it is dropped.
    pub fn push(&mut self, sent: f64, pose: Pose) {
        if self.last.is_none_or(|(t, _)| sent >= t) {
            self.last = Some((sent, pose));
        }
    }

    /// The pose to draw at server time `now`, `dt` seconds after the previous call.
    pub fn step(&mut self, now: f64, dt: f64) -> Option<Pose> {
        let (sent, p) = self.last?;
        let ahead = (now - sent).clamp(0.0, MAX_EXTRAPOLATE) as f32;
        let target = Pose { position: [0, 1, 2].map(|i| p.position[i] + p.velocity[i] * ahead), ..p };
        let k = (1.0 - (-dt * EASE).exp()) as f32;
        let shown = match self.shown {
            Some(s) => Pose { position: lerp3(s.position, target.position, k), rotation: nlerp(s.rotation, target.rotation, k), velocity: p.velocity },
            None => target,
        };
        self.shown = Some(shown);
        Some(shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32], eps: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= eps)
    }

    #[test]
    fn bits_round_trip() {
        let mut w = BitWriter::default();
        w.bits(5, 3);
        w.bit(true);
        w.bits(1023, 10);
        w.f32(-2.5);
        let b = w.finish();
        let mut r = BitReader::new(&b);
        assert_eq!((r.bits(3), r.bit(), r.bits(10), r.f32()), (Ok(5), Ok(true), Ok(1023), Ok(-2.5)));
    }

    #[test]
    fn avatar_data_round_trips_within_quantisation() {
        let yaw = 2.5f32;
        let pitch = -0.4f32;
        // yaw about y, then pitch about x
        let (sy, cy, sp, cp) = ((yaw / 2.0).sin(), (yaw / 2.0).cos(), (pitch / 2.0).sin(), (pitch / 2.0).cos());
        let head_q = [cy * sp, sy * cp, -sy * sp, cy * cp];
        let a = AvatarData {
            tracking_space: Some(Pose { position: [12.25, 0.0, -3.5], rotation: [0.0, sy, 0.0, cy], velocity: [2.9, 0.0, -1.2] }),
            left_hand: None,
            right_hand: None,
            head: Some(Pose { position: [12.25, 1.55, -3.5], rotation: head_q, velocity: [0.0; 3] }),
        };
        let back = AvatarData::decode(&a.encode()).unwrap();
        let (t, h) = (back.tracking_space.unwrap(), back.head.unwrap());
        assert_eq!(t.position, [12.25, 0.0, -3.5]);
        assert!(close(&t.rotation, &[0.0, sy, 0.0, cy], 0.002), "{:?}", t.rotation);
        assert!(close(&t.velocity, &[2.9, 0.0, -1.2], 0.01), "{:?}", t.velocity);
        assert!(close(&h.rotation, &head_q, 0.002), "{:?}", h.rotation);
        assert_eq!(h.velocity, [0.0; 3]);
        assert!(back.left_hand.is_none() && back.right_hand.is_none());

        let buf = write_buffer(900, 2, &[(200001, PLAYER_AVATAR_DATA, a.encode())]);
        let (tick, actor, records) = read_buffer(&buf).unwrap();
        assert_eq!((tick, actor, records.len(), records[0].0, records[0].1), (900, 2, 1, 200001, PLAYER_AVATAR_DATA));
        assert_eq!(AvatarData::decode(records[0].2).unwrap(), back);
    }

    #[test]
    fn smoother_extrapolates_then_settles() {
        let mut s = Smoother::default();
        assert!(s.step(0.0, 0.016).is_none());
        s.push(1.0, Pose { position: [0.0; 3], velocity: [2.0, 0.0, 0.0], ..Default::default() });
        // 0.1 s after the sample: carried forward 0.2 m
        assert!(close(&s.step(1.1, 0.016).unwrap().position, &[0.2, 0.0, 0.0], 1e-5));
        // a stale sample is ignored
        s.push(0.5, Pose { position: [9.0; 3], ..Default::default() });
        // the avatar stops at 1 m: the shown pose closes on it
        s.push(1.5, Pose { position: [1.0, 0.0, 0.0], ..Default::default() });
        let mut p = Pose::default();
        for i in 0..60 {
            p = s.step(1.5 + i as f64 * 0.016, 0.016).unwrap();
        }
        assert!(close(&p.position, &[1.0, 0.0, 0.0], 0.01), "{:?}", p.position);
    }
}

//! Room audio: the game's AudioClips decoded from the bundles (FMOD sound banks, FSB5, inside each clip's
//! `.resource`), and AudioSources played in the room.
//!
//! FSB5 layout (public knowledge, as read by vgmstream and python-fsb5): a 60-byte header (64 for version 0),
//! one 64-bit sample header per sample (+ chunks), a name table, then the sample data.
use std::{path::Path, sync::Arc, time::Duration};

use bevy::{
    audio::{ChannelCount, Decodable, PlaybackMode, Sample, SampleRate, Source, SpatialScale, Volume},
    prelude::*,
};
use unity_assets::Value;

use crate::{room::{value, Library, RoomSound}, settings::Settings, world::{self, Location}};

/// A decoded clip: interleaved 16-bit samples.
#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    /// the FSB5 codec, for reports
    pub codec: &'static str,
}

fn codec_name(mode: u32) -> &'static str {
    match mode {
        1 => "pcm8",
        2 => "pcm16",
        3 => "pcm24",
        4 => "pcm32",
        5 => "pcmfloat",
        6 => "gcadpcm",
        7 => "imaadpcm",
        11 => "mpeg",
        15 => "vorbis",
        16 => "fadpcm",
        17 => "opus",
        _ => "other",
    }
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| "fsb5: truncated".to_string())
}

/// One sample of an FSB5 bank.
pub struct FsbSample<'a> {
    pub rate: u32,
    pub channels: u16,
    pub frames: u32,
    pub data: &'a [u8],
    /// `VORBISDATA` chunk: the CRC32 of the stripped Vorbis setup header
    pub vorbis_crc: Option<u32>,
}

/// Parses an FSB5 bank: the codec and its samples.
pub fn parse_fsb5(b: &[u8]) -> Result<(u32, Vec<FsbSample<'_>>), String> {
    if b.get(..4) != Some(b"FSB5") {
        return Err("not an FSB5 bank".into());
    }
    let version = u32_at(b, 4)?;
    let (count, headers_size, names_size, data_size, mode) = (u32_at(b, 8)?, u32_at(b, 12)?, u32_at(b, 16)?, u32_at(b, 20)?, u32_at(b, 24)?);
    let header = if version == 0 { 64 } else { 60 };
    let data_start = header + headers_size as usize + names_size as usize;
    let data = b.get(data_start..data_start + data_size as usize).ok_or("fsb5: data past the end")?;
    let mut at = header;
    let mut raw = Vec::new();
    for _ in 0..count {
        let h = u64::from_le_bytes(b.get(at..at + 8).ok_or("fsb5: truncated")?.try_into().unwrap());
        at += 8;
        let mut rate = match (h >> 1) & 0xf {
            1 => 8000, 2 => 11000, 3 => 11025, 4 => 16000, 5 => 22050, 6 => 24000, 7 => 32000, 8 => 44100, 9 => 48000, _ => 44100,
        };
        let mut channels = ((h >> 5) & 1) as u16 + 1;
        let offset = ((h >> 6) & 0x0fff_ffff) as usize * 16;
        let frames = ((h >> 34) & 0x3fff_ffff) as u32;
        let mut vorbis_crc = None;
        let mut more = h & 1 != 0;
        while more {
            let c = u32_at(b, at)?;
            more = c & 1 != 0;
            let (size, kind) = (((c >> 1) & 0x00ff_ffff) as usize, (c >> 25) & 0x7f);
            let body = b.get(at + 4..at + 4 + size).ok_or("fsb5: chunk past the end")?;
            match kind {
                1 => channels = *body.first().unwrap_or(&1) as u16,
                2 => rate = u32_at(body, 0)?,
                11 => vorbis_crc = Some(u32_at(body, 0)?),
                _ => {}
            }
            at += 4 + size;
        }
        raw.push((rate, channels, frames, offset, vorbis_crc));
    }
    let mut out = Vec::new();
    for (i, &(rate, channels, frames, offset, vorbis_crc)) in raw.iter().enumerate() {
        let end = raw.get(i + 1).map_or(data.len(), |n| n.3).min(data.len());
        out.push(FsbSample { rate, channels, frames, data: data.get(offset..end).ok_or("fsb5: sample past the end")?, vorbis_crc });
    }
    Ok((mode, out))
}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143,
    157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411,
    1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442,
    11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn ima_nibble(n: u8, pred: &mut i32, step: &mut i32) -> i16 {
    let s = IMA_STEPS[*step as usize];
    let mut diff = s >> 3;
    if n & 4 != 0 { diff += s; }
    if n & 2 != 0 { diff += s >> 1; }
    if n & 1 != 0 { diff += s >> 2; }
    *pred = if n & 8 != 0 { *pred - diff } else { *pred + diff }.clamp(-32768, 32767);
    *step = (*step + IMA_INDEX[n as usize]).clamp(0, 88);
    *pred as i16
}

/// FMOD's IMA ADPCM (Xbox layout): blocks of 36 bytes per channel, each a 4-byte header (first sample, step index)
/// and 64 nibbles; channels interleave in 4-byte words after their headers.
pub fn decode_ima(data: &[u8], channels: usize, frames: usize) -> Vec<i16> {
    let block = 36 * channels;
    let mut out = Vec::with_capacity(frames * channels);
    for b in data.chunks_exact(block) {
        let mut chans: Vec<Vec<i16>> = vec![Vec::with_capacity(65); channels];
        let mut state: Vec<(i32, i32)> = (0..channels).map(|c| {
            let h = &b[c * 4..c * 4 + 4];
            (i16::from_le_bytes([h[0], h[1]]) as i32, (h[2] as i32).clamp(0, 88))
        }).collect();
        for (c, s) in state.iter().enumerate() {
            chans[c].push(s.0 as i16);
        }
        let body = &b[4 * channels..];
        for (w, word) in body.chunks_exact(4).enumerate() {
            let c = w % channels;
            let (pred, step) = &mut state[c];
            for &byte in word {
                chans[c].push(ima_nibble(byte & 0xf, pred, step));
                chans[c].push(ima_nibble(byte >> 4, pred, step));
            }
        }
        for i in 0..chans[0].len() {
            for ch in &chans {
                out.push(*ch.get(i).unwrap_or(&0));
            }
        }
    }
    out.truncate(frames * channels);
    out
}

/// Decodes one FSB5 sample to 16-bit PCM.
pub fn decode(mode: u32, s: &FsbSample) -> Result<Vec<i16>, String> {
    let ch = s.channels.max(1) as usize;
    let n = s.frames as usize * ch;
    Ok(match mode {
        1 => s.data.iter().take(n).map(|&b| ((b as i16) - 128) << 8).collect(),
        2 => s.data.chunks_exact(2).take(n).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
        5 => s.data.chunks_exact(4).take(n).map(|c| (f32::from_le_bytes(c.try_into().unwrap()).clamp(-1.0, 1.0) * 32767.0) as i16).collect(),
        7 => decode_ima(s.data, ch, s.frames as usize),
        15 => return Err(format!("vorbis: setup header {:08x} not available", s.vorbis_crc.unwrap_or(0))),
        m => return Err(format!("{}: codec not decoded", codec_name(m))),
    })
}

/// An AudioClip object's value (its `.resource` read from `bundle`) decoded.
/// The FMOD Vorbis setup codebooks, extracted once from the user's own UnityPlayer.dll into a local (gitignored)
/// `vorbis_setups.bin` and pointed at by `RR_VORBIS_DIR`. Loaded once; `None` (→ Vorbis clips stay silent) when the env
/// var is unset or the file is missing, exactly the pre-existing behaviour for music/VO.
fn vorbis_setups() -> Option<&'static unity_assets::VorbisSetups> {
    static SETUPS: std::sync::OnceLock<Option<unity_assets::VorbisSetups>> = std::sync::OnceLock::new();
    SETUPS
        .get_or_init(|| {
            let dir = std::env::var("RR_VORBIS_DIR").ok()?;
            unity_assets::VorbisSetups::load(std::path::Path::new(&dir)).ok()
        })
        .as_ref()
}

pub fn clip_from_value(v: &Value, bundle: &unity_assets::Bundle) -> Result<Clip, String> {
    let name = v.get("m_Name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let res = v.get("m_Resource").ok_or("no m_Resource")?;
    let source = res.get("m_Source").and_then(|x| x.as_str()).unwrap_or_default();
    let offset = res.get("m_Offset").and_then(|x| x.as_u64()).unwrap_or(0);
    let size = res.get("m_Size").and_then(|x| x.as_u64()).unwrap_or(0);
    let node = bundle.node(source).ok_or_else(|| format!("resource {source:?} not in bundle"))?;
    let bytes = bundle.read_node_range(node, offset, size).map_err(|e| e.to_string())?;
    let (mode, samples) = parse_fsb5(&bytes)?;
    let s = samples.first().ok_or("fsb5: no samples")?;
    // PCM/IMA are decoded in-crate; Vorbis (music/VO) goes through unity_assets' full FSB5 decoder with the FMOD setup
    // codebooks (its headers are stripped and looked up by id), so rooms play real music, not just SFX.
    if mode == 15 {
        let channels = v.get("m_Channels").and_then(|x| x.as_i64()).unwrap_or(s.channels as i64) as u32;
        let frequency = v.get("m_Frequency").and_then(|x| x.as_i64()).unwrap_or(s.rate as i64) as u32;
        let data = unity_assets::audio::decode_fsb5(&bytes, channels, frequency, None, vorbis_setups()).map_err(|e| e.to_string())?;
        return Ok(Clip { name, rate: data.sample_rate, channels: data.channels as u16, samples: data.samples, codec: codec_name(mode) });
    }
    let pcm = decode(mode, s)?;
    Ok(Clip { name, rate: s.rate, channels: s.channels, samples: pcm, codec: codec_name(mode) })
}

/// The clip at a catalog location.
pub fn load_clip(lib: &mut Library, dir: &Path, loc: &Location) -> Result<Clip, String> {
    let (f, id) = world::locate(lib, dir, loc)?;
    let v = value(&f, id)?;
    clip_from_value(&v, &f.open.bundle)
}

/// A decoded clip as a Bevy audio asset (Bevy's own `AudioSource` wants a file format; this plays samples as they
/// are, so no extra decoder features are needed).
#[derive(Asset, TypePath, Clone)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub samples: Arc<[f32]>,
}

impl Pcm {
    pub fn new(c: &Clip) -> Pcm {
        Pcm { rate: c.rate.max(1), channels: c.channels.max(1), samples: c.samples.iter().map(|&s| s as f32 / 32768.0).collect() }
    }
}

pub struct PcmPlayer {
    pcm: Pcm,
    at: usize,
}

impl Iterator for PcmPlayer {
    type Item = Sample;
    fn next(&mut self) -> Option<Sample> {
        let s = self.pcm.samples.get(self.at).copied();
        self.at += 1;
        s
    }
}

impl Source for PcmPlayer {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.pcm.samples.len().saturating_sub(self.at))
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.pcm.channels).unwrap_or(ChannelCount::MIN)
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.pcm.rate).unwrap_or(SampleRate::MIN)
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.pcm.samples.len() as f64 / self.pcm.channels as f64 / self.pcm.rate as f64))
    }
}

impl Decodable for Pcm {
    type Decoder = PcmPlayer;
    fn decoder(&self) -> PcmPlayer {
        PcmPlayer { pcm: self.clone(), at: 0 }
    }
}

/// The player's volumes, 0..1, from the client's settings (`volume_master`, `volume_music`, `volume_sfx`).
/// INFERRED interim: the settings live in main.rs's private `Prefs`, so this re-reads `settings.json` when its date
/// changes (the client saves it on every change). Once the UI lane's settings resource is merged, set this from it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Volumes {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
}

impl Default for Volumes {
    fn default() -> Self {
        Volumes { master: 1.0, music: 1.0, sfx: 1.0 }
    }
}

impl Volumes {
    pub fn from_settings(s: &Settings) -> Volumes {
        let v = |k: &str| (s.get(k) as f32 / 100.0).clamp(0.0, 1.0);
        Volumes { master: v("volume_master"), music: v("volume_music"), sfx: v("volume_sfx") }
    }
}

/// A playing room sound; its sink volume follows the listener's distance and the player's volumes.
#[derive(Component)]
pub struct RoomAudio {
    pub sound: RoomSound,
    /// a 2D loop (ambience, radio music) counts as music, everything else as effects (INFERRED: the game routes
    /// sources through mixer groups we do not read)
    pub music: bool,
}

/// The listener: the camera, ears 0.2 m apart.
pub fn listener() -> SpatialListener {
    SpatialListener::new(0.2)
}

/// How a room sound starts: Bevy's own distance falloff (1/d^2 past 1 unit) is scaled away (positions x 0.01, so it
/// stays 1 within 100 m) and Unity's rolloff is applied by `follow_listener` instead; Bevy keeps the left/right pan.
pub fn playback(s: &RoomSound) -> PlaybackSettings {
    PlaybackSettings {
        mode: if s.looped { PlaybackMode::Loop } else { PlaybackMode::Once },
        volume: Volume::Linear(0.0),
        speed: s.pitch.max(0.01),
        spatial: s.spatial >= 0.5,
        spatial_scale: Some(SpatialScale::new(0.01)),
        ..default()
    }
}

/// Sets each room sound's volume: its own, times Unity's rolloff at the listener's distance, times the player's.
pub fn follow_listener(
    volumes: Res<Volumes>,
    listeners: Query<&GlobalTransform, With<SpatialListener>>,
    mut sounds: Query<(&RoomAudio, &GlobalTransform, Option<&mut AudioSink>, Option<&mut SpatialAudioSink>)>,
) {
    let ear = listeners.iter().next().map(|t| t.translation());
    for (a, at, sink, spatial) in &mut sounds {
        let d = ear.map_or(0.0, |e| e.distance(at.translation()));
        let group = if a.music { volumes.music } else { volumes.sfx };
        let v = Volume::Linear(a.sound.volume * a.sound.gain_at(d) * volumes.master * group);
        if let Some(mut s) = sink {
            s.set_volume(v);
        }
        if let Some(mut s) = spatial {
            s.set_volume(v);
        }
    }
}

/// Logs each room sound when Bevy has started it (its sink exists).
pub fn log_started(started: Query<(&RoomAudio, Option<&AudioSink>, Option<&SpatialAudioSink>), Or<(Added<AudioSink>, Added<SpatialAudioSink>)>>) {
    for (a, sink, spatial) in &started {
        let v = sink.map(|s| s.volume()).or(spatial.map(|s| s.volume()));
        println!("sound {} playing ({}, {}, volume {v:?})", a.sound.clip.name, if spatial.is_some() { "3D" } else { "2D" }, if a.music { "music" } else { "sfx" });
    }
}

/// Re-reads the volumes when the settings file changes (checked once a second).
pub fn watch_settings(time: Res<Time>, mut volumes: ResMut<Volumes>, mut last: Local<(f32, Option<std::time::SystemTime>)>) {
    last.0 -= time.delta_secs();
    if last.0 > 0.0 {
        return;
    }
    last.0 = 1.0;
    let Some(path) = Settings::path() else { return };
    let date = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    if date.is_some() && date != last.1 {
        last.1 = date;
        let v = Volumes::from_settings(&Settings::load(&path));
        if *volumes != v {
            *volumes = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pcm16_bank_decodes() {
        let mut b = b"FSB5".to_vec();
        for v in [1u32, 1, 8, 0, 8, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.resize(60, 0);
        // 44100 Hz (index 8), mono, data offset 0, 4 frames
        b.extend_from_slice(&((8u64 << 1) | (4u64 << 34)).to_le_bytes());
        for s in [0i16, 1000, -1000, 32767] {
            b.extend_from_slice(&s.to_le_bytes());
        }
        let (mode, samples) = parse_fsb5(&b).unwrap();
        assert_eq!((mode, samples.len(), samples[0].rate, samples[0].channels, samples[0].frames), (2, 1, 44100, 1, 4));
        assert_eq!(decode(mode, &samples[0]).unwrap(), [0, 1000, -1000, 32767]);
    }

    #[test]
    fn an_ima_block_starts_at_its_header_sample() {
        // one mono block: header sample 500, step 0, then 64 nibbles of 7 (the largest step up): it rises to the top and stays
        let mut b = vec![0x77u8; 36];
        b[..4].copy_from_slice(&[0xf4, 0x01, 0, 0]);
        let out = decode_ima(&b, 1, 65);
        assert_eq!(out.len(), 65);
        assert_eq!(out[..2], [500, 511]);
        assert!(out.windows(2).all(|w| w[1] >= w[0]) && out[64] == 32767, "{out:?}");
    }
}

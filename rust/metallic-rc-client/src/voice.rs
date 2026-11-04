//! Voice chat: the microphone (cpal) is cut into 20 ms frames at 48 kHz mono, Opus-encoded (VOIP, 24 kb/s, in-band
//! FEC, as Dissonance sets it up) and sent to the room; every other player's frames are decoded into a small jitter
//! buffer each and played back placed at that player's head (distance fall-off and left/right balance).
//!
//! Settings (`settings.json`, read again when it changes): `voice_open_mic` (1, else push-to-talk on T),
//! `voice_mute` (0), `volume_voice` (100) and `volume_master`. `METALLIC_VOICE_TONE=<hz>` sends a tone instead of
//! the microphone (tests without one). Packets: `rr_net::voice` (Dissonance VoiceData over the room connection).
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rr_net::voice::{FRAME, SAMPLE_RATE};

use crate::netobj::RoomLink;
use crate::settings::Settings;

/// Samples buffered before a speaker starts playing, and the most kept (older ones are dropped).
const START: usize = FRAME * 3;
const MAX_BUFFERED: usize = FRAME * 15;
/// Open-mic gate: level (RMS) that opens it, and how long it stays open after the voice drops.
const GATE: f32 = 0.01;
const HANGOVER: f32 = 0.3;
/// Full volume up to this distance (m); half at twice it; silent beyond `FAR`.
const NEAR: f32 = 1.5;
const FAR: f32 = 40.0;
const PUSH_TO_TALK: KeyCode = KeyCode::KeyT;

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start).add_systems(Update, voice);
    }
}

/// An Opus encoder or decoder (the pure-Rust libopus port), freed on drop.
pub struct Opus {
    enc: *mut unsafe_libopus::OpusEncoder,
    dec: *mut unsafe_libopus::OpusDecoder,
}

// the codec state is plain memory owned by this value
unsafe impl Send for Opus {}
unsafe impl Sync for Opus {}

impl Opus {
    pub fn encoder() -> Opus {
        use unsafe_libopus::*;
        let mut err = 0;
        unsafe {
            let enc = opus_encoder_create(SAMPLE_RATE as i32, 1, OPUS_APPLICATION_VOIP, &mut err);
            assert!(!enc.is_null(), "opus encoder: {err}");
            opus_encoder_ctl!(enc, OPUS_SET_BITRATE_REQUEST, 24_000);
            opus_encoder_ctl!(enc, OPUS_SET_INBAND_FEC_REQUEST, 1);
            opus_encoder_ctl!(enc, OPUS_SET_PACKET_LOSS_PERC_REQUEST, 10);
            Opus { enc, dec: std::ptr::null_mut() }
        }
    }

    pub fn decoder() -> Opus {
        let mut err = 0;
        let dec = unsafe { unsafe_libopus::opus_decoder_create(SAMPLE_RATE as i32, 1, &mut err) };
        assert!(!dec.is_null(), "opus decoder: {err}");
        Opus { enc: std::ptr::null_mut(), dec }
    }

    /// One 20 ms frame in, one Opus packet out.
    pub fn encode(&mut self, pcm: &[f32]) -> Vec<u8> {
        let pcm: Vec<i16> = pcm.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
        let mut out = [0u8; 400];
        let n = unsafe { unsafe_libopus::opus_encode(self.enc, pcm.as_ptr(), FRAME as i32, out.as_mut_ptr(), out.len() as i32) };
        out[..n.max(0) as usize].to_vec()
    }

    /// Decodes a packet; `None` conceals a lost frame; `fec` takes the frame before `packet` from its redundancy.
    pub fn decode(&mut self, packet: Option<&[u8]>, fec: bool) -> Vec<f32> {
        let mut pcm = [0i16; FRAME];
        let (ptr, len) = packet.map_or((std::ptr::null(), 0), |p| (p.as_ptr(), p.len() as i32));
        let n = unsafe { unsafe_libopus::opus_decode(self.dec, ptr, len, pcm.as_mut_ptr(), FRAME as i32, fec as i32) };
        pcm[..n.max(0) as usize].iter().map(|&s| s as f32 / 32768.0).collect()
    }
}

impl Drop for Opus {
    fn drop(&mut self) {
        unsafe {
            if !self.enc.is_null() {
                unsafe_libopus::opus_encoder_destroy(self.enc);
            }
            if !self.dec.is_null() {
                unsafe_libopus::opus_decoder_destroy(self.dec);
            }
        }
    }
}

/// Linear resampling of a stream, chunk by chunk.
struct Resample {
    step: f64,
    t: f64,
    last: f32,
}

impl Resample {
    fn new(from: u32, to: u32) -> Self {
        Resample { step: from as f64 / to as f64, t: 0.0, last: 0.0 }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = input.len() as f64;
        let at = |i: isize| if i < 0 { self.last } else { input[i as usize] };
        while self.t < n - 1.0 {
            let k = self.t.floor() as isize;
            let (a, b) = (at(k), at(k + 1));
            out.push(a + (b - a) * (self.t - k as f64) as f32);
            self.t += self.step;
        }
        if let Some(&l) = input.last() {
            self.t -= n;
            self.last = l;
        }
    }
}

/// One speaker's decoded voice waiting to be played, and where it is heard from.
#[derive(Default)]
struct Track {
    buf: VecDeque<f32>,
    playing: bool,
    gain: [f32; 2],
}

impl Track {
    fn pull(&mut self) -> f32 {
        if !self.playing && self.buf.len() < START {
            return 0.0;
        }
        self.playing = true;
        self.buf.pop_front().unwrap_or_else(|| {
            // ran dry: wait to refill before playing on
            self.playing = false;
            0.0
        })
    }
}

/// What the output stream plays: every speaker, at 48 kHz, mixed to stereo.
#[derive(Default)]
pub struct Mixer {
    tracks: HashMap<u16, Track>,
}

impl Mixer {
    fn next(&mut self) -> [f32; 2] {
        let mut lr = [0.0; 2];
        for t in self.tracks.values_mut() {
            let s = t.pull();
            lr[0] += s * t.gain[0];
            lr[1] += s * t.gain[1];
        }
        lr.map(|v| v.clamp(-1.0, 1.0))
    }
}

/// Distance fall-off and equal-power left/right balance for a sound at `at`, heard from `ear` (`right` = the ear's
/// right-hand direction).
pub fn placement(ear: Vec3, right: Vec3, at: Vec3) -> [f32; 2] {
    let d = ear.distance(at);
    if d > FAR {
        return [0.0; 2];
    }
    let fall = NEAR / d.max(NEAR);
    let side = if d > 1e-3 { ((at - ear) / d).dot(right).clamp(-1.0, 1.0) } else { 0.0 };
    let a = (side + 1.0) * std::f32::consts::FRAC_PI_4;
    [a.cos() * fall * std::f32::consts::SQRT_2, a.sin() * fall * std::f32::consts::SQRT_2].map(|g| g.min(1.0))
}

struct Speaker {
    opus: Opus,
    next: Option<u16>,
    heard: f32,
}

#[derive(Resource)]
struct Voice {
    mic: Mutex<Receiver<Vec<f32>>>,
    mixer: Arc<Mutex<Mixer>>,
    encoder: Opus,
    speakers: HashMap<u16, Speaker>,
    gate_open_until: f32,
    last_sent: f32,
    tone: Option<(f32, u64)>,
    settings: Settings,
    settings_stamp: Option<std::time::SystemTime>,
    problem: Option<String>,
}

#[derive(Component)]
struct VoiceText;

fn start(mut commands: Commands) {
    let mixer = Arc::new(Mutex::new(Mixer::default()));
    let (tx, rx) = channel();
    let tone = std::env::var("METALLIC_VOICE_TONE").ok().and_then(|v| v.parse::<f32>().ok()).map(|hz| (hz, 0));
    let problem = audio_thread(mixer.clone(), (tone.is_none()).then_some(tx)).err();
    if let Some(p) = &problem {
        warn!("voice: {p}");
    }
    commands.insert_resource(Voice {
        mic: Mutex::new(rx),
        mixer,
        encoder: Opus::encoder(),
        speakers: HashMap::new(),
        gate_open_until: 0.0,
        last_sent: f32::MIN,
        tone,
        settings: Settings::default(),
        settings_stamp: None,
        problem,
    });
    commands.spawn((Text::new(""), TextFont { font_size: bevy::text::FontSize::Px(14.0), ..default() }, VoiceText,
        Node { position_type: PositionType::Absolute, bottom: px(12.0), right: px(12.0), ..default() }));
}

/// Opens the default microphone (if `mic` is given) and speakers on a thread that keeps the streams alive.
fn audio_thread(mixer: Arc<Mutex<Mixer>>, mic: Option<Sender<Vec<f32>>>) -> Result<(), String> {
    let (ready_tx, ready_rx) = channel();
    std::thread::Builder::new().name("voice-audio".into()).spawn(move || {
        let host = cpal::default_host();
        let mut streams = vec![];
        let mut problems = vec![];
        match host.default_output_device().ok_or("no speakers".to_string()).and_then(|d| output(&d, mixer)) {
            Ok(s) => streams.push(s),
            Err(e) => problems.push(e),
        }
        if let Some(tx) = mic {
            match host.default_input_device().ok_or("no microphone".to_string()).and_then(|d| input(&d, tx)) {
                Ok(s) => streams.push(s),
                Err(e) => problems.push(e),
            }
        }
        let _ = ready_tx.send(if problems.is_empty() { Ok(()) } else { Err(problems.join(", ")) });
        // the streams stop when dropped: keep them for the life of the game
        loop {
            std::thread::park();
        }
    }).map_err(|e| e.to_string())?;
    ready_rx.recv().map_err(|e| e.to_string())?
}

fn output(device: &cpal::Device, mixer: Arc<Mutex<Mixer>>) -> Result<cpal::Stream, String> {
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let (channels, rate) = (config.channels() as usize, config.sample_rate());
    let step = SAMPLE_RATE as f64 / rate as f64;
    let (mut phase, mut prev, mut cur) = (1.0f64, [0.0f32; 2], [0.0f32; 2]);
    let mut fill = move |out: &mut [f32]| {
        let mut m = mixer.lock().unwrap();
        for frame in out.chunks_mut(channels) {
            phase += step;
            while phase >= 1.0 {
                phase -= 1.0;
                (prev, cur) = (cur, m.next());
            }
            let t = phase as f32;
            let lr = [prev[0] + (cur[0] - prev[0]) * t, prev[1] + (cur[1] - prev[1]) * t];
            for (i, s) in frame.iter_mut().enumerate() {
                *s = if channels == 1 { (lr[0] + lr[1]) * 0.5 } else { lr.get(i).copied().unwrap_or(0.0) };
            }
        }
    };
    let err = |e| warn!("voice output: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(&config.config(), move |o: &mut [f32], _: &_| fill(o), err, None),
        cpal::SampleFormat::I16 => {
            let mut tmp = vec![];
            device.build_output_stream(&config.config(), move |o: &mut [i16], _: &_| {
                tmp.resize(o.len(), 0.0);
                fill(&mut tmp);
                o.iter_mut().zip(&tmp).for_each(|(d, s)| *d = (s * 32767.0) as i16);
            }, err, None)
        }
        f => return Err(format!("speaker sample format {f:?}")),
    }.map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn input(device: &cpal::Device, tx: Sender<Vec<f32>>) -> Result<cpal::Stream, String> {
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    match config.sample_format() {
        cpal::SampleFormat::F32 => input_as::<f32>(device, &config.config(), tx),
        cpal::SampleFormat::I16 => input_as::<i16>(device, &config.config(), tx),
        f => Err(format!("microphone sample format {f:?}")),
    }
}

fn input_as<T: SizedSample>(device: &cpal::Device, config: &cpal::StreamConfig, tx: Sender<Vec<f32>>) -> Result<cpal::Stream, String>
where
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let mut resample = Resample::new(config.sample_rate, SAMPLE_RATE);
    let (mut mono, mut at48, mut frame) = (vec![], vec![], Vec::with_capacity(FRAME));
    let stream = device.build_input_stream(config, move |data: &[T], _: &_| {
        mono.clear();
        mono.extend(data.chunks(channels).map(|c| c.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / channels as f32));
        at48.clear();
        resample.push(&mono, &mut at48);
        for &s in &at48 {
            frame.push(s);
            if frame.len() == FRAME {
                let _ = tx.send(std::mem::replace(&mut frame, Vec::with_capacity(FRAME)));
            }
        }
    }, |e| warn!("voice input: {e}"), None).map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn rms(pcm: &[f32]) -> f32 {
    (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len().max(1) as f32).sqrt()
}

/// Each frame: settings, the microphone (or tone) through the gate to the room, the room's voice into the
/// speakers' tracks, and where each speaker is heard from.
fn voice(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    link: Res<RoomLink>,
    mut v: ResMut<Voice>,
    ears: Query<&GlobalTransform, With<Camera3d>>,
    mut text: Single<&mut Text, With<VoiceText>>,
) {
    let v = &mut *v;
    if let Some(path) = Settings::path() {
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if stamp != v.settings_stamp {
            (v.settings, v.settings_stamp) = (Settings::load(&path), stamp);
        }
    }
    let s = &v.settings;
    let (open_mic, muted) = (s.on("voice_open_mic"), s.on("voice_mute"));
    let volume = (s.get("volume_voice") * s.get("volume_master") / 10_000.0) as f32;

    let mut frames: Vec<Vec<f32>> = v.mic.lock().unwrap().try_iter().collect();
    if let Some((hz, sent)) = &mut v.tone {
        // the tone keeps real time: as many frames as the clock says are due
        let due = (time.elapsed_secs_f64() * SAMPLE_RATE as f64) as u64 / FRAME as u64;
        while *sent < due {
            let start = *sent * FRAME as u64;
            frames.push((0..FRAME).map(|i| 0.3 * (std::f32::consts::TAU * *hz * (start + i as u64) as f32 / SAMPLE_RATE as f32).sin()).collect());
            *sent += 1;
        }
    }
    let now = time.elapsed_secs();
    let rt = link.get();
    for f in frames {
        let level = rms(&f);
        let send = !muted && if open_mic { level > GATE || now < v.gate_open_until } else { keys.pressed(PUSH_TO_TALK) };
        if open_mic && level > GATE {
            v.gate_open_until = now + HANGOVER;
        }
        if let (true, Some(rt)) = (send, &rt) {
            let packet = v.encoder.encode(&f);
            rt.room.lock().unwrap().send_voice(packet, (level * 4.0 * 255.0).min(255.0) as u8);
            v.last_sent = now;
        }
    }
    let talking = now - v.last_sent < 0.3;

    let Some(rt) = rt else { return };
    let mut room = rt.room.lock().unwrap();
    let mut decoded: Vec<(u16, Vec<f32>)> = vec![];
    while let Some(p) = room.poll_voice() {
        let sp = v.speakers.entry(p.sender).or_insert_with(|| Speaker { opus: Opus::decoder(), next: None, heard: 0.0 });
        let gap = sp.next.map_or(0, |n| p.sequence.wrapping_sub(n) as i16);
        if gap < 0 {
            continue; // late or repeated
        }
        if (1..=3).contains(&gap) {
            // lost frames: conceal all but the last, which the new packet's redundancy brings back
            for _ in 1..gap {
                decoded.push((p.sender, sp.opus.decode(None, false)));
            }
            decoded.push((p.sender, sp.opus.decode(Some(&p.frame), true)));
        }
        decoded.push((p.sender, sp.opus.decode(Some(&p.frame), false)));
        sp.next = Some(p.sequence.wrapping_add(1));
        sp.heard = now;
    }
    let heads: HashMap<u16, (Vec3, String)> = room.shown().iter().map(|s| {
        let at = s.head.map_or(Vec3::from(s.body.position) + Vec3::Y * 1.55, |h| Vec3::from(h.position));
        (s.actor as u16, (at, s.name.clone()))
    }).collect();
    drop(room);

    let ear = ears.single().ok();
    let mut m = v.mixer.lock().unwrap();
    for (who, pcm) in decoded {
        let t = m.tracks.entry(who).or_default();
        t.buf.extend(pcm);
        let extra = t.buf.len().saturating_sub(MAX_BUFFERED);
        t.buf.drain(..extra);
    }
    m.tracks.retain(|who, _| v.speakers.get(who).is_some_and(|s| now - s.heard < 5.0));
    for (who, t) in m.tracks.iter_mut() {
        let place = match (ear, heads.get(who)) {
            (Some(e), Some((at, _))) => placement(e.translation(), e.right().into(), *at),
            _ => [1.0; 2],
        };
        t.gain = place.map(|g| g * volume);
    }
    drop(m);
    v.speakers.retain(|_, s| now - s.heard < 5.0);

    let hearing: Vec<&str> = v.speakers.iter().filter(|(_, s)| now - s.heard < 0.5).filter_map(|(w, _)| heads.get(w).map(|h| h.1.as_str())).collect();
    let mode = if muted { "muted" } else if open_mic { "open mic" } else { "push to talk (T)" };
    let line = format!("Voice: {mode}{}{}{}", if talking { ", talking" } else { "" },
        if hearing.is_empty() { String::new() } else { format!(" | hearing {}", hearing.join(", ")) },
        v.problem.as_ref().map_or(String::new(), |p| format!(" | {p}")));
    if text.0 != line {
        text.0 = line;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_carries_a_tone_and_covers_a_lost_frame() {
        let (mut enc, mut dec) = (Opus::encoder(), Opus::decoder());
        let tone = |k: usize| (0..FRAME).map(|i| 0.3 * (std::f32::consts::TAU * 440.0 * (k * FRAME + i) as f32 / 48_000.0).sin()).collect::<Vec<f32>>();
        let packets: Vec<Vec<u8>> = (0..70).map(|k| enc.encode(&tone(k))).collect();
        assert!(packets.iter().all(|p| !p.is_empty() && p.len() < 200), "{:?}", packets.iter().map(Vec::len).collect::<Vec<_>>());
        let mut out = vec![];
        for (k, p) in packets.iter().enumerate() {
            if k == 30 {
                // lost: brought back from the next packet's redundancy
                continue;
            }
            if k == 31 {
                out.extend(dec.decode(Some(p), true));
            }
            out.extend(dec.decode(Some(p), false));
        }
        assert_eq!(out.len(), 70 * FRAME);
        // after the codec's start-up the level matches and the pitch is 440 Hz (zero crossings over one second)
        let second = &out[FRAME * 10..FRAME * 10 + 48_000];
        assert!((rms(second) - 0.3 / 2f32.sqrt()).abs() < 0.05, "{}", rms(second));
        let crossings = second.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((430..=450).contains(&crossings), "{crossings}");
    }

    #[test]
    fn resampling_and_placement() {
        let mut r = Resample::new(44_100, 48_000);
        let mut out = vec![];
        for _ in 0..10 {
            r.push(&[0.5; 441], &mut out);
        }
        assert!((out.len() as i32 - 4800).abs() <= 1 && out[10..].iter().all(|&s| (s - 0.5).abs() < 1e-6), "{}", out.len());
        // in front at 1 m: both ears full; to the right: right ear only; 6 m: a quarter
        let ([l, r], right) = (placement(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, -1.0)), placement(Vec3::ZERO, Vec3::X, Vec3::X));
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        assert!(right[0] < 1e-5 && (right[1] - 1.0).abs() < 1e-5);
        assert!((placement(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, -6.0))[0] - 0.25).abs() < 1e-5);
    }
}

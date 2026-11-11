//! Rec Room 2026 (Unity 6) AudioClip (class 83) decode.
//!
//! Each AudioClip's samples are an **FSB5** (FMOD Sound Bank) blob in a streamed `.resource`. The
//! resource is encrypted in the install but present decrypted in the corpus (`m_Resource.m_Source`
//! + offset/size). Inside, one subsound. Rec Room 2026 uses three FSB5 codecs: **PCM16** (mode 2),
//! **IMA ADPCM** (mode 7, Unity's "ADPCM") and **Vorbis** (mode 15). Channels/frequency come straight
//! from the AudioClip fields, so decoding only needs the FSB5 data region + the codec. PCM16 and IMA
//! ADPCM are implemented (also FADPCM mode 16, though Rec Room doesn't use it); Vorbis is not yet
//! (FMOD strips the Vorbis setup headers).
//!
//! [`AudioClip::from_value`] reads the metadata (decode the object with a bundle-sourced type tree,
//! as for [`crate::AnimationClip`]); [`AudioClip::decode`] turns the `.resource` bytes into
//! interleaved PCM. PCM16 and FADPCM are implemented; Vorbis (FMOD-stripped headers) is not yet.

use crate::value::Value;
use crate::{Error, Result};

/// AudioClip metadata + where its FSB5 blob lives.
#[derive(Debug, Clone, Default)]
pub struct AudioClip {
    pub name: String,
    pub channels: u32,
    pub frequency: u32,
    /// Unity `m_CompressionFormat`: 0 PCM, 1 Vorbis, 2 ADPCM (FADPCM), 3 MP3.
    pub compression: i32,
    pub length_seconds: f32,
    /// streamed resource holding the FSB5 blob
    pub source: String,
    pub offset: u64,
    pub size: u64,
}

/// Decoded interleaved 16-bit PCM.
#[derive(Debug, Clone, Default)]
pub struct AudioData {
    pub channels: u32,
    pub sample_rate: u32,
    /// interleaved samples: `[ch0, ch1, ch0, ch1, ...]`
    pub samples: Vec<i16>,
}

impl AudioData {
    /// A canonical 16-bit PCM WAV file.
    pub fn to_wav(&self) -> Vec<u8> {
        let ch = self.channels.max(1) as u16;
        let sr = self.sample_rate.max(1);
        let bytes_per_sample = 2u32;
        let block_align = ch as u32 * bytes_per_sample;
        let data_len = (self.samples.len() * 2) as u32;
        let mut w = Vec::with_capacity(44 + data_len as usize);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data_len).to_le_bytes());
        w.extend_from_slice(b"WAVE");
        w.extend_from_slice(b"fmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&ch.to_le_bytes());
        w.extend_from_slice(&sr.to_le_bytes());
        w.extend_from_slice(&(sr * block_align).to_le_bytes());
        w.extend_from_slice(&(block_align as u16).to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data_len.to_le_bytes());
        for s in &self.samples {
            w.extend_from_slice(&s.to_le_bytes());
        }
        w
    }
}

fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

impl AudioClip {
    pub fn from_value(v: &Value) -> Result<AudioClip> {
        let g = |n: &str| v.get(n);
        let res = g("m_Resource");
        Ok(AudioClip {
            name: g("m_Name").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            channels: g("m_Channels").and_then(|x| x.as_i64()).unwrap_or(1).max(1) as u32,
            frequency: g("m_Frequency").and_then(|x| x.as_i64()).unwrap_or(44100) as u32,
            compression: g("m_CompressionFormat").and_then(|x| x.as_i64()).unwrap_or(-1) as i32,
            length_seconds: g("m_Length").and_then(|x| x.as_f32()).unwrap_or(0.0),
            source: res.and_then(|r| r.get("m_Source")).and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            offset: res.and_then(|r| r.get("m_Offset")).and_then(|x| x.as_u64()).unwrap_or(0),
            size: res.and_then(|r| r.get("m_Size")).and_then(|x| x.as_u64()).unwrap_or(0),
        })
    }

    /// The `.resource` basename this clip streams from (e.g. `resources.resource`).
    pub fn resource_name(&self) -> &str {
        self.source.rsplit(['/', '\\']).next().unwrap_or(&self.source)
    }

    /// Decodes this clip to interleaved PCM. `resource` is the whole `.resource` file the clip streams
    /// from (its FSB5 blob is sliced out by `offset`/`size`). Errors for Vorbis (not yet implemented).
    pub fn decode(&self, resource: &[u8]) -> Result<AudioData> {
        let (o, n) = (self.offset as usize, self.size as usize);
        if o + n > resource.len() {
            return Err(Error::Format(format!("{}: resource too small ({} < {o}+{n})", self.name, resource.len())));
        }
        let fsb = &resource[o..o + n];
        decode_fsb5(fsb, self.channels, self.frequency, self.trim_samples())
    }

    fn trim_samples(&self) -> Option<usize> {
        (self.length_seconds > 0.0).then(|| (self.length_seconds * self.frequency as f32).round() as usize)
    }
}

/// FADPCM coefficients (XA/PSX style, scaled by 64), indices 0..8 (only 1..4 are non-zero).
const FADPCM_COEFS: [[i32; 2]; 8] = [[0, 0], [60, 0], [122, 60], [115, 52], [98, 55], [0, 0], [0, 0], [0, 0]];

/// Standard IMA ADPCM step-size table (89 entries).
const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73,
    80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494,
    544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499,
    2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487,
    12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
/// IMA ADPCM index adjustment table (16 entries).
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Decodes FMOD FSB IMA ADPCM (mode 7): frames of 0x24 bytes per channel, all channels sharing each
/// frame; header has every channel's int16 predictor then int8 step index; 64 samples/channel/frame.
/// Returns per-channel sample vectors.
fn decode_fsb_ima(data: &[u8], channels: usize) -> Vec<Vec<i16>> {
    let frame_size = 0x24 * channels;
    let mut out: Vec<Vec<i16>> = vec![Vec::new(); channels];
    for frame in data.chunks(frame_size) {
        if frame.len() < frame_size {
            break;
        }
        for c in 0..channels {
            let mut pred = i16::from_le_bytes([frame[2 * c], frame[2 * c + 1]]) as i32;
            let mut index = (frame[2 * channels + 2 * c] as i8 as i32).clamp(0, 88);
            out[c].push(pred as i16);
            for n in 0..63usize {
                let byte_off = 0x04 * channels + 0x02 * c + (n / 4) * 2 * channels + (n % 4) / 2;
                let shift = if n % 2 == 0 { 0 } else { 4 };
                let nib = ((frame[byte_off] >> shift) & 0x0f) as i32;
                let step = IMA_STEP[index as usize];
                let mut delta = step >> 3;
                if nib & 1 != 0 {
                    delta += step >> 2;
                }
                if nib & 2 != 0 {
                    delta += step >> 1;
                }
                if nib & 4 != 0 {
                    delta += step;
                }
                if nib & 8 != 0 {
                    delta = -delta;
                }
                pred = (pred + delta).clamp(-32768, 32767);
                index = (index + IMA_INDEX[nib as usize]).clamp(0, 88);
                out[c].push(pred as i16);
            }
        }
    }
    out
}

/// Parses an FSB5 blob (single Unity subsound) and decodes its data region to interleaved PCM.
pub fn decode_fsb5(fsb: &[u8], channels: u32, frequency: u32, trim_per_channel: Option<usize>) -> Result<AudioData> {
    if fsb.len() < 0x3c || &fsb[..4] != b"FSB5" {
        return Err(Error::Format("not an FSB5 blob".into()));
    }
    let version = u32le(fsb, 4);
    let sample_headers_size = u32le(fsb, 0x0c) as usize;
    let name_table_size = u32le(fsb, 0x10) as usize;
    let data_size = u32le(fsb, 0x14) as usize;
    let mode = u32le(fsb, 0x18);
    let header_size = if version == 0 { 0x28 } else { 0x3c };
    let data_start = header_size + sample_headers_size + name_table_size;
    if data_start + data_size > fsb.len() {
        return Err(Error::Format("FSB5 data region past end".into()));
    }
    let data = &fsb[data_start..data_start + data_size];
    let ch = channels.max(1) as usize;

    let per_channel: Vec<Vec<i16>> = match mode {
        // PCM16 (FMOD mode 2): interleaved little-endian i16, already [L R L R ...]
        2 => {
            let all: Vec<i16> = data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            // de-interleave into per-channel so trimming + reinterleave is uniform
            (0..ch).map(|c| all.iter().skip(c).step_by(ch).copied().collect()).collect()
        }
        // IMA ADPCM (FMOD mode 7): 0x24-per-channel frames, channels share each frame
        7 => decode_fsb_ima(data, ch),
        // FADPCM (FMOD mode 16): 0x8c blocks, external interleave by block across channels
        16 => {
            let blocks: Vec<&[u8]> = data.chunks(0x8c).filter(|b| b.len() == 0x8c).collect();
            (0..ch)
                .map(|c| {
                    let mut out = Vec::new();
                    for b in blocks.iter().skip(c).step_by(ch) {
                        decode_fadpcm_block(b, &mut out);
                    }
                    out
                })
                .collect()
        }
        15 => return Err(Error::Unsupported("FSB5 Vorbis (FMOD-stripped headers) not implemented".into())),
        other => return Err(Error::Unsupported(format!("FSB5 codec mode {other}"))),
    };

    // trim each channel to the real sample count, then interleave
    let n = per_channel.iter().map(|c| c.len()).min().unwrap_or(0);
    let n = trim_per_channel.map(|t| t.min(n)).unwrap_or(n);
    let mut samples = Vec::with_capacity(n * ch);
    for i in 0..n {
        for c in 0..ch {
            samples.push(per_channel[c][i]);
        }
    }
    Ok(AudioData { channels: ch as u32, sample_rate: frequency, samples })
}

/// Decodes one 0x8c-byte FADPCM frame (256 samples) into `out`.
fn decode_fadpcm_block(frame: &[u8], out: &mut Vec<i16>) {
    let coefs = u32le(frame, 0);
    let shifts = u32le(frame, 4);
    let mut hist1 = i16::from_le_bytes([frame[8], frame[9]]) as i32;
    let mut hist2 = i16::from_le_bytes([frame[10], frame[11]]) as i32;
    for g in 0..8 {
        let idx = ((coefs >> (g * 4)) & 0x0f) % 7;
        let s = (shifts >> (g * 4)) & 0x0f;
        let shift = 22 - s as i32;
        let [c1, c2] = FADPCM_COEFS[idx as usize];
        for w in 0..4 {
            let word = u32le(frame, 0x0c + g * 0x10 + w * 4);
            for k in 0..8 {
                let nib = ((word >> (k * 4)) & 0x0f) as i32;
                let raw = (nib << 28) >> shift; // sign-extend nibble, arithmetic shift
                let pred = raw - hist2 * c2 + hist1 * c1;
                let sample = (pred >> 6).clamp(-32768, 32767);
                out.push(sample as i16);
                hist2 = hist1;
                hist1 = sample;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bundle, Reader, SerializedFile};
    use std::path::Path;

    /// Decode real corpus AudioClips (PCM16 + FADPCM) with a bundle-sourced AudioClip type tree and
    /// check the output is sane (right length, non-silent). Gated on `RR_BUNDLE_DIR` (the install's
    /// Addressables bundle folder — scanned for a bundle holding an AudioClip), `RR_CORPUS_DIR`, and
    /// `RR_RESOURCE_DIR` (the install's `RecRoom_Data`, where the `.resource` streams are plaintext).
    /// Skipped when unset.
    #[test]
    fn decode_corpus_audio() {
        let (Ok(bdir), Ok(corpus), Ok(resdir)) =
            (std::env::var("RR_BUNDLE_DIR"), std::env::var("RR_CORPUS_DIR"), std::env::var("RR_RESOURCE_DIR"))
        else {
            eprintln!("decode_corpus_audio: skipped (set RR_BUNDLE_DIR, RR_CORPUS_DIR, RR_RESOURCE_DIR)");
            return;
        };
        let mut files: Vec<_> = std::fs::read_dir(&bdir).unwrap().filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "bundle").unwrap_or(false)).collect();
        files.sort();
        let tree = files.iter().find_map(|f| {
            let b = Bundle::open(f).ok()?;
            b.serialized_files().ok()?.into_iter().flat_map(|(_, sf)| sf.types)
                .find(|t| t.class_id == 83 && t.tree.is_some()).and_then(|t| t.tree)
        });
        let Some(tree) = tree else {
            eprintln!("decode_corpus_audio: no AudioClip tree found in bundles; skipped");
            return;
        };
        let res = SerializedFile::parse(std::fs::read(Path::new(&corpus).join("resources.assets")).unwrap()).unwrap();
        let mut cache: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        let (mut pcm, mut adpcm) = (0, 0);
        for o in res.objects.iter().filter(|o| o.class_id == 83) {
            let mut r = Reader::new(res.object_bytes(o), res.big_endian);
            let Ok(v) = crate::value::read(&tree, &res.ref_types, &mut r) else { continue };
            let clip = AudioClip::from_value(&v).unwrap();
            if clip.compression != 0 && clip.compression != 2 { continue; } // PCM or ADPCM
            let resbytes = cache.entry(clip.resource_name().to_string())
                .or_insert_with(|| std::fs::read(Path::new(&resdir).join(clip.resource_name())).unwrap_or_default());
            if resbytes.is_empty() { continue; }
            let Ok(audio) = clip.decode(resbytes) else { continue };
            assert!(!audio.samples.is_empty(), "{}: empty", clip.name);
            assert!(audio.samples.iter().any(|&s| s != 0), "{}: all silence", clip.name);
            // length within 10% of declared
            if clip.length_seconds > 0.0 {
                let secs = audio.samples.len() as f32 / (audio.channels.max(1) * audio.sample_rate) as f32;
                assert!((secs - clip.length_seconds).abs() < clip.length_seconds * 0.1 + 0.05, "{}: {secs} vs {}", clip.name, clip.length_seconds);
            }
            if clip.compression == 0 { pcm += 1 } else { adpcm += 1 }
            if pcm + adpcm >= 40 { break; }
        }
        assert!(pcm > 0 && adpcm > 0, "need both PCM and ADPCM clips (got pcm={pcm} adpcm={adpcm})");
        eprintln!("decode_corpus_audio: PCM {pcm} + IMA-ADPCM {adpcm} clips decoded + validated");
    }
}

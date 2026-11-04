//! Room textures made GPU-friendly: every texture a `StandardMaterial` uses gets a full mip chain, is capped at
//! `Quality::texture_cap`, and is block-compressed where the GPU can sample it that way (BC7 colour and BC5 normal
//! maps on desktop, ASTC 4x4 on phones and headsets; RGBA8 with mips elsewhere). The loaders hand over plain RGBA8
//! (`render::image`); the work runs on Bevy's async compute pool and swaps the image in place when done, so a room
//! shows at once and sharpens a moment later. Results are kept on disk under
//! `%LOCALAPPDATA%\Metallic-RC\cache\textures` (by content hash; 1 GB cap), so the second load only reads them.
//!
//! Why: InkSpace's 115 textures were 382 MB uploaded as RGBA8 without mips; BC7 is 4x smaller, mips cost a third
//! more and stop the shimmer and cache thrash of minified textures.
use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
    path::PathBuf,
};

use bevy::{
    asset::AssetEvent,
    image::{CompressedImageFormatSupport, CompressedImageFormats, ImageSampler, ImageSamplerDescriptor},
    platform::hash::FixedHasher,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};

use crate::quality::Quality;

/// What a texture is for, which picks its format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// sRGB colour (base colour, emissive)
    Color,
    /// linear data (metallic/roughness)
    Data,
    /// tangent-space normal map: only X and Y are kept, the shader rebuilds Z
    Normal,
}

/// The block format family to produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Bc,
    Astc,
    /// RGBA8 with mips (no compressed format available)
    Plain,
}

impl Target {
    pub fn for_gpu(f: CompressedImageFormats) -> Target {
        // phones and headsets prefer ASTC even when they also list BC
        if f.contains(CompressedImageFormats::ASTC_LDR) && (cfg!(target_os = "android") || !f.contains(CompressedImageFormats::BC)) {
            Target::Astc
        } else if f.contains(CompressedImageFormats::BC) {
            Target::Bc
        } else {
            Target::Plain
        }
    }
}

pub struct TexPrepPlugin;

impl Plugin for TexPrepPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pending>().add_systems(Last, (queue, finish).chain());
    }
}

#[derive(Resource, Default)]
struct Pending {
    tasks: Vec<(AssetId<Image>, Task<Option<Image>>)>,
    /// images already handled (or being handled), and the materials that use each
    done: HashSet<AssetId<Image>>,
    users: HashMap<AssetId<Image>, Vec<AssetId<StandardMaterial>>>,
}

fn queue(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    materials: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    support: Option<Res<CompressedImageFormatSupport>>,
    quality: Option<Res<Quality>>,
    mut pending: ResMut<Pending>,
) {
    // METALLIC_RC_TEXPREP=plain|bc|astc forces a target (to compare and debug)
    let forced = std::env::var("METALLIC_RC_TEXPREP").ok().and_then(|v| match v.as_str() {
        "plain" => Some(Target::Plain),
        "bc" => Some(Target::Bc),
        "astc" => Some(Target::Astc),
        _ => None,
    });
    let target = forced.unwrap_or_else(|| support.map_or(Target::Plain, |s| Target::for_gpu(s.0)));
    let (cap, aniso) = quality.map_or((4096, 8), |q| (q.texture_cap, q.anisotropy));
    for e in events.read() {
        let AssetEvent::Added { id } = e else { continue };
        let Some(m) = materials.get(*id) else { continue };
        let maps = [(&m.base_color_texture, Role::Color), (&m.emissive_texture, Role::Color), (&m.metallic_roughness_texture, Role::Data), (&m.normal_map_texture, Role::Normal)];
        for (handle, role) in maps {
            let Some(image_id) = handle.as_ref().map(Handle::id) else { continue };
            pending.users.entry(image_id).or_default().push(*id);
            if !pending.done.insert(image_id) {
                continue;
            }
            let Some(img) = images.get(image_id) else { continue };
            let d = &img.texture_descriptor;
            let fmt = d.format;
            let plain = matches!(fmt, TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm) && d.mip_level_count == 1
                && d.size.depth_or_array_layers == 1 && d.dimension == TextureDimension::D2;
            let Some(data) = img.data.clone().filter(|_| plain) else { continue };
            let (w, h, srgb) = (d.size.width, d.size.height, fmt == TextureFormat::Rgba8UnormSrgb);
            let base = Image { data: None, data_order: img.data_order, texture_descriptor: img.texture_descriptor.clone(), sampler: img.sampler.clone(),
                texture_view_descriptor: img.texture_view_descriptor.clone(), asset_usage: img.asset_usage, copy_on_resize: img.copy_on_resize };
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let job = Job { w, h, srgb, role, target, cap };
                let out = prepare_cached(&job, &data)?;
                Some(out.into_image(base, aniso))
            });
            pending.tasks.push((image_id, task));
        }
    }
}

/// Swaps finished images in and touches the materials that use them, so their bind groups and normal-map flags
/// are rebuilt with the new format.
fn finish(mut pending: ResMut<Pending>, mut images: ResMut<Assets<Image>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut touched = HashSet::new();
    let p = &mut *pending;
    p.tasks.retain_mut(|(id, task)| {
        let Some(result) = block_on(future::poll_once(task)) else { return true };
        if let Some(img) = result {
            if images.contains(*id) {
                let _ = images.insert(*id, img);
                touched.extend(p.users.get(id).into_iter().flatten().copied());
            }
        }
        false
    });
    for m in touched {
        let _ = materials.get_mut(m);
    }
}

/// One texture to prepare.
#[derive(Clone, Copy, Debug, Hash)]
pub struct Job {
    pub w: u32,
    pub h: u32,
    pub srgb: bool,
    pub role: Role,
    pub target: Target,
    /// largest side kept
    pub cap: u32,
}

/// A prepared texture: its format, the size of its top level and every level's data, top first.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub format: TextureFormat,
    pub w: u32,
    pub h: u32,
    pub mips: u32,
    pub data: Vec<u8>,
}

impl Prepared {
    /// `base` without data: its sampler and usage are kept; mipmapped and anisotropic sampling is switched on.
    pub fn into_image(self, mut base: Image, anisotropy: u16) -> Image {
        base.texture_descriptor.size = Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 };
        base.texture_descriptor.format = self.format;
        base.texture_descriptor.mip_level_count = self.mips;
        base.data = Some(self.data);
        let mut s = match &base.sampler {
            ImageSampler::Descriptor(d) => d.clone(),
            ImageSampler::Default => ImageSamplerDescriptor::linear(),
        };
        s.mipmap_filter = bevy::image::ImageFilterMode::Linear;
        // wgpu wants every filter linear for anisotropy
        let linear = s.mag_filter == bevy::image::ImageFilterMode::Linear && s.min_filter == bevy::image::ImageFilterMode::Linear;
        s.anisotropy_clamp = if linear { anisotropy.max(1) } else { 1 };
        base.sampler = ImageSampler::Descriptor(s);
        base
    }
}

/// `prepare`, through the disk cache.
pub fn prepare_cached(job: &Job, rgba: &[u8]) -> Option<Prepared> {
    let key = FixedHasher.hash_one((job, rgba));
    let path = cache_dir().map(|d| d.join(format!("{key:016x}.tex")));
    if let Some(p) = path.as_ref().and_then(|p| read(p)) {
        return Some(p);
    }
    let out = prepare(job, rgba)?;
    if let Some(p) = &path {
        write(p, &out);
    }
    Some(out)
}

/// Mips (box filter, in linear light for sRGB), the size cap, then the block format.
pub fn prepare(job: &Job, rgba: &[u8]) -> Option<Prepared> {
    let (w, h) = (job.w as usize, job.h as usize);
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return None;
    }
    let mut levels = vec![(w, h, rgba.to_vec())];
    while let Some((lw, lh, px)) = levels.last().filter(|(lw, lh, _)| *lw > 1 || *lh > 1) {
        let next = downsample(*lw, *lh, px, job.srgb);
        levels.push(next);
    }
    // the cap drops top levels; never below 4 px so block formats still have a full block
    while levels.len() > 1 && levels[0].0.max(levels[0].1) > job.cap.max(4) as usize {
        levels.remove(0);
    }
    let (top_w, top_h) = (levels[0].0 as u32, levels[0].1 as u32);
    // block formats need the top level in whole blocks (wgpu); odd sizes stay plain
    let target = if top_w % 4 != 0 || top_h % 4 != 0 { Target::Plain } else { job.target };
    let (format, encode): (TextureFormat, fn(usize, usize, &[u8], Role) -> Vec<u8>) = match (target, job.role) {
        (Target::Bc, Role::Normal) => (TextureFormat::Bc5RgUnorm, encode_bc5),
        (Target::Bc, _) => (if job.srgb { TextureFormat::Bc7RgbaUnormSrgb } else { TextureFormat::Bc7RgbaUnorm }, encode_bc7),
        (Target::Astc, _) => {
            let channel = if job.srgb { bevy::render::render_resource::AstcChannel::UnormSrgb } else { bevy::render::render_resource::AstcChannel::Unorm };
            (TextureFormat::Astc { block: bevy::render::render_resource::AstcBlock::B4x4, channel }, encode_astc)
        }
        (Target::Plain, _) => (if job.srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm }, |_, _, px, _| px.to_vec()),
    };
    let mut data = Vec::new();
    for (lw, lh, px) in &levels {
        data.extend(encode(*lw, *lh, px, job.role));
    }
    Some(Prepared { format, w: top_w, h: top_h, mips: levels.len() as u32, data })
}

/// sRGB byte to linear light, and back.
fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(l: f32) -> u8 {
    let c = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// Half size (each side at least 1), averaging 2x2 pixels (or 2x1 / 1x2 at an odd edge).
fn downsample(w: usize, h: usize, px: &[u8], srgb: bool) -> (usize, usize, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let lut: Option<Vec<f32>> = srgb.then(|| (0..=255u8).map(to_linear).collect());
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let xs = [(2 * x).min(w - 1), (2 * x + 1).min(w - 1)];
            let ys = [(2 * y).min(h - 1), (2 * y + 1).min(h - 1)];
            for c in 0..4 {
                let mut sum = 0.0f32;
                for &sy in &ys {
                    for &sx in &xs {
                        let v = px[(sy * w + sx) * 4 + c];
                        sum += match (&lut, c) {
                            (Some(l), 0..=2) => l[v as usize],
                            _ => v as f32 / 255.0,
                        };
                    }
                }
                let avg = sum / 4.0;
                out[(y * nw + x) * 4 + c] = match (&lut, c) {
                    (Some(_), 0..=2) => to_srgb(avg),
                    _ => (avg * 255.0 + 0.5) as u8,
                };
            }
        }
    }
    (nw, nh, out)
}

/// The level padded to whole 4x4 blocks by repeating its last row and column (the encoders want that).
fn padded(w: usize, h: usize, px: &[u8], channels: usize) -> (u32, u32, Vec<u8>) {
    let (pw, ph) = (w.div_ceil(4) * 4, h.div_ceil(4) * 4);
    let mut out = Vec::with_capacity(pw * ph * channels);
    for y in 0..ph {
        for x in 0..pw {
            let i = (y.min(h - 1) * w + x.min(w - 1)) * 4;
            out.extend_from_slice(&px[i..i + channels]);
        }
    }
    (pw as u32, ph as u32, out)
}

fn has_alpha(px: &[u8]) -> bool {
    px.chunks_exact(4).any(|p| p[3] < 255)
}

fn encode_bc7(w: usize, h: usize, px: &[u8], _role: Role) -> Vec<u8> {
    let (pw, ph, data) = padded(w, h, px, 4);
    let settings = if has_alpha(px) { intel_tex_2::bc7::alpha_very_fast_settings() } else { intel_tex_2::bc7::opaque_very_fast_settings() };
    intel_tex_2::bc7::compress_blocks(&settings, &intel_tex_2::RgbaSurface { data: &data, width: pw, height: ph, stride: pw * 4 })
}

fn encode_bc5(w: usize, h: usize, px: &[u8], _role: Role) -> Vec<u8> {
    let (pw, ph, data) = padded(w, h, px, 2);
    intel_tex_2::bc5::compress_blocks(&intel_tex_2::RgSurface { data: &data, width: pw, height: ph, stride: pw * 2 })
}

fn encode_astc(w: usize, h: usize, px: &[u8], role: Role) -> Vec<u8> {
    let (pw, ph, mut data) = padded(w, h, px, 4);
    if role == Role::Normal {
        // Bevy reads ASTC normal maps as two-component: X in red, Y in green
        for p in data.chunks_exact_mut(4) {
            (p[2], p[3]) = (0, 255);
        }
    }
    let settings = if role != Role::Normal && has_alpha(px) { intel_tex_2::astc::alpha_fast_settings(4, 4) } else { intel_tex_2::astc::opaque_fast_settings(4, 4) };
    intel_tex_2::astc::compress_blocks(&settings, &intel_tex_2::RgbaSurface { data: &data, width: pw, height: ph, stride: pw * 4 })
}

// ---- the disk cache ----

const MAGIC: &[u8; 8] = b"MRCTEX01";
const LIMIT: u64 = 1 << 30;

/// `%LOCALAPPDATA%\Metallic-RC\cache\textures` (Linux/Android: `$XDG_CACHE_HOME` or `~/.cache`, `metallic-rc/textures`).
pub fn cache_dir() -> Option<PathBuf> {
    if std::env::var_os("METALLIC_RC_NO_CACHE").is_some() {
        return None;
    }
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        return Some(PathBuf::from(d).join("Metallic-RC").join("cache").join("textures"));
    }
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("metallic-rc").join("textures"))
}

/// Formats by number in the cache file (only those `prepare` makes).
fn format_code(f: TextureFormat) -> u8 {
    use bevy::render::render_resource::AstcChannel as C;
    match f {
        TextureFormat::Bc7RgbaUnorm => 1,
        TextureFormat::Bc7RgbaUnormSrgb => 2,
        TextureFormat::Bc5RgUnorm => 3,
        TextureFormat::Rgba8Unorm => 4,
        TextureFormat::Rgba8UnormSrgb => 5,
        TextureFormat::Astc { channel: C::Unorm, .. } => 6,
        TextureFormat::Astc { channel: C::UnormSrgb, .. } => 7,
        _ => 0,
    }
}

fn format_of(code: u8) -> Option<TextureFormat> {
    use bevy::render::render_resource::{AstcBlock, AstcChannel as C};
    Some(match code {
        1 => TextureFormat::Bc7RgbaUnorm,
        2 => TextureFormat::Bc7RgbaUnormSrgb,
        3 => TextureFormat::Bc5RgUnorm,
        4 => TextureFormat::Rgba8Unorm,
        5 => TextureFormat::Rgba8UnormSrgb,
        6 => TextureFormat::Astc { block: AstcBlock::B4x4, channel: C::Unorm },
        7 => TextureFormat::Astc { block: AstcBlock::B4x4, channel: C::UnormSrgb },
        _ => return None,
    })
}

fn read(path: &std::path::Path) -> Option<Prepared> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 21 || &b[..8] != MAGIC {
        return None;
    }
    let u = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    Some(Prepared { format: format_of(b[8])?, w: u(9), h: u(13), mips: u(17), data: b[21..].to_vec() })
}

fn write(path: &std::path::Path, p: &Prepared) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let mut b = Vec::with_capacity(21 + p.data.len());
    b.extend_from_slice(MAGIC);
    b.push(format_code(p.format));
    for v in [p.w, p.h, p.mips] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&p.data);
    let tmp = path.with_extension(format!("part{}", std::process::id()));
    if std::fs::write(&tmp, &b).is_err() || std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    trim(dir);
}

/// Drops the oldest files until the folder is under `LIMIT`.
fn trim(dir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd.flatten()
        .filter_map(|e| { let m = e.metadata().ok()?; Some((m.modified().ok()?, m.len(), e.path())) })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= LIMIT {
        return;
    }
    files.sort();
    for (_, len, p) in files {
        if total <= LIMIT {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: usize, h: usize) -> Vec<u8> {
        (0..w * h).flat_map(|i| if (i % w + i / w) % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }).collect()
    }

    #[test]
    fn mips_and_cap() {
        let job = Job { w: 64, h: 32, srgb: false, role: Role::Data, target: Target::Plain, cap: 4096 };
        let p = prepare(&job, &checker(64, 32)).unwrap();
        // 64x32, 32x16, ..., 2x1, 1x1
        assert_eq!((p.w, p.h, p.mips), (64, 32, 7));
        assert_eq!(p.data.len(), (0..7).map(|m| ((64 >> m).max(1) * (32 >> m).max(1) * 4) as usize).sum::<usize>());
        // a checkerboard averages to mid grey
        let last = &p.data[p.data.len() - 4..];
        assert!((126..=129).contains(&last[0]), "{last:?}");
        let capped = prepare(&Job { cap: 16, ..job }, &checker(64, 32)).unwrap();
        assert_eq!((capped.w, capped.h, capped.mips), (16, 8, 5));
    }

    #[test]
    fn srgb_mips_average_in_linear_light() {
        // black and white average to linear 0.5, which is sRGB 188, not 128
        let job = Job { w: 2, h: 2, srgb: true, role: Role::Color, target: Target::Plain, cap: 4096 };
        let p = prepare(&job, &checker(2, 2)).unwrap();
        assert_eq!(p.data[16..], [188, 188, 188, 255]);
    }

    #[test]
    fn bc_sizes() {
        // 32x12: 8x3 blocks of 16 bytes at the top level; the smaller levels are padded to whole blocks
        let px: Vec<u8> = (0..32 * 12 * 4).map(|i| (i * 7 % 251) as u8).collect();
        let job = Job { w: 32, h: 12, srgb: true, role: Role::Color, target: Target::Bc, cap: 4096 };
        let p = prepare(&job, &px).unwrap();
        assert_eq!((p.format, p.mips), (TextureFormat::Bc7RgbaUnormSrgb, 6));
        let blocks: usize = [(32, 12), (16, 6), (8, 3), (4, 1), (2, 1), (1, 1)].iter().map(|&(w, h): &(usize, usize)| w.div_ceil(4) * h.div_ceil(4)).sum();
        assert_eq!(p.data.len(), blocks * 16);
        let n = prepare(&Job { role: Role::Normal, srgb: false, ..job }, &px).unwrap();
        assert_eq!((n.format, n.data.len()), (TextureFormat::Bc5RgUnorm, blocks * 16));
        // and the image Bevy gets says the same size
        let img = n.into_image(Image::default(), 8);
        assert_eq!(crate::quality::image_bytes(&img), (blocks * 16) as u64);
        // a top level that is not whole blocks stays RGBA8
        let odd = prepare(&Job { w: 30, h: 10, ..job }, &px[..30 * 10 * 4]).unwrap();
        assert_eq!(odd.format, TextureFormat::Rgba8UnormSrgb);
    }

    #[test]
    fn cache_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("mrc-texprep-{}", std::process::id()));
        let path = dir.join("x.tex");
        let p = Prepared { format: TextureFormat::Bc7RgbaUnormSrgb, w: 8, h: 4, mips: 2, data: vec![7; 48] };
        write(&path, &p);
        assert_eq!(read(&path), Some(p));
        let _ = std::fs::remove_dir_all(dir);
    }
}

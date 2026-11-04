//! The Maker Pen palette (`ShapeColor` ids to colours) and materials (`ShapeMaterial` ids to textures), which colour
//! the room pieces drawn with `AG/MakerPenShape`.
//!
//! The numbers come from `rooms/shape_palette.txt` (written by `examples/shape_palette.rs`). The 2026 install ships
//! the material textures only in its scrambled files, so they are read from an older install with plain files
//! (`OLD_BUILD_DIRS`, Unity 2021.3, no type trees); without one, the pieces get their palette colour untextured.
use std::{collections::HashMap, path::{Path, PathBuf}};

use unity_assets::{classes, texture, SerializedFile};

const PALETTE: &str = include_str!("../rooms/shape_palette.txt");

/// `RecRoom_Data` folders of older installs to take the Maker Pen textures from.
pub fn old_build_dirs() -> Vec<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|l| vec![PathBuf::from(l).join(r"Programs\MeowNet\RecRoom_Data")])
        .unwrap_or_default()
}

pub struct ShapeMaterial {
    pub name: String,
    /// metres one texture repeat covers (ShapeMaterialInfo.DefaultTextureSize)
    pub size: f32,
    pub smoothness: f32,
    pub base_map: String,
    pub spec_map: String,
}

/// width, height, RGBA8 rows top-down
pub type Pixels = (u32, u32, Vec<u8>);

pub struct ShapeKit {
    colors: HashMap<i32, [f32; 3]>,
    materials: HashMap<i32, ShapeMaterial>,
    old: Option<(PathBuf, SerializedFile, HashMap<String, i64>)>,
    textures: HashMap<String, Option<Pixels>>,
}

impl ShapeKit {
    pub fn new(old_build: Option<&Path>) -> Self {
        let mut colors = HashMap::new();
        let mut materials = HashMap::new();
        for line in PALETTE.lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|x| x.parse::<f32>().ok()).unwrap_or(1.0);
            let Some(id) = f.get(1).and_then(|x| x.parse::<i32>().ok()) else { continue };
            match f[0] {
                "color" => {
                    colors.insert(id, [num(2), num(3), num(4)]);
                }
                "material" if f.len() >= 8 => {
                    materials.insert(id, ShapeMaterial {
                        name: f[7..].join(" "),
                        size: num(2),
                        smoothness: num(4),
                        base_map: f[5].to_string(),
                        spec_map: f[6].to_string(),
                    });
                }
                _ => {}
            }
        }
        let old = old_build.and_then(|dir| {
            let sf = SerializedFile::parse(std::fs::read(dir.join("resources.assets")).ok()?).ok()?;
            let names = sf.objects.iter()
                .filter(|o| o.class_id == classes::TEXTURE_2D)
                .filter_map(|o| Some((sf.object_name(o)?, o.path_id)))
                .collect();
            Some((dir.to_path_buf(), sf, names))
        });
        ShapeKit { colors, materials, old, textures: HashMap::new() }
    }

    /// The palette's base colour (sRGB) for a `ShapeColor` id.
    pub fn color(&self, id: i32) -> Option<[f32; 3]> {
        self.colors.get(&id).copied()
    }

    pub fn material(&self, id: i32) -> Option<&ShapeMaterial> {
        self.materials.get(&id)
    }

    pub fn has_textures(&self) -> bool {
        self.old.is_some()
    }

    /// A texture of the older install by name, decoded once.
    pub fn texture(&mut self, name: &str) -> Option<&Pixels> {
        if !self.textures.contains_key(name) {
            let t = self.old.as_ref().and_then(|(dir, sf, names)| read_texture(dir, sf, *names.get(name)?));
            self.textures.insert(name.to_string(), t);
        }
        self.textures[name].as_ref()
    }
}

/// A 2021.3 Texture2D without type tree: m_Name, fallback format, two flags, width, height, image size,
/// mips stripped, format, ...; the pixels are inline or in the `.resS` file named by the trailing StreamData.
fn read_texture(dir: &Path, sf: &SerializedFile, path_id: i64) -> Option<Pixels> {
    let b = sf.object_bytes(sf.object(path_id)?);
    let i32_at = |p: usize| Some(i32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?));
    let name_end = (4 + i32_at(0)? as usize + 3) & !3;
    let (w, h) = (i32_at(name_end + 8)? as usize, i32_at(name_end + 12)? as usize);
    let size = i32_at(name_end + 16)? as usize;
    let format = i32_at(name_end + 24)?;
    // StreamData {u64 offset, u32 size, string path} ends the object
    let res = b"resources.assets.resS";
    let data = match b.windows(res.len()).rposition(|w| w == res) {
        Some(at) => {
            let off = u64::from_le_bytes(b.get(at - 16..at - 8)?.try_into().ok()?);
            let len = i32_at(at - 8)? as usize;
            let mut f = std::fs::File::open(dir.join("resources.assets.resS")).ok()?;
            use std::io::{Read, Seek, SeekFrom};
            f.seek(SeekFrom::Start(off)).ok()?;
            let mut v = vec![0; len];
            f.read_exact(&mut v).ok()?;
            v
        }
        None => b.get(b.len().checked_sub(16 + size)?..b.len() - 16)?.to_vec(),
    };
    let mip0 = texture::mip0_size(format, w, h)?;
    let px = texture::decode_rgba8(format, w, h, data.get(..mip0)?).ok()?;
    Some((w as u32, h as u32, px))
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// The Recolor / Maker Pen albedo, baked: `rgb * lerp(1, tint, a)` in linear light, with alpha (the recolour mask)
/// made opaque. `tint` is linear.
pub fn bake_tint(src: &Pixels, tint: [f32; 3]) -> Pixels {
    let lin: Vec<f32> = (0..256).map(|i| to_linear(i as f32 / 255.0)).collect();
    let mut px = src.2.clone();
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            let v = lin[p[c] as usize] * (1.0 + (tint[c] - 1.0) * a);
            p[c] = (to_srgb(v.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
        }
        p[3] = 255;
    }
    (src.0, src.1, px)
}

/// sRGB colour to linear.
pub fn linear(c: [f32; 3]) -> [f32; 3] {
    c.map(to_linear)
}

/// The mean of a texture's alpha (a spec-gloss map's gloss), 0..1.
pub fn mean_alpha(t: &Pixels) -> f32 {
    let n = t.2.len() / 4;
    if n == 0 {
        return 1.0;
    }
    t.2.chunks_exact(4).map(|p| p[3] as u64).sum::<u64>() as f32 / (n as f32 * 255.0)
}

/// The AG shaders' emission colour, baked: `rgb * a * tint` (linear light), stored sRGB and opaque; the material
/// multiplies it by `_EmissivePower`.
pub fn bake_emission(src: &Pixels, tint: [f32; 3]) -> Pixels {
    let lin: Vec<f32> = (0..256).map(|i| to_linear(i as f32 / 255.0)).collect();
    let mut px = src.2.clone();
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            p[c] = (to_srgb((lin[p[c] as usize] * a * tint[c]).clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
        }
        p[3] = 255;
    }
    (src.0, src.1, px)
}

/// A spec-gloss map turned into Bevy's metallic-roughness layout: green = 1 - alpha * smoothness, blue (metallic) 0.
pub fn roughness_from_gloss(src: &Pixels, smoothness: f32) -> Pixels {
    let mut px = src.2.clone();
    for p in px.chunks_exact_mut(4) {
        let g = 1.0 - p[3] as f32 / 255.0 * smoothness;
        p.copy_from_slice(&[0, (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8, 0, 255]);
    }
    (src.0, src.1, px)
}

/// The mean over a spec map of its brightest channel (as stored, the map is linear), 0..1.
pub fn mean_rgb_max(t: &Pixels) -> f32 {
    let n = t.2.len() / 4;
    if n == 0 {
        return 0.0;
    }
    t.2.chunks_exact(4).map(|p| p[0].max(p[1]).max(p[2]) as u64).sum::<u64>() as f32 / (n as f32 * 255.0)
}

/// A Unity normal map as full RGB normals. Unity packs them two ways: DXT5nm (x in alpha, y in green, red all
/// white) and plain RG(B); z is rebuilt from x and y.
pub fn unpack_normals(src: &Pixels) -> Pixels {
    let n = (src.2.len() / 4).max(1);
    let red_white = src.2.chunks_exact(4).filter(|p| p[0] >= 250).count() * 10 >= n * 9;
    let mut px = src.2.clone();
    for p in px.chunks_exact_mut(4) {
        let x = if red_white { p[3] } else { p[0] } as f32 / 127.5 - 1.0;
        let y = p[1] as f32 / 127.5 - 1.0;
        let z = (1.0 - x * x - y * y).max(0.0).sqrt();
        p.copy_from_slice(&[p_byte(x), p_byte(y), p_byte(z), 255]);
    }
    (src.0, src.1, px)
}

fn p_byte(v: f32) -> u8 {
    ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

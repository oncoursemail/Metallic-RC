//! Writes `rooms/shape_palette.txt`: the Maker Pen colours and materials from the ShapeConfig ScriptableObject.
//!
//! `cargo run --release -p metallic-rc-client --example shape_palette -- <RecRoom_Data dir> > rooms/shape_palette.txt`
//! The 2026 build ships ShapeConfig only in its scrambled files, so this reads an older build whose
//! `resources.assets` is plain (Unity 2021.3, no type trees). Field layout (rendering.md): `ShapeColorInfos[]`
//! {name, ColorID, baseColor, emissiveColor, specularColor}, then `ShapeMaterialInfos[]` {DisplayName, ColorUsage,
//! DefaultColor, DefaultTextureSize, uvOffsetStart, tiling, ShapeMaterial, Material}.
use unity_assets::SerializedFile;

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl R<'_> {
    fn i32(&mut self) -> i32 {
        let v = i32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        v
    }
    fn i64(&mut self) -> i64 {
        let v = i64::from_le_bytes(self.b[self.p..self.p + 8].try_into().unwrap());
        self.p += 8;
        v
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.i32() as u32)
    }
    fn color(&mut self) -> [f32; 4] {
        [self.f32(), self.f32(), self.f32(), self.f32()]
    }
    fn align(&mut self) {
        self.p = (self.p + 3) & !3;
    }
    fn string(&mut self) -> String {
        // clamped, so a search can try any offset
        let n = (self.i32().max(0) as usize).min(self.b.len() - self.p);
        let s = String::from_utf8_lossy(&self.b[self.p..self.p + n]).into_owned();
        self.p += n;
        self.align();
        s
    }
}

/// Offset of the first `count, name` pair whose name passes `ok`.
fn find_list(b: &[u8], from: usize, ok: impl Fn(&str) -> bool) -> Option<usize> {
    (from..b.len().saturating_sub(16)).find(|&p| {
        let mut t = R { b, p };
        (1..100).contains(&t.i32()) && ok(&t.string())
    })
}

/// A 2021.3 Material's texture names by property, and its float properties.
fn material(sf: &SerializedFile, b: &[u8]) -> (Vec<(String, String)>, Vec<(String, f32)>) {
    let mut r = R { b, p: 0 };
    r.string();
    r.i32();
    r.i64();
    // past the keyword lists (upper-case names) to the tex envs (mixed-case property names)
    let Some(at) = find_list(b, r.p, |s| s.starts_with('_') && s.chars().any(|c| c.is_ascii_lowercase())) else {
        return Default::default();
    };
    r.p = at;
    let mut textures = Vec::new();
    for _ in 0..r.i32() {
        let k = r.string();
        let (tf, tp) = (r.i32(), r.i64());
        r.p += 16;
        if tf == 0 && tp != 0 {
            if let Some(o) = sf.object(tp) {
                textures.push((k, R { b: sf.object_bytes(o), p: 0 }.string()));
            }
        }
    }
    for _ in 0..r.i32() {
        r.string();
        r.i32();
    }
    let floats = (0..r.i32()).map(|_| (r.string(), r.f32())).collect();
    (textures, floats)
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: shape_palette <RecRoom_Data dir>"));
    let ggm = SerializedFile::parse(std::fs::read(dir.join("globalgamemanagers.assets")).unwrap()).unwrap();
    let res = SerializedFile::parse(std::fs::read(dir.join("resources.assets")).unwrap()).unwrap();
    let script = ggm.objects.iter()
        .filter(|o| o.class_id == 115)
        .find(|o| R { b: ggm.object_bytes(o), p: 0 }.string() == "ShapeConfig")
        .expect("no ShapeConfig MonoScript")
        .path_id;
    let (b, mut r) = res.objects.iter().filter(|o| o.class_id == 114).find_map(|o| {
        let b = res.object_bytes(o);
        let mut r = R { b, p: 0 };
        r.i32();
        r.i64();
        r.p += 1;
        r.align();
        r.i32();
        (r.i64() == script).then_some((b, r))
    }).expect("no ShapeConfig asset");
    r.string();

    println!("# Maker Pen palette from the ShapeConfig asset, written by examples/shape_palette.rs.");
    println!("# color <ShapeColor id> <baseColor sRGB r g b> <name>");
    println!("# material <ShapeMaterial id> <DefaultTextureSize> <material _BaseTextureSize> <_Smoothness> <_BaseMap texture> <_SpecGlossMap texture> <name>");
    // the shape list comes first; the colour list is found by its first name
    r.p = find_list(b, r.p, |s| s == "Salmon").expect("no colour list");
    for _ in 0..r.i32() {
        let name = r.string();
        let id = r.i32();
        let base = r.color();
        r.p += 32;
        println!("color {id} {:.6} {:.6} {:.6} {name}", base[0], base[1], base[2]);
    }
    for _ in 0..r.i32() {
        let name = r.string();
        let (_usage, _color, size, _uv_start) = (r.i32(), r.i32(), r.f32(), r.i32());
        r.p += 8;
        let (id, mf, mp) = (r.i32(), r.i32(), r.i64());
        let (textures, floats) = match (mf, res.object(mp)) {
            (0, Some(o)) => material(&res, res.object_bytes(o)),
            _ => Default::default(),
        };
        let tex = |k: &str| textures.iter().find(|t| t.0 == k).map_or("-".to_string(), |t| t.1.clone());
        let float = |k: &str| floats.iter().find(|f| f.0 == k).map_or(1.0, |f| f.1);
        println!("material {id} {size} {} {} {} {} {name}", float("_BaseTextureSize"), float("_Smoothness"), tex("_BaseMap"), tex("_SpecGlossMap"));
    }
}

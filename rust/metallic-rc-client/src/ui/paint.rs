//! What a laid-out page looks like: a tree of boxes with their fills, sprites and texts, ready for any renderer.
//! Sprites come out of the game's sprite atlases, fonts are the TTFs behind its TextMeshPro font assets, and text is
//! measured with their real glyph advances so the layout pass sizes labels as the game does.
use std::collections::HashMap;
use std::sync::Arc;

use unity_assets::{PPtr, Texture2D, Value};

use super::input::{Interact, Kind, Tints};
use super::layout::{lay_out, Measure, Placed};
use super::prefab::{Component, Element};
use crate::room::{value, FileRef, Library};

/// A sprite cut out of its texture or atlas, as RGBA rows from the top.
pub struct Sprite {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// nine-slice borders in pixels: left, bottom, right, top
    pub border: [f32; 4],
    pub pixels_per_unit: f32,
}

/// A TextMeshPro font asset's face, with the TTF it was made from.
pub struct Font {
    pub name: String,
    pub ttf: Arc<Vec<u8>>,
    pub point_size: f32,
    pub line_height: f32,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Start,
    Middle,
    End,
}

pub enum Paint {
    /// An Image: a colour, optionally a sprite (nine-sliced when `sliced`), a gradient across it and rounded corners.
    Fill {
        color: [f32; 4],
        sprite: Option<Arc<Sprite>>,
        sliced: bool,
        preserve_aspect: bool,
        /// colour stops left to right, already multiplied by `color`
        gradient: Vec<([f32; 4], f32)>,
        /// top left, top right, bottom right, bottom left
        radius: [f32; 4],
        /// the Image component's id, which selectables tint
        graphic: i64,
    },
    /// A UITiler's pattern: the sprite repeated in `rows` rows over the box, tinted.
    Tiles { sprite: Arc<Sprite>, tint: [f32; 4], rows: f32 },
    Text {
        text: String,
        font: Option<Arc<Font>>,
        size: f32,
        color: [f32; 4],
        horizontal: Align,
        vertical: Align,
        wrap: bool,
        /// left, top, right, bottom insets
        margin: [f32; 4],
        /// the text component's id
        graphic: i64,
    },
}

/// One element as drawn, with its children.
pub struct Drawn {
    pub name: String,
    /// the element's RectTransform id
    pub transform: i64,
    pub rect: Placed,
    /// children are cut to this box (Mask, RectMask2D), for drawing and for the pointer
    pub clip: bool,
    /// it has a graphic the pointer hits (`raycastTarget`), drawn or not
    pub raycast: bool,
    /// a selectable on it
    pub interact: Option<Interact>,
    pub paint: Vec<Paint>,
    pub children: Vec<Drawn>,
}

/// Sprites and fonts read from the install, each once.
pub struct Looks<'a> {
    lib: &'a mut Library,
    sprites: HashMap<(String, i64), Option<Arc<Sprite>>>,
    textures: HashMap<(String, i64), Option<Arc<(usize, usize, Vec<u8>)>>>,
    fonts: HashMap<(String, i64), Option<Arc<Font>>>,
    /// per-paint sprite overrides by Image component id: for content the game assigns at runtime (e.g. the customize
    /// category icons, whose prefab sprite is null). Cleared after each page.
    overrides: HashMap<i64, Arc<Sprite>>,
}

/// Loads an exported RRUI icon PNG (white-on-transparent) as a sprite, baking `tint` into the RGB so it shows on a
/// light chip; keeps the icon's own alpha. Used to place the real customize-category icons the prefab leaves null.
pub fn load_icon_sprite(path: &std::path::Path, tint: [u8; 3]) -> Option<Arc<Sprite>> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgba = img.into_raw();
    for px in rgba.chunks_exact_mut(4) {
        px[0] = tint[0];
        px[1] = tint[1];
        px[2] = tint[2];
    }
    Some(Arc::new(Sprite { name: "icon".into(), width: w, height: h, rgba, border: [0.0; 4], pixels_per_unit: 100.0 }))
}

/// Loads a full-colour sprite PNG (keeping its own RGB, unlike `load_icon_sprite` which recolours it) — for a logo or
/// any coloured image placed on a UI element by filename.
pub fn load_sprite_png(path: &std::path::Path) -> Option<Arc<Sprite>> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(Arc::new(Sprite { name: "img".into(), width: w, height: h, rgba: img.into_raw(), border: [0.0; 4], pixels_per_unit: 100.0 }))
}

fn color(v: Option<&Value>) -> Option<[f32; 4]> {
    let f = v?.floats();
    (f.len() == 4).then(|| [f[0], f[1], f[2], f[3]])
}

fn rect(v: Option<&Value>) -> [f32; 4] {
    let f = v.map(Value::floats).unwrap_or_default();
    [0, 1, 2, 3].map(|i| f.get(i).copied().unwrap_or(0.0))
}

fn pptr(v: &Value, name: &str) -> Option<PPtr> {
    v.get(name).and_then(Value::as_pptr).filter(|p| !p.is_null())
}

/// `<b>`, `<color=..>` and the other rich text tags are left out for now; the text between them stays.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out
}

impl<'a> Looks<'a> {
    pub fn new(lib: &'a mut Library) -> Self {
        Looks { lib, sprites: HashMap::new(), textures: HashMap::new(), fonts: HashMap::new(), overrides: HashMap::new() }
    }

    /// Sets a sprite override for an Image component id (used before painting a page, cleared by `clear_overrides`).
    pub fn override_sprite(&mut self, id: i64, sprite: Arc<Sprite>) {
        self.overrides.insert(id, sprite);
    }

    pub fn clear_overrides(&mut self) {
        self.overrides.clear();
    }

    /// The library it reads from, for loading more pages while keeping what it has read.
    pub fn library(&mut self) -> &mut Library {
        self.lib
    }

    fn texture(&mut self, from: &FileRef, p: PPtr) -> Option<Arc<(usize, usize, Vec<u8>)>> {
        let (tf, id) = self.lib.resolve(from, p).ok()?;
        let key = (tf.cab().to_string(), id);
        if let Some(t) = self.textures.get(&key) {
            return t.clone();
        }
        let t = value(&tf, id).ok().and_then(|v| Texture2D::from_value(&v, &tf.open.bundle).ok())
            .and_then(|t| Some(Arc::new((t.width, t.height, t.decode_rgba8().ok()?))));
        self.textures.insert(key, t.clone());
        t
    }

    /// The sprite an Image shows. A per-paint override (set for runtime-assigned graphics like the customize icons)
    /// wins over the component's own `m_Sprite`.
    pub fn sprite(&mut self, c: &Component) -> Option<Arc<Sprite>> {
        if let Some(s) = self.overrides.get(&c.id) {
            return Some(s.clone());
        }
        let (file, p) = (c.file.as_ref()?, pptr(&c.value, "m_Sprite")?);
        self.sprite_at(file, p)
    }

    fn sprite_at(&mut self, file: &FileRef, p: PPtr) -> Option<Arc<Sprite>> {
        let (sf, id) = self.lib.resolve(file, p).ok()?;
        let key = (sf.cab().to_string(), id);
        if let Some(s) = self.sprites.get(&key) {
            return s.clone();
        }
        let s = self.read_sprite(&sf, id).map(Arc::new);
        self.sprites.insert(key, s.clone());
        s
    }

    fn read_sprite(&mut self, sf: &FileRef, id: i64) -> Option<Sprite> {
        let v = value(sf, id).ok()?;
        let [_, _, w, h] = rect(v.get("m_Rect"));
        let rd = v.get("m_RD")?;
        // packed sprites keep their pixels in the atlas, under the sprite's render data key
        let (from, data) = match pptr(rd, "texture") {
            Some(_) => (sf.clone(), rd.clone()),
            None => {
                let (af, aid) = self.lib.resolve(sf, pptr(&v, "m_SpriteAtlas")?).ok()?;
                let atlas = value(&af, aid).ok()?;
                let key = v.get("m_RenderDataKey")?;
                let data = atlas.get("m_RenderDataMap")?.pairs().into_iter().find(|(k, _)| *k == key)?.1.clone();
                (af, data)
            }
        };
        let tex = self.texture(&from, pptr(&data, "texture")?)?;
        let (tw, th, px) = (tex.0, tex.1, &tex.2);
        let [tx, ty, cw, ch] = rect(data.get("textureRect"));
        let [ox, oy, _, _] = rect(data.get("textureRectOffset"));
        let (w, h) = (w.round().max(1.0) as usize, h.round().max(1.0) as usize);
        let mut rgba = vec![0u8; w * h * 4];
        // sprite rects count rows from the bottom; decoded textures and our sprites run top down
        let (tx, ty, ox, oy) = (tx.round() as i64, ty.round() as i64, ox.round() as i64, oy.round() as i64);
        for y in 0..ch.round() as i64 {
            let (src_y, dst_y) = (ty + y, oy + y);
            if src_y < 0 || src_y >= th as i64 || dst_y < 0 || dst_y >= h as i64 {
                continue;
            }
            for x in 0..cw.round() as i64 {
                let (src_x, dst_x) = (tx + x, ox + x);
                if src_x < 0 || src_x >= tw as i64 || dst_x < 0 || dst_x >= w as i64 {
                    continue;
                }
                let s = ((th - 1 - src_y as usize) * tw + src_x as usize) * 4;
                let d = ((h - 1 - dst_y as usize) * w + dst_x as usize) * 4;
                rgba[d..d + 4].copy_from_slice(&px[s..s + 4]);
            }
        }
        Some(Sprite {
            name: v.get("m_Name").and_then(Value::as_str).unwrap_or("").into(),
            width: w as u32,
            height: h as u32,
            rgba,
            border: rect(v.get("m_Border")),
            pixels_per_unit: v.get("m_PixelsToUnits").and_then(Value::as_f32).unwrap_or(100.0),
        })
    }

    /// The font a TextMeshPro text uses (its own TTF, else the first fallback that has one).
    pub fn font(&mut self, c: &Component) -> Option<Arc<Font>> {
        if let Some((file, p)) = &c.themed.font {
            return self.font_asset(file, *p, 0);
        }
        let (file, p) = (c.file.as_ref()?, pptr(&c.value, "m_fontAsset")?);
        self.font_asset(file, p, 0)
    }

    fn font_asset(&mut self, from: &FileRef, p: PPtr, depth: usize) -> Option<Arc<Font>> {
        let (af, id) = self.lib.resolve(from, p).ok()?;
        let key = (af.cab().to_string(), id);
        if let Some(f) = self.fonts.get(&key) {
            return f.clone();
        }
        let v = value(&af, id).ok()?;
        let face = v.get("m_FaceInfo");
        let num = |n: &str| face.and_then(|f| f.get(n)).and_then(Value::as_f32).unwrap_or(0.0);
        let ttf = pptr(&v, "m_SourceFontFile").and_then(|s| self.lib.resolve(&af, s).ok())
            .and_then(|(ff, fid)| value(&ff, fid).ok()?.get("m_FontData")?.as_bytes().map(<[u8]>::to_vec))
            .filter(|b| !b.is_empty());
        let font = match ttf {
            Some(ttf) => Some(Arc::new(Font {
                name: v.get("m_Name").and_then(Value::as_str).unwrap_or("").into(),
                ttf: Arc::new(ttf),
                point_size: num("m_PointSize").max(1.0),
                line_height: num("m_LineHeight"),
                ascent: num("m_AscentLine"),
                descent: num("m_DescentLine"),
            })),
            None if depth < 4 => v.get("m_FallbackFontAssetTable").and_then(Value::as_array).unwrap_or_default().iter()
                .filter_map(Value::as_pptr).filter(|p| !p.is_null())
                .find_map(|p| self.font_asset(&af, p, depth + 1)),
            None => None,
        };
        self.fonts.insert(key, font.clone());
        font
    }

    /// Width of one line of `text` at `size`.
    fn line_width(&mut self, font: &Font, text: &str, size: f32) -> f32 {
        // parsing only reads the table directory, so it is done per call
        let Ok(face) = ttf_parser::Face::parse(&font.ttf, 0) else { return text.chars().count() as f32 * size * 0.5 };
        let scale = size / face.units_per_em() as f32;
        text.chars().map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map_or(size * 0.5, |a| a as f32 * scale)).sum()
    }

    /// Lines of `text` wrapped to `width`, word by word.
    fn wrap(&mut self, font: &Font, text: &str, size: f32, width: Option<f32>) -> Vec<f32> {
        let mut lines = vec![];
        for para in text.split('\n') {
            let Some(width) = width else {
                lines.push(self.line_width(font, para, size));
                continue;
            };
            let mut line = String::new();
            for word in para.split(' ') {
                let tried = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if !line.is_empty() && self.line_width(font, &tried, size) > width + 0.01 {
                    lines.push(self.line_width(font, &line, size));
                    line = word.to_string();
                } else {
                    line = tried;
                }
            }
            lines.push(self.line_width(font, &line, size));
        }
        lines
    }
}

fn text_settings(c: &Component) -> (String, f32, bool, [f32; 4]) {
    let v = &c.value;
    let text = plain(v.get("m_text").and_then(Value::as_str).unwrap_or(""));
    let auto = v.get("m_enableAutoSizing").and_then(Value::as_bool).unwrap_or(false);
    let size = if auto { "m_fontSizeMax" } else { "m_fontSize" };
    let size = c.themed.font_size.or_else(|| v.get(size).and_then(Value::as_f32)).unwrap_or(36.0);
    let wrap = match v.get("m_TextWrappingMode").and_then(Value::as_i64) {
        Some(m) => m != 0,
        None => v.get("m_enableWordWrapping").and_then(Value::as_bool).unwrap_or(true),
    };
    // TMP margins: left, top, right, bottom
    (text, size, wrap, rect(v.get("m_margin")))
}

impl Measure for Looks<'_> {
    fn image(&mut self, c: &Component) -> Option<[f32; 2]> {
        let s = self.sprite(c)?;
        let multiplier = c.value.get("m_PixelsPerUnitMultiplier").and_then(Value::as_f32).unwrap_or(1.0).max(0.01);
        // Image.pixelsPerUnit: the sprite's, over the canvas reference of 100
        let ppu = s.pixels_per_unit / 100.0 * multiplier;
        let sliced = matches!(c.value.get("m_Type").and_then(Value::as_i64), Some(1 | 2));
        Some(if sliced {
            [(s.border[0] + s.border[2]) / ppu, (s.border[1] + s.border[3]) / ppu]
        } else {
            [s.width as f32 / ppu, s.height as f32 / ppu]
        })
    }

    fn text(&mut self, c: &Component, width: Option<f32>) -> [f32; 2] {
        let (text, size, wrap, m) = text_settings(c);
        let Some(font) = self.font(c) else { return super::layout::Rough.text(c, width) };
        let width = width.filter(|_| wrap).map(|w| (w - m[0] - m[2]).max(0.0));
        let lines = self.wrap(&font, &text, size, width);
        let k = size / font.point_size;
        let w = lines.iter().copied().fold(0.0, f32::max) + m[0] + m[2];
        let h = (font.ascent - font.descent) * k + (lines.len().max(1) - 1) as f32 * font.line_height * k + m[1] + m[3];
        [w, h]
    }
}

const IMAGE: &str = "UnityEngine.UI.Image";
const TEXT: &str = "TMPro.TextMeshProUGUI";

/// Lays `root` out on `canvas` and returns what to draw.
pub fn page(lib: &mut Library, root: &Element, canvas: [f32; 2]) -> Drawn {
    page_with(&mut Looks::new(lib), root, canvas)
}

/// As [`page`], reusing the sprites and fonts `looks` has read.
pub fn page_with(looks: &mut Looks, root: &Element, canvas: [f32; 2]) -> Drawn {
    let laid = lay_out(root, canvas, looks);
    let placed: HashMap<usize, Placed> = laid.boxes.iter().map(|(i, _, p)| (*i, *p)).collect();
    // a slider's drag box is the parent of its handle (else of its fill): transform id -> the parent's place
    let mut parents = HashMap::new();
    let mut index = 0;
    fn go(e: &Element, parent: Option<usize>, index: &mut usize, placed: &HashMap<usize, Placed>, out: &mut HashMap<i64, Placed>) {
        let mine = *index;
        *index += 1;
        if let Some(r) = parent.and_then(|p| placed.get(&p)) {
            out.insert(e.transform, *r);
        }
        for c in &e.children {
            go(c, Some(mine), index, placed, out);
        }
    }
    go(root, None, &mut index, &placed, &mut parents);
    let mut index = 0;
    let at = Places { placed: &placed, parents: &parents };
    draw(looks, root, &at, &mut index).unwrap_or(Drawn { name: root.name.clone(), transform: root.transform, rect: Placed::default(), clip: false, raycast: false, interact: None, paint: vec![], children: vec![] })
}

struct Places<'a> {
    placed: &'a HashMap<usize, Placed>,
    /// RectTransform id -> its parent's place
    parents: &'a HashMap<i64, Placed>,
}

/// Classes that are uGUI Selectables (`m_Colors`, `m_TargetGraphic`, `m_Interactable`).
fn selectable(c: &Component) -> bool {
    c.value.get("m_Colors").is_some() && c.value.get("m_Interactable").is_some()
}

fn interact(c: &Component, at: &Places) -> Interact {
    let v = &c.value;
    let (bg, fg) = c.themed.tints.unwrap_or((None, None));
    let tinted = v.get("m_Transition").and_then(Value::as_i64) == Some(1);
    let target = pptr(v, "m_TargetGraphic").filter(|p| p.file_id == 0 && tinted)
        .and_then(|p| Some((p.path_id, bg.or_else(|| v.get("m_Colors").and_then(Tints::read))?)));
    let extra = v.get("additionalGraphics").and_then(Value::as_array).unwrap_or_default().iter()
        .filter_map(Value::as_pptr).filter(|p| p.file_id == 0 && !p.is_null()).map(|p| p.path_id).collect();
    let extra_tints = fg.or_else(|| v.get("additionalGraphicsColors").and_then(Tints::read));
    let num = |n: &str| v.get(n).and_then(Value::as_f32);
    let kind = match (num("m_MinValue"), num("m_MaxValue")) {
        (Some(min), Some(max)) => {
            let track = ["m_HandleRect", "m_FillRect"].iter().filter_map(|n| pptr(v, n)).find_map(|p| at.parents.get(&p.path_id).copied());
            match track {
                Some(track) => Kind::Slider { track, min, max, whole: v.get("m_WholeNumbers").and_then(Value::as_bool).unwrap_or(false) },
                None => Kind::Button,
            }
        }
        _ => Kind::Button,
    };
    Interact { id: c.id, interactable: v.get("m_Interactable").and_then(Value::as_bool).unwrap_or(true), target, extra, extra_tints, kind }
}

fn raycast(c: &Component) -> bool {
    c.value.get("m_RaycastTarget").and_then(Value::as_bool).unwrap_or(false)
}

fn draw(looks: &mut Looks, e: &Element, at: &Places, index: &mut usize) -> Option<Drawn> {
    let mine = *index;
    *index += 1;
    let Some(&rect) = at.placed.get(&mine) else {
        *index += e.walk().len() - 1;
        return None;
    };
    let mut paint = vec![];
    let mask = e.component("UnityEngine.UI.Mask");
    let hide_graphic = mask.is_some_and(|m| !m.value.get("m_ShowMaskGraphic").and_then(Value::as_bool).unwrap_or(true));
    if let Some(c) = e.component(IMAGE).filter(|_| !hide_graphic) {
        let mut base = c.themed.color.or_else(|| color(c.value.get("m_Color"))).unwrap_or([1.0; 4]);
        let sprite = looks.sprite(c);
        // a tiler draws its own background, with the pattern over it
        let tiles = e.component("RRUI.UITiler").and_then(|t| {
            let (file, sprite, tint, background, rows) = match &t.themed.tiler {
                Some((f, s, tint, bg, rows)) => (f.clone(), *s, *tint, *bg, *rows),
                None => (t.file.clone()?, pptr(&t.value, "sprite")?, color(t.value.get("tintColor"))?,
                    color(t.value.get("backgroundColor"))?, t.value.get("rows").and_then(Value::as_f32).unwrap_or(5.0)),
            };
            base = background;
            Some(Paint::Tiles { sprite: looks.sprite_at(&file, sprite)?, tint, rows })
        });
        let gradient = e.component("RRUI.UIGradient")
            .and_then(|g| g.themed.gradient.as_ref().or_else(|| g.value.get("gradient")))
            .map(|g| gradient(g, base)).unwrap_or_default();
        let radius = e.component("RRUI.RoundedCorners").map_or([0.0; 4], |r| {
            let v = &r.value;
            let radius = v.get("radius").and_then(Value::as_f32).unwrap_or(0.0);
            let on = |n: &str| v.get("customCorners").and_then(|c| c.get(n)).and_then(Value::as_bool).unwrap_or(true);
            ["topLeft", "topRight", "bottomRight", "bottomLeft"].map(|n| if on(n) { radius } else { 0.0 })
        });
        if base[3] > 0.0 || gradient.iter().any(|(c, _)| c[3] > 0.0) {
            paint.push(Paint::Fill {
                color: base,
                sliced: sprite.is_some() && matches!(c.value.get("m_Type").and_then(Value::as_i64), Some(1)),
                preserve_aspect: c.value.get("m_PreserveAspect").and_then(Value::as_bool).unwrap_or(false),
                sprite,
                gradient,
                radius,
                graphic: c.id,
            });
        }
        paint.extend(tiles);
    }
    if let Some(c) = e.component(TEXT) {
        let (mut text, mut size, mut wrap, margin) = text_settings(c);
        let font = looks.font(c);
        if let (Some(f), true) = (&font, c.value.get("m_enableAutoSizing").and_then(Value::as_bool).unwrap_or(false)) {
            // shrink until it fits, as TMP's auto size does
            let min = c.value.get("m_fontSizeMin").and_then(Value::as_f32).unwrap_or(10.0);
            let width = (rect.w - margin[0] - margin[2]).max(0.0);
            while size > min {
                let lines = looks.wrap(f, &text, size, wrap.then_some(width));
                let k = size / f.point_size;
                let h = (f.ascent - f.descent) * k + (lines.len() - 1) as f32 * f.line_height * k;
                if lines.iter().all(|w| *w <= width + 0.01) && h <= rect.h - margin[1] - margin[3] + 0.01 {
                    break;
                }
                size = (size - 1.0).max(min);
            }
        }
        if let Some(f) = &font {
            (text, wrap) = fit(looks, f, text, size, wrap, &rect, margin, c.value.get("m_overflowMode").and_then(Value::as_i64).unwrap_or(0));
        }
        let v = &c.value;
        let (h, vert) = match (v.get("m_HorizontalAlignment").and_then(Value::as_i64), v.get("m_VerticalAlignment").and_then(Value::as_i64)) {
            (Some(h), Some(vv)) => (h, vv),
            _ => {
                let a = v.get("m_textAlignment").and_then(Value::as_i64).unwrap_or(257);
                (a & 0xff, a & 0xff00)
            }
        };
        paint.push(Paint::Text {
            text,
            font,
            size,
            color: c.themed.color.or_else(|| color(v.get("m_fontColor"))).unwrap_or([1.0; 4]),
            horizontal: match h { 2 | 32 => Align::Middle, 4 => Align::End, _ => Align::Start },
            vertical: match vert { 512 | 4096 => Align::Middle, 1024 => Align::End, _ => Align::Start },
            wrap,
            margin,
            graphic: c.id,
        });
    }
    let clip = mask.is_some() || e.has("UnityEngine.UI.RectMask2D");
    let raycast = [IMAGE, TEXT, "UnityEngine.UI.RawImage"].iter().any(|k| e.component(k).is_some_and(raycast));
    let interact = e.components.iter().find(|c| c.enabled && selectable(c)).map(|c| interact(c, at));
    let children = e.children.iter().filter_map(|c| draw(looks, c, at, index)).collect();
    Some(Drawn { name: e.name.clone(), transform: e.transform, rect, clip, raycast, interact, paint, children })
}

/// TMP's overflow modes, as far as the box goes: text that fits on one line is drawn on one line (so the renderer's
/// own measure does not wrap a label sized to it), and Ellipsis (1) or Truncate (3) cut what does not fit in the box,
/// Ellipsis ending the cut with "...". Other modes let the text run over.
#[allow(clippy::too_many_arguments)]
fn fit(looks: &mut Looks, f: &Font, text: String, size: f32, wrap: bool, rect: &Placed, margin: [f32; 4], overflow: i64) -> (String, bool) {
    let (w, h) = ((rect.w - margin[0] - margin[2]).max(0.0), rect.h - margin[1] - margin[3] + 0.01);
    let k = size / f.point_size;
    let height = |n: usize| (f.ascent - f.descent) * k + n.saturating_sub(1) as f32 * f.line_height * k;
    let fits = |looks: &mut Looks, t: &str| {
        let lines = looks.wrap(f, t, size, wrap.then_some(w));
        lines.iter().all(|x| *x <= w + 0.01) && height(lines.len()) <= h || !wrap && lines.len() == 1 && lines[0] <= w + 0.01
    };
    let one_line = |looks: &mut Looks, t: &str| !t.contains('\n') && looks.wrap(f, t, size, None)[0] <= w + 0.01;
    if !matches!(overflow, 1 | 3) || fits(looks, &text) {
        let single = one_line(looks, &text);
        return (text, wrap && !single);
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut: String = chars.iter().collect::<String>().trim_end().to_string();
        let t = if overflow == 1 { format!("{cut}...") } else { cut };
        if fits(looks, &t) {
            let single = one_line(looks, &t);
            return (t, wrap && !single);
        }
    }
    (String::new(), wrap)
}

/// A Unity Gradient's colour keys times the image colour. INFERRED: UIGradient type 0 runs left to right, as angle 0
/// does in Unity.
fn gradient(g: &Value, base: [f32; 4]) -> Vec<([f32; 4], f32)> {
    let n = g.get("m_NumColorKeys").and_then(Value::as_i64).unwrap_or(0).clamp(0, 8) as usize;
    (0..n).filter_map(|i| {
        let c = color(g.get(&format!("key{i}")))?;
        let t = g.get(&format!("ctime{i}")).and_then(Value::as_f32).unwrap_or(0.0) / 65535.0;
        Some(([c[0] * base[0], c[1] * base[1], c[2] * base[2], c[3] * base[3]], t))
    }).collect()
}

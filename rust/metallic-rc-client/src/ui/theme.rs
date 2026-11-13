//! The palette theme (`RRUI.Theme`): `ThemesConfig` names one palette asset (light and dark are the same one), whose
//! lists hold typed entries (background colours, foreground colours, text styles, text sizes, buttons, toggles,
//! tilers). Theme components on a page pick an entry by its type number and colour the graphics they point at.
//!
//! What a component sets on which graphic is read from the field names (INFERRED): Surface -> image colour and
//! gradient; Label -> text colour, font and size; Button and Toggle (the active or inactive look) -> surface colour,
//! gradient, graphic colours, text font and size, border shown or hidden, and the selectable's state tints; Toggle ->
//! the objects shown only while active or inactive; Switch -> its knob at the on or off position; Tiler -> the tiled
//! pattern.
//!
//! The state tints (INFERRED from data, no method bodies read): a button palette's `backgroundTints` and
//! `foregroundTints` equal, value for value, the `m_Colors` and `additionalGraphicsColors` serialized on the
//! MultiGraphicButtons themed with it (toggle type 17: white surface, white text colour, orange foreground tint), so
//! the theme writes them there, and the text colour seen in game is the foreground colour times that tint.
use std::collections::{HashMap, HashSet};
use std::path::Path;

use unity_assets::{PPtr, Value};

use super::input::Tints;
use super::prefab::{asset_object, Element, Pages, Themed};
use crate::room::{value, FileRef, Library};

pub struct Theme {
    file: FileRef,
    palette: Value,
}

fn pptr(v: &Value, name: &str) -> Option<PPtr> {
    v.get(name).and_then(Value::as_pptr).filter(|p| !p.is_null())
}

fn int(v: &Value, name: &str) -> Option<i64> {
    v.get(name).and_then(Value::as_i64)
}

fn flag(v: &Value, name: &str) -> bool {
    v.get(name).and_then(Value::as_bool).unwrap_or(false)
}

fn rgba(v: Option<&Value>) -> Option<[f32; 4]> {
    let f = v?.floats();
    (f.len() == 4).then(|| [f[0], f[1], f[2], f[3]])
}

/// Puts what `p` sets over `into`.
fn merge(into: &mut Themed, p: Themed) {
    into.color = p.color.or(into.color);
    into.gradient = p.gradient.or(into.gradient.take());
    into.font = p.font.or(into.font.take());
    into.font_size = p.font_size.or(into.font_size);
    into.tiler = p.tiler.or(into.tiler.take());
    into.tints = p.tints.or(into.tints.take());
}

/// What a page's theme components do besides colouring: components whose element is hidden, and knobs to move
/// (element id -> the element whose place it takes).
#[derive(Default)]
struct Effects {
    hide: HashSet<i64>,
    moves: HashMap<i64, i64>,
}

impl Theme {
    /// The theme the install's `ThemesConfig` names.
    pub fn load(lib: &mut Library, pages: &Pages, bundle_dir: &Path) -> Result<Theme, String> {
        let (bundle, asset) = pages.find_typed("ThemesConfig", "RRUI.ThemesConfig").ok_or("no ThemesConfig in the catalog")?;
        let (file, id) = asset_object(lib, bundle_dir, bundle, asset)?;
        let config = value(&file, id)?;
        let light = pptr(&config, "_light").ok_or("ThemesConfig has no theme")?;
        let (file, id) = lib.resolve(&file, light)?;
        let palette = value(&file, id)?;
        Ok(Theme { file, palette })
    }

    /// The entry of `list` whose `kind` field is `n`.
    fn entry(&self, list: &str, kind: &str, n: i64) -> Option<&Value> {
        self.palette.get(list)?.as_array()?.iter().find(|e| int(e, kind) == Some(n))
    }

    fn text_size(&self, n: i64) -> Option<f32> {
        self.entry("textSizePalettes", "textSizePaletteType", n)?.get("size")?.as_f32()
    }

    fn font(&self, style: &Value) -> Option<(FileRef, PPtr)> {
        pptr(style, "font").map(|p| (self.file.clone(), p))
    }

    /// Colours, fonts and sizes from the page's theme components, put on the components they name; every toggle off.
    pub fn apply(&self, root: &mut Element) {
        self.apply_with(root, &HashSet::new());
    }

    /// As [`Theme::apply`], with the toggles whose ToggleTheme component id is in `active` on.
    pub fn apply_with(&self, root: &mut Element, active: &HashSet<i64>) {
        let mut patches: HashMap<i64, Themed> = HashMap::new();
        let mut fx = Effects::default();
        let mut rects = HashMap::new();
        for (_, e) in root.walk() {
            rects.insert(e.id, e.rect);
            for c in e.components.iter().filter(|c| c.enabled) {
                self.patches(&c.class, &c.value, active.contains(&c.id), &mut patches, &mut fx);
            }
        }
        root.each_mut(&mut |e| {
            if e.components.iter().any(|c| fx.hide.contains(&c.id)) {
                e.active = false;
            }
            if let Some(r) = fx.moves.get(&e.id).and_then(|to| rects.get(to)) {
                e.rect.anchored_position = r.anchored_position;
            }
            for c in &mut e.components {
                if let Some(p) = patches.remove(&c.id) {
                    merge(&mut c.themed, p);
                }
            }
        });
    }

    fn patches(&self, class: &str, v: &Value, on: bool, out: &mut HashMap<i64, Themed>, fx: &mut Effects) {
        let mut put = |target: Option<PPtr>, t: Themed| {
            if let Some(p) = target.filter(|p| p.file_id == 0) {
                let mut slot = out.remove(&p.path_id).unwrap_or_default();
                merge(&mut slot, t);
                out.insert(p.path_id, slot);
            }
        };
        // the field holding the target, or its override when the component says to use that
        let pick = |name: &str, use_override: &str, over: &str| if flag(v, use_override) { pptr(v, over).or(pptr(v, name)) } else { pptr(v, name) };
        match class {
            "RRUI.Theme.SurfaceTheme" => {
                let Some(entry) = int(v, "color").and_then(|n| self.entry("backgroundColorPalettes", "backgroundColorPaletteType", n)) else { return };
                put(pptr(v, "image"), Themed { color: rgba(entry.get("color")), ..Default::default() });
                if !flag(v, "ignoreGradient") {
                    put(pptr(v, "gradient"), Themed { gradient: entry.get("gradient").cloned(), ..Default::default() });
                }
            }
            "RRUI.Theme.LabelTheme" => {
                let color = int(v, "color").and_then(|n| self.entry("foregroundColorPalettes", "foregroundColorPaletteType", n)).and_then(|e| rgba(e.get("color")));
                let style = int(v, "textType").and_then(|n| self.entry("textStylePalettes", "textStylePaletteType", n)).and_then(|e| e.get("textStylePalette"));
                let t = Themed { color, font: style.and_then(|s| self.font(s)), font_size: int(v, "textSize").and_then(|n| self.text_size(n)), ..Default::default() };
                put(pptr(v, "textMeshProUGUI").or(pick("graphic", "useGraphicOverride", "graphicOverride")), t);
            }
            "RRUI.Theme.ButtonTheme" | "RRUI.Theme.ToggleTheme" | "RRUI.Theme.SwitchTheme" => {
                let palette = if class.ends_with("ButtonTheme") {
                    int(v, "buttonType").and_then(|n| self.entry("buttonPalettes", "buttonPaletteType", n)).and_then(|e| e.get("buttonPalette"))
                } else {
                    let look = if on { "togglePalette/activeButtonPalette" } else { "togglePalette/inactiveButtonPalette" };
                    for (list, hidden) in [("hideWhenActive", on), ("hideWhenNotActive", !on)] {
                        for h in v.get(list).and_then(Value::as_array).unwrap_or_default().iter().filter_map(Value::as_pptr) {
                            if hidden && h.file_id == 0 && !h.is_null() {
                                fx.hide.insert(h.path_id);
                            }
                        }
                    }
                    if let (Some(knob), Some(at)) = (pptr(v, "indicator"), pptr(v, if on { "indicatorOnPosition" } else { "indicatorOffPosition" })) {
                        fx.moves.insert(knob.path_id, at.path_id);
                    }
                    int(v, "toggleType").and_then(|n| self.entry("togglePalettes", "togglePaletteType", n)).and_then(|e| e.path(look))
                };
                let Some(p) = palette else { return };
                let tints = (p.get("backgroundTints").and_then(Tints::read), p.get("foregroundTints").and_then(Tints::read));
                put(pptr(v, "multiGraphicButton").or(pptr(v, "selectable")), Themed { tints: Some(tints), ..Default::default() });
                let fg = rgba(p.get("foregroundColor"));
                if v.get("controlSurfaceColor").and_then(Value::as_bool).unwrap_or(true) {
                    put(pick("surface", "useSurfaceOverride", "surfaceOverride"), Themed { color: rgba(p.get("backgroundColor")), ..Default::default() });
                }
                put(pick("gradient", "useGradientOverride", "gradientOverride"), Themed { gradient: p.path("backgroundGradients/normalGradient").cloned(), ..Default::default() });
                if v.get("controlGraphicColor").and_then(Value::as_bool).unwrap_or(true) {
                    let list = if flag(v, "useGraphicsOverride") { "graphicsOverride" } else { "graphics" };
                    for g in v.get(list).and_then(Value::as_array).unwrap_or_default() {
                        put(g.as_pptr(), Themed { color: fg, ..Default::default() });
                    }
                }
                let text = pptr(v, "textMeshProUGUI").or(pptr(v, "textMeshProUGUIOverride"));
                let font = p.get("textPalette").and_then(|s| self.font(s));
                put(text, Themed { color: fg, font, font_size: int(v, "textSize").and_then(|n| self.text_size(n)), ..Default::default() });
                if let (Some(b), false) = (pptr(v, "borderHideable"), flag(p, "showBorder")) {
                    fx.hide.insert(b.path_id);
                }
            }
            "RRUI.Theme.TilerTheme" => {
                let Some(t) = int(v, "type").and_then(|n| self.entry("tilerPaletteItems", "tilerPaletteType", n)).and_then(|e| e.get("tilerPalette")) else { return };
                let tiler = pptr(t, "sprite").map(|s| (self.file.clone(), s, rgba(t.get("tintColor")).unwrap_or([1.0; 4]),
                    rgba(t.get("backgroundColor")).unwrap_or([1.0; 4]), t.get("rows").and_then(Value::as_f32).unwrap_or(5.0)));
                put(pptr(v, "tiler"), Themed { tiler, ..Default::default() });
            }
            _ => {}
        }
    }
}

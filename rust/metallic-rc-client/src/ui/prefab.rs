//! Loads a UI page prefab by its Addressables address (e.g. `(EscapeMenu_standalone)`) from the install: the catalog
//! names the bundle and the asset path, the bundle's container gives the root GameObject, and the tree is read from
//! there. Components keep their serialized values; MonoBehaviours are named by their script class
//! (`UnityEngine.UI.Image`, `TMPro.TextMeshProUGUI`, `RRUI.RoundedCorners`, ...) so the later stages can pick theirs.
use std::path::Path;

use unity_assets::catalog::{bundle_file, Catalog};
use unity_assets::{AssetBundleInfo, GameObject, PPtr, Transform, Value};

use super::input::Tints;
use crate::room::{value, FileRef, Library};

const GAME_OBJECT: i32 = 1;
const RECT_TRANSFORM: i32 = 224;
const TRANSFORM: i32 = 4;
const MONO_BEHAVIOUR: i32 = 114;
const CANVAS_RENDERER: i32 = 222;
const ASSET_BUNDLE: i32 = 142;

/// A RectTransform's rules, as Unity stores them (x, y; Unity's y points up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectRules {
    pub anchor_min: [f32; 2],
    pub anchor_max: [f32; 2],
    pub anchored_position: [f32; 2],
    pub size_delta: [f32; 2],
    pub pivot: [f32; 2],
    pub scale: [f32; 3],
}

impl Default for RectRules {
    fn default() -> Self {
        RectRules { anchor_min: [0.5; 2], anchor_max: [0.5; 2], anchored_position: [0.0; 2], size_delta: [100.0; 2], pivot: [0.5; 2], scale: [1.0; 3] }
    }
}

/// One component: its class (script class for MonoBehaviours, Unity class otherwise) and serialized values.
#[derive(Clone, Debug)]
pub struct Component {
    pub class: String,
    pub value: Value,
    /// the file the value was read from, for following its references (sprites, fonts)
    pub(crate) file: Option<FileRef>,
    /// the component's object id in `file`, which other components' references name
    pub id: i64,
    pub enabled: bool,
    /// what the page's theme components set on this one (see [`super::theme`])
    pub themed: Themed,
}

/// Values a theme component puts on a graphic, read before the serialized ones.
#[derive(Clone, Debug, Default)]
pub struct Themed {
    pub color: Option<[f32; 4]>,
    /// a Unity Gradient value, for a UIGradient
    pub gradient: Option<Value>,
    /// a TMP font asset, with the file its reference is relative to
    pub(crate) font: Option<(FileRef, PPtr)>,
    pub font_size: Option<f32>,
    /// a UITiler's pattern: sprite (with its file), tint, background, rows
    pub(crate) tiler: Option<(FileRef, PPtr, [f32; 4], [f32; 4], f32)>,
    /// a selectable's state tints: for its target graphic, and for a MultiGraphicButton's other graphics
    pub tints: Option<(Option<Tints>, Option<Tints>)>,
}

#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub active: bool,
    /// the GameObject's and the RectTransform's object ids, which other components name
    pub id: i64,
    pub transform: i64,
    pub rect: RectRules,
    pub components: Vec<Component>,
    pub children: Vec<Element>,
}

impl Element {
    /// The first enabled component of a class.
    pub fn component(&self, class: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.enabled && c.class == class)
    }

    pub fn has(&self, class: &str) -> bool {
        self.component(class).is_some()
    }

    /// The controller implementations (`XModelController/YImpl` and their data) of the enabled controllers here.
    pub fn impls(&self) -> Vec<(&str, &Value)> {
        let mut out = vec![];
        for c in self.components.iter().filter(|c| c.enabled) {
            let Some(refs) = c.value.path("references/RefIds").and_then(Value::as_array) else { continue };
            let rid = c.value.path("implementation/rid").and_then(Value::as_i64);
            for r in refs.iter().filter(|r| r.get("rid").and_then(Value::as_i64) == rid) {
                if let (Some(class), Some(data)) = (r.path("type/class").and_then(Value::as_str), r.get("data")) {
                    out.push((class, data));
                }
            }
        }
        out
    }

    /// Every element in the tree, depth first, mutably.
    pub fn each_mut(&mut self, f: &mut dyn FnMut(&mut Element)) {
        f(self);
        for c in &mut self.children {
            c.each_mut(f);
        }
    }

    /// Every element in the tree, depth first, with its depth.
    pub fn walk(&self) -> Vec<(usize, &Element)> {
        fn go<'a>(e: &'a Element, d: usize, out: &mut Vec<(usize, &'a Element)>) {
            out.push((d, e));
            for c in &e.children {
                go(c, d + 1, out);
            }
        }
        let mut out = vec![];
        go(self, 0, &mut out);
        out
    }
}

impl std::fmt::Debug for FileRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.cab())
    }
}

fn floats<const N: usize>(v: &Value, name: &str, default: [f32; N]) -> [f32; N] {
    v.get(name).map(|x| x.floats()).and_then(|f| f.get(..N).map(|s| s.try_into().unwrap())).unwrap_or(default)
}

fn rect_rules(v: &Value) -> RectRules {
    let d = RectRules::default();
    RectRules {
        anchor_min: floats(v, "m_AnchorMin", d.anchor_min),
        anchor_max: floats(v, "m_AnchorMax", d.anchor_max),
        anchored_position: floats(v, "m_AnchoredPosition", d.anchored_position),
        size_delta: floats(v, "m_SizeDelta", d.size_delta),
        pivot: floats(v, "m_Pivot", d.pivot),
        scale: floats(v, "m_LocalScale", d.scale),
    }
}

/// The address -> (bundle file, asset path, type) table of the install's Addressables catalog.
pub struct Pages {
    entries: Vec<(String, String, String, String)>,
}

impl Pages {
    pub fn open(bundle_dir: &Path) -> Result<Self, String> {
        let path = bundle_dir.parent().ok_or("no catalog folder")?.join("catalog.bin");
        let cat = Catalog::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut entries = vec![];
        for e in cat.entries().map_err(|e| e.to_string())? {
            for l in &e.locations {
                if let Some(b) = bundle_file(l) {
                    entries.push((e.key.clone(), b.to_string(), l.internal_id.clone(), l.resource_type.clone()));
                }
            }
        }
        Ok(Pages { entries })
    }

    /// Bundle file and asset path of a page (a GameObject) by its address.
    pub fn find(&self, address: &str) -> Option<(&str, &str)> {
        self.find_typed(address, "UnityEngine.GameObject")
    }

    /// Bundle file and asset path of an asset of type `resource_type` (e.g. `RRUI.ThemesConfig`).
    pub fn find_typed(&self, address: &str, resource_type: &str) -> Option<(&str, &str)> {
        self.entries.iter().find(|(k, _, _, t)| k == address && t == resource_type).map(|(_, b, i, _)| (b.as_str(), i.as_str()))
    }

    /// Page addresses that contain `part`.
    pub fn search(&self, part: &str) -> Vec<&str> {
        let mut v: Vec<&str> = self.entries.iter().filter(|(k, _, _, t)| k.contains(part) && t == "UnityEngine.GameObject").map(|(k, _, _, _)| k.as_str()).collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Reads the page at `address`.
pub fn load(lib: &mut Library, pages: &Pages, bundle_dir: &Path, address: &str) -> Result<Element, String> {
    let (bundle, asset) = pages.find(address).ok_or_else(|| format!("no page {address} in the catalog"))?;
    let (file, id) = asset_object(lib, bundle_dir, bundle, asset)?;
    element(lib, &file, id, 0)
}

/// The object an asset path names, through its bundle's container.
pub(crate) fn asset_object(lib: &mut Library, bundle_dir: &Path, bundle: &str, asset: &str) -> Result<(FileRef, i64), String> {
    let open = lib.open_bundle(&bundle_dir.join(bundle))?;
    for index in 0..open.files.len() {
        let file = FileRef { open: open.clone(), index };
        let sf = file.sf();
        let Some(info) = sf.objects.iter().find(|o| o.class_id == ASSET_BUNDLE) else { continue };
        let info = AssetBundleInfo::from_value(&value(&file, info.path_id)?).map_err(|e| e.to_string())?;
        if let Some((_, root)) = info.container.iter().find(|(p, _)| p.eq_ignore_ascii_case(asset)) {
            return Ok((file, root.path_id));
        }
    }
    Err(format!("{asset} not in {bundle}"))
}

fn script_class(lib: &mut Library, file: &FileRef, script: PPtr) -> String {
    let named = lib.resolve(file, script).ok().and_then(|(f, id)| {
        let v = value(&f, id).ok()?;
        let (ns, class) = (v.get("m_Namespace")?.as_str()?, v.get("m_ClassName")?.as_str()?);
        Some(if ns.is_empty() { class.to_string() } else { format!("{ns}.{class}") })
    });
    named.unwrap_or_else(|| "?".into())
}

/// Marks a list spawner's item template (see [`element`]).
pub const TEMPLATE: &str = " [template]";

/// The GameObject `p` names, or the one the component it names is on.
fn object(lib: &mut Library, file: &FileRef, p: PPtr, depth: usize) -> Result<Element, String> {
    let (f, mut go) = lib.resolve(file, p)?;
    if f.sf().object(go).map(|o| o.class_id) != Some(GAME_OBJECT) {
        go = value(&f, go)?.get("m_GameObject").and_then(Value::as_pptr).ok_or("prefab reference is not a GameObject")?.path_id;
    }
    element(lib, &f, go, depth + 1)
}

fn element(lib: &mut Library, file: &FileRef, go_id: i64, depth: usize) -> Result<Element, String> {
    if depth > 64 {
        return Err("page tree too deep".into());
    }
    let go = GameObject::from_value(&value(file, go_id)?).map_err(|e| e.to_string())?;
    let mut e = Element { name: go.name.clone(), active: go.active, id: go_id, transform: 0, rect: RectRules::default(), components: vec![], children: vec![] };
    let mut children = vec![];
    for c in &go.components {
        // prefab components live in the prefab's own file
        if c.file_id != 0 {
            continue;
        }
        let Some(o) = file.sf().object(c.path_id) else { continue };
        let v = value(file, c.path_id)?;
        match o.class_id {
            RECT_TRANSFORM | TRANSFORM => {
                if o.class_id == RECT_TRANSFORM {
                    e.rect = rect_rules(&v);
                }
                e.transform = c.path_id;
                let t = Transform::from_value(&v).map_err(|e| e.to_string())?;
                children = t.children;
            }
            CANVAS_RENDERER => {}
            MONO_BEHAVIOUR => {
                let class = match v.get("m_Script").and_then(Value::as_pptr) {
                    Some(s) if !s.is_null() => script_class(lib, file, s),
                    _ => "?".into(),
                };
                let enabled = v.get("m_Enabled").and_then(Value::as_bool).unwrap_or(true);
                e.components.push(Component { class, value: v, file: Some(file.clone()), id: c.path_id, enabled, themed: Themed::default() });
            }
            _ => {
                let class = file.sf().type_name(o);
                let enabled = v.get("m_Enabled").and_then(Value::as_bool).unwrap_or(true);
                e.components.push(Component { class, value: v, file: Some(file.clone()), id: c.path_id, enabled, themed: Themed::default() });
            }
        }
    }
    for ch in children {
        let t = Transform::from_value(&value(file, ch.path_id)?).map_err(|e| e.to_string())?;
        e.children.push(element(lib, file, t.game_object.path_id, depth + 1)?);
    }
    // a StaggeredPrefabSpawner puts its prefab under itself once it is on screen; the page is read with it in place
    // (prefabs named by Addressables GUID instead are left out)
    let spawn = e.component("RRUI.StaggeredPrefabSpawner").filter(|c| !c.value.get("useAssetReference").and_then(Value::as_bool).unwrap_or(false))
        .and_then(|c| c.value.get("prefab").and_then(Value::as_pptr)).filter(|p| !p.is_null());
    if let Some(p) = spawn {
        e.children.push(object(lib, file, p, depth)?);
    }
    // a list spawner's item prefab is kept under it, inactive, as the template its items are made from
    let item = e.impls().iter().find_map(|(_, d)| d.get("defaultPrefab").and_then(Value::as_pptr)).filter(|p| !p.is_null());
    if let Some(p) = item {
        let mut t = object(lib, file, p, depth)?;
        t.active = false;
        t.name.push_str(TEMPLATE);
        e.children.push(t);
    }
    Ok(e)
}

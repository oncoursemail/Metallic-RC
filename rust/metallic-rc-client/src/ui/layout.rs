//! Unity's uGUI layout, so a page lands where the game puts it: RectTransform anchors and pivots, then the layout
//! pass (sizes calculated bottom-up, then set top-down; widths first, then heights, because text height depends on
//! width) with Horizontal/Vertical layout groups, LayoutElement, ContentSizeFitter and AspectRatioFitter.
//!
//! Builds keep RectTransforms that a layout group drives at zero, so the pass is needed even for a static page.
//! Text and image sizes come from a [`Measure`], which knows fonts and sprites.
use unity_assets::Value;

use super::prefab::{Component, Element, RectRules};

/// Sizes the layout needs from outside: an image's sprite size and a text's size.
pub trait Measure {
    /// The sprite's natural size in canvas units (`Image.preferredWidth/Height`), when the image has a sprite.
    fn image(&mut self, image: &Component) -> Option<[f32; 2]>;
    /// The text's size: unwrapped when `width` is None, wrapped to `width` otherwise.
    fn text(&mut self, text: &Component, width: Option<f32>) -> [f32; 2];
}

/// A measure for when fonts and sprites are not at hand: no sprite sizes, text at half an em per character.
pub struct Rough;

impl Measure for Rough {
    fn image(&mut self, _: &Component) -> Option<[f32; 2]> {
        None
    }
    fn text(&mut self, c: &Component, width: Option<f32>) -> [f32; 2] {
        let size = c.value.get("m_fontSize").and_then(Value::as_f32).unwrap_or(36.0);
        let text = c.value.get("m_text").and_then(Value::as_str).unwrap_or("");
        let line = text.chars().count() as f32 * size * 0.5;
        let lines = width.filter(|w| *w > 0.0).map_or(1.0, |w| (line / w).ceil().max(1.0));
        [width.map_or(line, |w| line.min(w)), lines * size * 1.2]
    }
}

/// An element's place on the canvas: left, top, width, height, with y growing downwards.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The laid-out page: one entry per drawn element, parents before children, with the index of the element in
/// `Element::walk()` order and its depth.
pub struct Laid {
    pub boxes: Vec<(usize, usize, Placed)>,
}

const IMAGE: &str = "UnityEngine.UI.Image";
const TEXT: &str = "TMPro.TextMeshProUGUI";
const HORIZONTAL: &str = "UnityEngine.UI.HorizontalLayoutGroup";
const VERTICAL: &str = "UnityEngine.UI.VerticalLayoutGroup";
const LAYOUT_ELEMENT: &str = "UnityEngine.UI.LayoutElement";
const SIZE_FITTER: &str = "UnityEngine.UI.ContentSizeFitter";
const ASPECT_FITTER: &str = "UnityEngine.UI.AspectRatioFitter";

fn f(v: &Value, name: &str, default: f32) -> f32 {
    v.get(name).and_then(Value::as_f32).unwrap_or(default)
}

fn i(v: &Value, name: &str) -> i64 {
    v.get(name).and_then(Value::as_i64).unwrap_or(0)
}

fn b(v: &Value, name: &str) -> bool {
    v.get(name).and_then(Value::as_bool).unwrap_or(false)
}

#[derive(Clone, Copy)]
struct Group {
    vertical: bool,
    /// left, right, top, bottom
    padding: [f32; 4],
    spacing: f32,
    alignment: i64,
    control: [bool; 2],
    expand: [bool; 2],
    scale: [bool; 2],
    reverse: bool,
}

impl Group {
    fn read(c: &Component) -> Option<Group> {
        let vertical = match c.class.as_str() {
            HORIZONTAL => false,
            VERTICAL => true,
            _ => return None,
        };
        let v = &c.value;
        let p = v.get("m_Padding");
        let pad = |n: &str| p.and_then(|p| p.get(n)).and_then(Value::as_f32).unwrap_or(0.0);
        Some(Group {
            vertical,
            padding: [pad("m_Left"), pad("m_Right"), pad("m_Top"), pad("m_Bottom")],
            spacing: f(v, "m_Spacing", 0.0),
            alignment: i(v, "m_ChildAlignment"),
            control: [b(v, "m_ChildControlWidth"), b(v, "m_ChildControlHeight")],
            expand: [b(v, "m_ChildForceExpandWidth"), b(v, "m_ChildForceExpandHeight")],
            scale: [b(v, "m_ChildScaleWidth"), b(v, "m_ChildScaleHeight")],
            reverse: b(v, "m_ReverseArrangement"),
        })
    }

    fn padding_sum(&self, axis: usize) -> f32 {
        if axis == 0 { self.padding[0] + self.padding[1] } else { self.padding[2] + self.padding[3] }
    }

    fn padding_start(&self, axis: usize) -> f32 {
        if axis == 0 { self.padding[0] } else { self.padding[2] }
    }

    fn alignment_on(&self, axis: usize) -> f32 {
        if axis == 0 { (self.alignment % 3) as f32 * 0.5 } else { (self.alignment / 3) as f32 * 0.5 }
    }
}

/// One element during the pass.
struct Node {
    rules: RectRules,
    children: Vec<usize>,
    /// index into the element list
    element: usize,
    /// min, preferred, flexible per axis, as the element reports them (`ILayoutElement` values combined)
    input: [[f32; 3]; 2],
    /// the layout group's own totals per axis (`m_TotalMinSize` etc.), which a LayoutElement on the same object does not change
    totals: [[f32; 3]; 2],
    /// rect size, kept current as rules change
    size: [f32; 2],
    group: Option<Group>,
}

struct Pass<'a> {
    elements: Vec<(usize, &'a Element)>,
    nodes: Vec<Node>,
}

/// Lays out `root` on a canvas of `canvas` size (Unity units).
pub fn lay_out(root: &Element, canvas: [f32; 2], measure: &mut dyn Measure) -> Laid {
    let elements = root.walk();
    let mut pass = Pass { elements, nodes: vec![] };
    pass.build(root, canvas);
    let order = pass.order();
    for axis in 0..2 {
        // sizes bottom-up, then placement top-down
        for &n in order.iter().rev() {
            pass.calculate(n, axis, measure);
        }
        for &n in &order {
            pass.control(n, axis);
        }
    }
    let mut boxes = vec![];
    pass.place(0, Placed { x: 0.0, y: 0.0, w: canvas[0], h: canvas[1] }, true, 0, &mut boxes);
    Laid { boxes }
}

impl<'a> Pass<'a> {
    /// Nodes for the active elements; node 0 is the root, sized to fill the canvas.
    fn build(&mut self, root: &Element, canvas: [f32; 2]) {
        fn go(p: &mut Pass, e: &Element, index: &mut usize) -> Option<usize> {
            let mine = *index;
            *index += 1;
            if !e.active {
                // skip the subtree's indices
                *index += e.walk().len() - 1;
                return None;
            }
            let n = p.nodes.len();
            let group = e.components.iter().filter(|c| c.enabled).find_map(Group::read);
            p.nodes.push(Node { rules: e.rect, children: vec![], element: mine, input: [[0.0; 3]; 2], totals: [[0.0; 3]; 2], size: [0.0; 2], group });
            for c in &e.children {
                if let Some(k) = go(p, c, index) {
                    p.nodes[n].children.push(k);
                }
            }
            Some(n)
        }
        let mut index = 0;
        if go(self, root, &mut index).is_none() {
            return;
        }
        // the page fills its canvas
        let r = &mut self.nodes[0].rules;
        (r.anchor_min, r.anchor_max, r.size_delta, r.anchored_position) = ([0.0; 2], [1.0; 2], [0.0; 2], [0.0; 2]);
        self.nodes[0].size = canvas;
        self.resize_children(0);
    }

    fn element(&self, n: usize) -> &'a Element {
        self.elements[self.nodes[n].element].1
    }

    /// Parents before children.
    fn order(&self) -> Vec<usize> {
        let mut out = vec![];
        let mut stack = vec![0];
        while let Some(n) = stack.pop() {
            if n < self.nodes.len() {
                out.push(n);
                stack.extend(self.nodes[n].children.iter().rev());
            }
        }
        out
    }

    /// A child's size from its rules and its parent's size.
    fn size_in(rules: &RectRules, parent: [f32; 2]) -> [f32; 2] {
        [0, 1].map(|a| (rules.anchor_max[a] - rules.anchor_min[a]) * parent[a] + rules.size_delta[a])
    }

    fn resize_children(&mut self, n: usize) {
        let parent = self.nodes[n].size;
        for k in self.nodes[n].children.clone() {
            self.nodes[k].size = Self::size_in(&self.nodes[k].rules, parent);
            self.resize_children(k);
        }
    }

    /// Children a layout group arranges: active, not ignoring layout.
    fn layout_children(&self, n: usize) -> Vec<usize> {
        self.nodes[n].children.iter().copied().filter(|&k| {
            !self.element(k).component(LAYOUT_ELEMENT).is_some_and(|c| b(&c.value, "m_IgnoreLayout"))
        }).collect()
    }

    /// `LayoutUtility.GetMinSize/GetPreferredSize/GetFlexibleSize` from the element's inputs.
    fn calculate(&mut self, n: usize, axis: usize, measure: &mut dyn Measure) {
        let e = self.element(n);
        // (priority, min, preferred, flexible) of each layout element on the object; -1 = no say
        let mut sources: Vec<(i64, [f32; 3])> = vec![];
        for c in e.components.iter().filter(|c| c.enabled) {
            match c.class.as_str() {
                IMAGE => {
                    let pref = measure.image(c).map_or(0.0, |s| s[axis]);
                    sources.push((0, [0.0, pref, -1.0]));
                }
                TEXT => {
                    let width = (axis == 1).then_some(self.nodes[n].size[0]);
                    let s = measure.text(c, width);
                    sources.push((0, [0.0, s[axis], -1.0]));
                }
                LAYOUT_ELEMENT => {
                    let v = &c.value;
                    let names = if axis == 0 { ["m_MinWidth", "m_PreferredWidth", "m_FlexibleWidth"] } else { ["m_MinHeight", "m_PreferredHeight", "m_FlexibleHeight"] };
                    let priority = v.get("m_LayoutPriority").and_then(Value::as_i64).unwrap_or(1);
                    sources.push((priority, names.map(|k| f(v, k, -1.0))));
                }
                _ => {}
            }
        }
        if let Some(g) = self.nodes[n].group {
            let totals = self.group_input(n, &g, axis);
            self.nodes[n].totals[axis] = totals;
            sources.push((0, totals));
        }
        let mut out = [0.0; 3];
        for p in 0..3 {
            let mut best: Option<(i64, f32)> = None;
            for (prio, vals) in &sources {
                let v = vals[p];
                if v < 0.0 {
                    continue;
                }
                best = match best {
                    Some((bp, bv)) if *prio < bp || (*prio == bp && v <= bv) => Some((bp, bv)),
                    _ => Some((*prio, v)),
                };
            }
            out[p] = best.map_or(0.0, |(_, v)| v);
        }
        out[1] = out[1].max(out[0]);
        self.nodes[n].input[axis] = out;
    }

    /// `GetChildSizes`: what a group counts for one child.
    fn child_sizes(&self, k: usize, axis: usize, g: &Group) -> [f32; 3] {
        let mut s = if g.control[axis] {
            self.nodes[k].input[axis]
        } else {
            let d = self.nodes[k].rules.size_delta[axis];
            [d, d, 0.0]
        };
        if g.expand[axis] {
            s[2] = s[2].max(1.0);
        }
        s
    }

    fn child_scale(&self, k: usize, axis: usize, g: &Group) -> f32 {
        if g.scale[axis] { self.nodes[k].rules.scale[axis] } else { 1.0 }
    }

    /// `CalcAlongAxis`: the group's own min/preferred/flexible.
    fn group_input(&self, n: usize, g: &Group, axis: usize) -> [f32; 3] {
        let pad = g.padding_sum(axis);
        let along_other = g.vertical ^ (axis == 1);
        let (mut min, mut pref, mut flex) = (pad, pad, 0.0f32);
        let children = self.layout_children(n);
        for &k in &children {
            let [cmin, cpref, cflex] = self.child_sizes(k, axis, g);
            let s = self.child_scale(k, axis, g);
            let (cmin, cpref, cflex) = (cmin * s, cpref * s, cflex * s);
            if along_other {
                min = min.max(cmin + pad);
                pref = pref.max(cpref + pad);
                flex = flex.max(cflex);
            } else {
                min += cmin + g.spacing;
                pref += cpref + g.spacing;
                flex += cflex;
            }
        }
        if !along_other && !children.is_empty() {
            min -= g.spacing;
            pref -= g.spacing;
        }
        [min, pref.max(min), flex]
    }

    /// Self controllers (fitters) first, then the group places its children.
    fn control(&mut self, n: usize, axis: usize) {
        let e = self.element(n);
        if let Some(c) = e.component(SIZE_FITTER) {
            let fit = i(&c.value, if axis == 0 { "m_HorizontalFit" } else { "m_VerticalFit" });
            if fit == 1 || fit == 2 {
                let size = self.nodes[n].input[axis][fit as usize - 1];
                self.set_size(n, axis, size);
            }
        }
        if let Some(c) = e.component(ASPECT_FITTER) {
            let (mode, ratio) = (i(&c.value, "m_AspectMode"), f(&c.value, "m_AspectRatio", 1.0).max(0.001));
            match (mode, axis) {
                // width controls height
                (1, 1) => self.set_size(n, 1, self.nodes[n].size[0] / ratio),
                // height controls width
                (2, 0) => self.set_size(n, 0, self.nodes[n].size[1] * ratio),
                _ => {}
            }
        }
        if let Some(g) = self.nodes[n].group {
            self.place_children(n, &g, axis);
        }
    }

    /// `SetSizeWithCurrentAnchors`
    fn set_size(&mut self, n: usize, axis: usize, size: f32) {
        let parent = self.parent_size(n);
        let r = &mut self.nodes[n].rules;
        r.size_delta[axis] = size - (r.anchor_max[axis] - r.anchor_min[axis]) * parent[axis];
        self.nodes[n].size[axis] = size;
        self.resize_children(n);
    }

    fn parent_size(&self, n: usize) -> [f32; 2] {
        self.nodes.iter().find(|p| p.children.contains(&n)).map_or(self.nodes[n].size, |p| p.size)
    }

    /// `SetChildAlongAxisWithScale`: anchored to the top left, placed at `pos` from the start edge.
    fn set_child(&mut self, k: usize, axis: usize, pos: f32, size: Option<f32>, scale: f32) {
        let r = &mut self.nodes[k].rules;
        r.anchor_min[axis] = if axis == 0 { 0.0 } else { 1.0 };
        r.anchor_max[axis] = r.anchor_min[axis];
        if let Some(s) = size {
            r.size_delta[axis] = s;
        }
        let s = r.size_delta[axis];
        r.anchored_position[axis] = if axis == 0 { pos + s * r.pivot[0] * scale } else { -pos - s * (1.0 - r.pivot[1]) * scale };
        self.nodes[k].size[axis] = s;
        self.resize_children(k);
    }

    fn start_offset(&self, n: usize, g: &Group, axis: usize, required: f32) -> f32 {
        let surplus = self.nodes[n].size[axis] - (required + g.padding_sum(axis));
        g.padding_start(axis) + surplus * g.alignment_on(axis)
    }

    /// `SetChildrenAlongAxis`
    fn place_children(&mut self, n: usize, g: &Group, axis: usize) {
        let size = self.nodes[n].size[axis];
        let along_other = g.vertical ^ (axis == 1);
        let mut children = self.layout_children(n);
        if g.reverse {
            children.reverse();
        }
        let control = g.control[axis];
        if along_other {
            let inner = size - g.padding_sum(axis);
            for k in children {
                let [min, pref, flex] = self.child_sizes(k, axis, g);
                let s = self.child_scale(k, axis, g);
                let required = inner.clamp(min, if flex > 0.0 { size } else { pref }.max(min));
                let start = self.start_offset(n, g, axis, required * s);
                if control {
                    self.set_child(k, axis, start, Some(required), s);
                } else {
                    let offset = (required - self.nodes[k].rules.size_delta[axis]) * g.alignment_on(axis);
                    self.set_child(k, axis, start + offset, None, s);
                }
            }
            return;
        }
        let [total_min, total_pref, total_flex] = self.nodes[n].totals[axis];
        let mut pos = g.padding_start(axis);
        let mut flex_mult = 0.0;
        let surplus = size - total_pref;
        if surplus > 0.0 {
            if total_flex == 0.0 {
                pos = self.start_offset(n, g, axis, total_pref - g.padding_sum(axis));
            } else {
                flex_mult = surplus / total_flex;
            }
        }
        let lerp = if total_min != total_pref { ((size - total_min) / (total_pref - total_min)).clamp(0.0, 1.0) } else { 0.0 };
        for k in children {
            let [min, pref, flex] = self.child_sizes(k, axis, g);
            let s = self.child_scale(k, axis, g);
            let child = min + (pref - min) * lerp + flex * flex_mult;
            if control {
                self.set_child(k, axis, pos, Some(child), s);
            } else {
                let offset = (child - self.nodes[k].rules.size_delta[axis]) * g.alignment_on(axis);
                self.set_child(k, axis, pos + offset, None, s);
            }
            pos += child * s + g.spacing;
        }
    }

    /// Canvas rects, y down, parents first.
    fn place(&self, n: usize, rect: Placed, _root: bool, depth: usize, out: &mut Vec<(usize, usize, Placed)>) {
        out.push((self.nodes[n].element, depth, rect));
        for &k in &self.nodes[n].children {
            let r = &self.nodes[k].rules;
            let [w, h] = self.nodes[k].size;
            // pivot point in the parent, measured from the parent's bottom left (Unity: y up)
            let px = rect.w * (r.anchor_min[0] + (r.anchor_max[0] - r.anchor_min[0]) * r.pivot[0]) + r.anchored_position[0];
            let py = rect.h * (r.anchor_min[1] + (r.anchor_max[1] - r.anchor_min[1]) * r.pivot[1]) + r.anchored_position[1];
            let left = px - w * r.pivot[0];
            let bottom = py - h * r.pivot[1];
            let child = Placed { x: rect.x + left, y: rect.y + (rect.h - bottom - h), w, h };
            self.place(k, child, false, depth + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unity_assets::Value;

    fn s(fields: &[(&str, Value)]) -> Value {
        Value::Struct { type_name: "".into(), fields: fields.iter().map(|(k, v)| ((*k).into(), v.clone())).collect() }
    }

    fn comp(class: &str, fields: &[(&str, Value)]) -> Component {
        Component { class: class.into(), value: s(fields), file: None, id: 0, enabled: true, themed: Default::default() }
    }

    fn el(name: &str, rect: RectRules, components: Vec<Component>, children: Vec<Element>) -> Element {
        Element { name: name.into(), active: true, rect, components, children }
    }

    fn group(class: &str, spacing: f32, pad: f32, control: bool, expand_w: bool, expand_h: bool) -> Component {
        let p = s(&[("m_Left", Value::Int(pad as i64)), ("m_Right", Value::Int(pad as i64)), ("m_Top", Value::Int(pad as i64)), ("m_Bottom", Value::Int(pad as i64))]);
        comp(class, &[
            ("m_Padding", p),
            ("m_Spacing", Value::Float(spacing as f64)),
            ("m_ChildAlignment", Value::Int(0)),
            ("m_ChildControlWidth", Value::Int(control as i64)),
            ("m_ChildControlHeight", Value::Int(control as i64)),
            ("m_ChildForceExpandWidth", Value::Int(expand_w as i64)),
            ("m_ChildForceExpandHeight", Value::Int(expand_h as i64)),
        ])
    }

    fn pref(w: f32, h: f32) -> Component {
        comp(LAYOUT_ELEMENT, &[("m_MinWidth", Value::Float(-1.0)), ("m_MinHeight", Value::Float(-1.0)), ("m_PreferredWidth", Value::Float(w as f64)), ("m_PreferredHeight", Value::Float(h as f64)), ("m_FlexibleWidth", Value::Float(-1.0)), ("m_FlexibleHeight", Value::Float(-1.0))])
    }

    fn zero() -> RectRules {
        RectRules { anchor_min: [0.0; 2], anchor_max: [0.0; 2], size_delta: [0.0; 2], ..Default::default() }
    }

    #[test]
    fn anchors_and_pivots() {
        // a 200x100 box anchored at the top right corner, pivot top right, 10 in from each edge
        let r = RectRules { anchor_min: [1.0, 1.0], anchor_max: [1.0, 1.0], anchored_position: [-10.0, -10.0], size_delta: [200.0, 100.0], pivot: [1.0, 1.0], scale: [1.0; 3] };
        let page = el("page", RectRules::default(), vec![], vec![el("box", r, vec![], vec![])]);
        let laid = lay_out(&page, [1920.0, 1080.0], &mut Rough);
        assert_eq!(laid.boxes[1].2, Placed { x: 1710.0, y: 10.0, w: 200.0, h: 100.0 });
        // stretched with insets
        let r = RectRules { anchor_min: [0.0, 0.0], anchor_max: [1.0, 1.0], anchored_position: [0.0, -15.0], size_delta: [0.0, -30.0], pivot: [0.5, 0.5], scale: [1.0; 3] };
        let page = el("page", RectRules::default(), vec![], vec![el("mask", r, vec![], vec![])]);
        assert_eq!(lay_out(&page, [400.0, 200.0], &mut Rough).boxes[1].2, Placed { x: 0.0, y: 30.0, w: 400.0, h: 170.0 });
    }

    #[test]
    fn vertical_group_stacks_and_expands() {
        // 3 rows of preferred height 50 in a 300x400 column, padding 10, spacing 5, widths forced to the column
        let rows = (0..3).map(|k| el(&format!("row{k}"), zero(), vec![pref(100.0, 50.0)], vec![])).collect();
        let col = el("col", RectRules { anchor_min: [0.0; 2], anchor_max: [0.0; 2], size_delta: [300.0, 400.0], pivot: [0.0, 1.0], anchored_position: [0.0, 400.0], scale: [1.0; 3] },
            vec![group(VERTICAL, 5.0, 10.0, true, true, false)], rows);
        let page = el("page", RectRules::default(), vec![], vec![col]);
        let laid = lay_out(&page, [1000.0, 1000.0], &mut Rough);
        let ys: Vec<Placed> = laid.boxes[2..].iter().map(|b| b.2).collect();
        assert_eq!(ys[0], Placed { x: 10.0, y: 610.0, w: 280.0, h: 50.0 });
        assert_eq!(ys[1].y, 665.0);
        assert_eq!(ys[2].y, 720.0);
    }

    #[test]
    fn horizontal_group_shares_flexible_space_and_content_fits() {
        // two children, one flexible: it takes the rest of a 500 wide row
        let flex = comp(LAYOUT_ELEMENT, &[("m_FlexibleWidth", Value::Float(1.0)), ("m_MinWidth", Value::Float(-1.0)), ("m_PreferredWidth", Value::Float(-1.0)), ("m_MinHeight", Value::Float(-1.0)), ("m_PreferredHeight", Value::Float(40.0)), ("m_FlexibleHeight", Value::Float(-1.0))]);
        let row = el("row", RectRules { anchor_min: [0.0, 1.0], anchor_max: [0.0, 1.0], size_delta: [500.0, 60.0], pivot: [0.0, 1.0], ..Default::default() },
            vec![group(HORIZONTAL, 0.0, 0.0, true, false, false)], vec![el("icon", zero(), vec![pref(60.0, 60.0)], vec![]), el("label", zero(), vec![flex], vec![])]);
        let page = el("page", RectRules::default(), vec![], vec![row]);
        let laid = lay_out(&page, [800.0, 600.0], &mut Rough);
        assert_eq!(laid.boxes[2].2, Placed { x: 0.0, y: 0.0, w: 60.0, h: 60.0 });
        assert_eq!(laid.boxes[3].2, Placed { x: 60.0, y: 0.0, w: 440.0, h: 40.0 });

        // a column that fits its content: as tall as its rows
        let fitter = comp(SIZE_FITTER, &[("m_HorizontalFit", Value::Int(0)), ("m_VerticalFit", Value::Int(2))]);
        let rows = (0..2).map(|k| el(&format!("r{k}"), zero(), vec![pref(10.0, 30.0)], vec![])).collect();
        let col = el("col", RectRules { anchor_min: [0.0, 1.0], anchor_max: [1.0, 1.0], size_delta: [0.0, 0.0], pivot: [0.5, 1.0], ..Default::default() },
            vec![group(VERTICAL, 10.0, 0.0, true, true, false), fitter], rows);
        let page = el("page", RectRules::default(), vec![], vec![col]);
        assert_eq!(lay_out(&page, [800.0, 600.0], &mut Rough).boxes[1].2, Placed { x: 0.0, y: 0.0, w: 800.0, h: 70.0 });
    }

    #[test]
    fn a_layout_element_on_a_group_does_not_shrink_its_children() {
        // a column with a minimum height of 600 (its LayoutElement) still gives its rows their preferred heights
        let min = comp(LAYOUT_ELEMENT, &[("m_MinWidth", Value::Float(-1.0)), ("m_MinHeight", Value::Float(600.0)), ("m_PreferredWidth", Value::Float(-1.0)), ("m_PreferredHeight", Value::Float(-1.0)), ("m_FlexibleWidth", Value::Float(-1.0)), ("m_FlexibleHeight", Value::Float(-1.0))]);
        let rows = (0..2).map(|k| el(&format!("r{k}"), zero(), vec![pref(10.0, 40.0)], vec![])).collect();
        let col = el("col", zero(), vec![group(VERTICAL, 0.0, 0.0, true, true, false), min], rows);
        let outer = el("outer", RectRules { anchor_min: [0.0; 2], anchor_max: [1.0; 2], size_delta: [0.0; 2], ..Default::default() },
            vec![group(VERTICAL, 0.0, 0.0, true, true, false)], vec![col]);
        let page = el("page", RectRules::default(), vec![], vec![outer]);
        let laid = lay_out(&page, [300.0, 800.0], &mut Rough);
        assert_eq!(laid.boxes[2].2.h, 600.0);
        assert_eq!(laid.boxes[3].2, Placed { x: 0.0, y: 0.0, w: 300.0, h: 40.0 });
        assert_eq!(laid.boxes[4].2.y, 40.0);
    }

    #[test]
    fn inactive_and_ignored_children_are_left_out() {
        let ignore = comp(LAYOUT_ELEMENT, &[("m_IgnoreLayout", Value::Int(1))]);
        let mut hidden = el("hidden", zero(), vec![pref(10.0, 100.0)], vec![]);
        hidden.active = false;
        let col = el("col", RectRules { anchor_min: [0.0, 1.0], anchor_max: [0.0, 1.0], size_delta: [100.0, 500.0], pivot: [0.0, 1.0], ..Default::default() },
            vec![group(VERTICAL, 0.0, 0.0, true, true, false)],
            vec![hidden, el("free", RectRules { size_delta: [20.0, 20.0], ..Default::default() }, vec![ignore], vec![]), el("row", zero(), vec![pref(10.0, 30.0)], vec![])]);
        let page = el("page", RectRules::default(), vec![], vec![col]);
        let laid = lay_out(&page, [800.0, 600.0], &mut Rough);
        // the hidden element is not drawn; the ignored one keeps its own rect; the row is first in the column
        assert_eq!(laid.boxes.len(), 4);
        assert_eq!(laid.boxes[3].2, Placed { x: 0.0, y: 0.0, w: 100.0, h: 30.0 });
        assert_eq!(laid.boxes[2].2.w, 20.0);
    }
}

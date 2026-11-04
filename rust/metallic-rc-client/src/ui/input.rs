//! The pointer on a drawn page, the way uGUI's EventSystem handles it: the topmost raycast-target graphic under the
//! pointer is hit (children of a mask only inside the mask), the nearest selectable at or above it takes the events,
//! and that selectable's state (normal, highlighted, pressed, selected, disabled) picks the tint of its graphics.
//!
//! Tints follow Selectable's colour-tint transition: the state's colour times the colour multiplier, multiplied into
//! the graphic's own colour. A MultiGraphicButton tints its extra graphics (text, icon, borders) with a second colour
//! block in the same way. INFERRED from data (see `theme`): the themes write their palette's background tints into the
//! first block and its foreground tints into the second, and a text button's text gets its orange from that tint.
use unity_assets::Value;

use super::layout::Placed;
use super::paint::Drawn;

/// A Unity ColorBlock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tints {
    pub normal: [f32; 4],
    pub highlighted: [f32; 4],
    pub pressed: [f32; 4],
    pub selected: [f32; 4],
    pub disabled: [f32; 4],
    pub multiplier: f32,
}

fn rgba(v: Option<&Value>) -> Option<[f32; 4]> {
    let f = v?.floats();
    (f.len() == 4).then(|| [f[0], f[1], f[2], f[3]])
}

impl Tints {
    pub fn read(v: &Value) -> Option<Tints> {
        let c = |n: &str| rgba(v.get(n));
        Some(Tints {
            normal: c("m_NormalColor")?,
            highlighted: c("m_HighlightedColor")?,
            pressed: c("m_PressedColor")?,
            selected: c("m_SelectedColor")?,
            disabled: c("m_DisabledColor")?,
            multiplier: v.get("m_ColorMultiplier").and_then(Value::as_f32).unwrap_or(1.0),
        })
    }

    pub fn get(&self, s: State) -> [f32; 4] {
        let c = match s {
            State::Normal => self.normal,
            State::Highlighted => self.highlighted,
            State::Pressed => self.pressed,
            State::Selected => self.selected,
            State::Disabled => self.disabled,
        };
        c.map(|x| (x * self.multiplier).min(1.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Normal,
    Highlighted,
    Pressed,
    Selected,
    Disabled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Button,
    /// A slider: the box a drag runs along (canvas units), the value range and whether values are whole.
    Slider { track: Placed, min: f32, max: f32, whole: bool },
}

/// A selectable on a drawn page.
#[derive(Clone, Debug, PartialEq)]
pub struct Interact {
    /// the selectable component's object id
    pub id: i64,
    pub interactable: bool,
    /// the target graphic (component id) and its tints, for a colour-tint transition
    pub target: Option<(i64, Tints)>,
    /// a MultiGraphicButton's other graphics and their tints
    pub extra: Vec<i64>,
    pub extra_tints: Option<Tints>,
    pub kind: Kind,
}

impl Interact {
    /// Each tinted graphic with the colour it is multiplied by in state `s`.
    pub fn tints(&self, s: State) -> Vec<(i64, [f32; 4])> {
        let mut out: Vec<(i64, [f32; 4])> = self.target.iter().map(|(g, t)| (*g, t.get(s))).collect();
        if let Some(t) = self.extra_tints {
            out.extend(self.extra.iter().map(|g| (*g, t.get(s))));
        }
        out
    }

    /// The slider value at canvas point `p`.
    pub fn slider_value(&self, p: [f32; 2]) -> Option<f32> {
        let Kind::Slider { track, min, max, whole } = &self.kind else { return None };
        let t = ((p[0] - track.x) / track.w.max(1e-3)).clamp(0.0, 1.0);
        let v = min + (max - min) * t;
        Some(if *whole { v.round() } else { v })
    }
}

/// Whether canvas point `p` is in `r`.
pub fn contains(r: &Placed, p: [f32; 2]) -> bool {
    p[0] >= r.x && p[0] <= r.x + r.w && p[1] >= r.y && p[1] <= r.y + r.h
}

/// The selectable that takes a pointer at canvas point `p`: the one at or above the last-drawn raycast target there.
/// A raycast target with no selectable above it takes the pointer too, so nothing behind it is hit.
pub fn hit(page: &Drawn, p: [f32; 2]) -> Option<&Interact> {
    fn go<'a>(d: &'a Drawn, p: [f32; 2], owner: Option<&'a Interact>, best: &mut Option<&'a Interact>, any: &mut bool) {
        let inside = contains(&d.rect, p);
        let owner = d.interact.as_ref().or(owner);
        if d.raycast && inside {
            (*best, *any) = (owner, true);
        }
        if d.clip && !inside {
            return;
        }
        for c in &d.children {
            go(c, p, owner, best, any);
        }
    }
    let (mut best, mut any) = (None, false);
    go(page, p, None, &mut best, &mut any);
    best.filter(|_| any)
}

/// Every selectable on a page.
pub fn selectables(page: &Drawn) -> Vec<&Interact> {
    let mut out = vec![];
    fn go<'a>(d: &'a Drawn, out: &mut Vec<&'a Interact>) {
        out.extend(d.interact.as_ref());
        for c in &d.children {
            go(c, out);
        }
    }
    go(page, &mut out);
    out
}

/// Where the element with RectTransform `transform` was drawn.
pub fn placed(page: &Drawn, transform: i64) -> Option<Placed> {
    if page.transform == transform {
        return Some(page.rect);
    }
    page.children.iter().find_map(|c| placed(c, transform))
}

/// What the pointer is doing to a page's selectables, by selectable id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pointer {
    pub hover: Option<i64>,
    pub down: Option<i64>,
    pub selected: Option<i64>,
}

impl Pointer {
    /// One frame: `over` is the selectable under the pointer; returns the one clicked (pressed and released on it).
    pub fn update(&mut self, over: Option<i64>, pressed: bool, released: bool) -> Option<i64> {
        self.hover = over;
        if pressed {
            // a press selects what it lands on, and deselects otherwise
            (self.down, self.selected) = (over, over);
        }
        let mut click = None;
        if released {
            if self.down.is_some() && self.down == over {
                click = over;
            }
            self.down = None;
        }
        click
    }

    /// `Selectable.currentSelectionState`
    pub fn state(&self, i: &Interact) -> State {
        let me = Some(i.id);
        if !i.interactable {
            State::Disabled
        } else if self.down == me {
            State::Pressed
        } else if self.selected == me {
            State::Selected
        } else if self.hover == me {
            State::Highlighted
        } else {
            State::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tints(n: f32) -> Tints {
        Tints { normal: [n; 4], highlighted: [0.9; 4], pressed: [0.8; 4], selected: [1.0; 4], disabled: [0.5; 4], multiplier: 1.0 }
    }

    fn button(id: i64) -> Interact {
        Interact { id, interactable: true, target: Some((id * 10, tints(1.0))), extra: vec![id * 10 + 1], extra_tints: Some(tints(0.5)), kind: Kind::Button }
    }

    fn node(rect: (f32, f32, f32, f32), raycast: bool, interact: Option<Interact>, clip: bool, children: Vec<Drawn>) -> Drawn {
        Drawn { name: String::new(), transform: 0, rect: Placed { x: rect.0, y: rect.1, w: rect.2, h: rect.3 }, clip, raycast, interact, paint: vec![], children }
    }

    #[test]
    fn the_last_drawn_target_wins_and_its_selectable_ancestor_takes_it() {
        // a button whose icon (a raycast target) sits inside it, and a later panel over the right half
        let icon = node((10.0, 10.0, 20.0, 20.0), true, None, false, vec![]);
        let b = node((0.0, 0.0, 100.0, 50.0), true, Some(button(1)), false, vec![icon]);
        let cover = node((60.0, 0.0, 40.0, 50.0), true, None, false, vec![]);
        let page = node((0.0, 0.0, 200.0, 200.0), false, None, false, vec![b, cover]);
        assert_eq!(hit(&page, [15.0, 15.0]).map(|i| i.id), Some(1));
        assert_eq!(hit(&page, [50.0, 40.0]).map(|i| i.id), Some(1));
        // the cover blocks the button under it
        assert_eq!(hit(&page, [70.0, 20.0]), None);
        assert_eq!(hit(&page, [150.0, 150.0]), None);
    }

    #[test]
    fn graphics_outside_a_mask_are_not_hit() {
        let inner = node((0.0, 80.0, 100.0, 50.0), true, Some(button(2)), false, vec![]);
        let viewport = node((0.0, 0.0, 100.0, 100.0), false, None, true, vec![inner]);
        let page = node((0.0, 0.0, 200.0, 200.0), false, None, false, vec![viewport]);
        assert_eq!(hit(&page, [50.0, 90.0]).map(|i| i.id), Some(2));
        assert_eq!(hit(&page, [50.0, 120.0]), None);
    }

    #[test]
    fn states_follow_the_pointer_and_a_click_is_press_and_release_on_one_selectable() {
        let (a, b) = (button(1), button(2));
        let mut p = Pointer::default();
        assert_eq!(p.update(Some(1), false, false), None);
        assert_eq!((p.state(&a), p.state(&b)), (State::Highlighted, State::Normal));
        assert_eq!(p.update(Some(1), true, false), None);
        assert_eq!(p.state(&a), State::Pressed);
        assert_eq!(p.update(Some(1), false, true), Some(1));
        assert_eq!(p.state(&a), State::Selected);
        // pressed on one, released on the other: no click; the press moved the selection
        p.update(Some(2), true, false);
        assert_eq!(p.update(Some(1), false, true), None);
        assert_eq!((p.state(&a), p.state(&b)), (State::Highlighted, State::Selected));
        // a press on nothing deselects
        p.update(None, true, true);
        assert_eq!(p.state(&b), State::Normal);
        let off = Interact { interactable: false, ..button(3) };
        assert_eq!(p.state(&off), State::Disabled);
    }

    #[test]
    fn tints_multiply_by_state() {
        let b = button(1);
        assert_eq!(b.tints(State::Normal), vec![(10, [1.0; 4]), (11, [0.5; 4])]);
        assert_eq!(b.tints(State::Pressed), vec![(10, [0.8; 4]), (11, [0.8; 4])]);
        let s = Interact { kind: Kind::Slider { track: Placed { x: 100.0, y: 0.0, w: 200.0, h: 10.0 }, min: 1.0, max: 101.0, whole: true }, ..button(4) };
        assert_eq!(s.slider_value([200.0, 5.0]), Some(51.0));
        assert_eq!(s.slider_value([0.0, 5.0]), Some(1.0));
        assert_eq!(b.slider_value([0.0, 0.0]), None);
    }
}

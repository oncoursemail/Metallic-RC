//! Puts a painted page on screen with Bevy UI: one absolutely placed node per element, its fills and texts as layers
//! under its children, scaled to fit the window and centred. Each fill and text keeps its own colour ([`Graphic`]),
//! so [`tint`] can multiply the state tints of the selectables into it every frame.
use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::text::{FontSize, LineBreak};
use bevy::ui::{BackgroundGradient, ColorStop, Gradient, LinearGradient};

use super::input::{selectables, Pointer};
use super::paint::{self, Align, Drawn, Paint};

/// The root of a page on screen.
#[derive(Component)]
pub struct Page;

/// A fill or text: the graphic component it draws and its own colour (each stop's, for a gradient).
#[derive(Component)]
pub struct Graphic {
    pub id: i64,
    pub color: [f32; 4],
    pub stops: Vec<[f32; 4]>,
}

/// What [`tint`] changes on a drawn graphic.
pub type Tinted = (&'static Graphic, Option<&'static mut ImageNode>, Option<&'static mut BackgroundColor>, Option<&'static mut TextColor>,
    Option<&'static mut BackgroundGradient>);

/// Sprites and fonts uploaded so far, kept from page to page (with the Arc, so its address is not reused).
#[derive(Resource, Default)]
pub struct Uploaded {
    sprite: HashMap<usize, (Arc<paint::Sprite>, Handle<Image>)>,
    font: HashMap<usize, (Arc<paint::Font>, Handle<Font>)>,
}

struct Uploads<'a> {
    images: &'a mut Assets<Image>,
    fonts: &'a mut Assets<Font>,
    done: &'a mut Uploaded,
}

impl Uploads<'_> {
    fn sprite(&mut self, s: &Arc<paint::Sprite>) -> Handle<Image> {
        let images = &mut *self.images;
        self.done.sprite.entry(Arc::as_ptr(s) as usize).or_insert_with(|| {
            (s.clone(), images.add(Image::new(
                Extent3d { width: s.width, height: s.height, depth_or_array_layers: 1 },
                TextureDimension::D2,
                s.rgba.clone(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            )))
        }).1.clone()
    }

    fn font(&mut self, f: &Arc<paint::Font>) -> Handle<Font> {
        let fonts = &mut *self.fonts;
        self.done.font.entry(Arc::as_ptr(f) as usize).or_insert_with(|| (f.clone(), fonts.add(Font::from_bytes(f.ttf.to_vec())))).1.clone()
    }
}

fn rgba(c: [f32; 4]) -> Color {
    Color::srgba(c[0], c[1], c[2], c[3])
}

/// How much `page` is scaled to fit `window`.
fn scale(page: &Drawn, window: Vec2) -> f32 {
    (window.x * 0.9 / page.rect.w.max(1.0)).min(window.y * 0.9 / page.rect.h.max(1.0))
}

/// The canvas point under window point `p` (logical pixels), for a page spawned into `window`.
pub fn to_canvas(page: &Drawn, window: Vec2, p: Vec2) -> [f32; 2] {
    let s = scale(page, window);
    let corner = (window - Vec2::new(page.rect.w.max(1.0), page.rect.h.max(1.0)) * s) / 2.0;
    let c = (p - corner) / s;
    [page.rect.x + c.x, page.rect.y + c.y]
}

/// Spawns `page` (laid out on a `canvas`-sized surface) fitted into `window`, returning the root entity.
pub fn spawn(commands: &mut Commands, images: &mut Assets<Image>, fonts: &mut Assets<Font>, done: &mut Uploaded, page: &Drawn, window: Vec2) -> Entity {
    let (w, h) = (page.rect.w.max(1.0), page.rect.h.max(1.0));
    let scale = scale(page, window);
    let mut up = Uploads { images, fonts, done };
    let root = commands.spawn((
        Page,
        Node { width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        GlobalZIndex(100),
    )).id();
    let surface = commands.spawn((Node { width: Val::Px(w * scale), height: Val::Px(h * scale), ..default() }, ChildOf(root))).id();
    element(commands, &mut up, page, page.rect.x, page.rect.y, scale, surface);
    root
}

fn element(commands: &mut Commands, up: &mut Uploads, d: &Drawn, px: f32, py: f32, s: f32, parent: Entity) {
    let r = d.rect;
    let node = commands.spawn((
        Name::new(d.name.clone()),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px((r.x - px) * s),
            top: Val::Px((r.y - py) * s),
            width: Val::Px(r.w * s),
            height: Val::Px(r.h * s),
            overflow: if d.clip { Overflow::clip() } else { Overflow::visible() },
            ..default()
        },
        ChildOf(parent),
    )).id();
    for p in &d.paint {
        layer(commands, up, p, r.w, r.h, s, node);
    }
    for c in &d.children {
        element(commands, up, c, r.x, r.y, s, node);
    }
}

fn full(w: f32, h: f32, s: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Px(w * s), height: Val::Px(h * s), ..default() }
}

fn layer(commands: &mut Commands, up: &mut Uploads, p: &Paint, w: f32, h: f32, s: f32, parent: Entity) {
    match p {
        Paint::Fill { color, sprite, sliced, preserve_aspect, gradient, radius, graphic } => {
            let mut node = full(w, h, s);
            node.border_radius = BorderRadius::px(radius[0] * s, radius[1] * s, radius[2] * s, radius[3] * s);
            let mut e = commands.spawn(ChildOf(parent));
            match sprite {
                Some(sp) => {
                    if *preserve_aspect && !*sliced {
                        // fit the sprite's aspect inside the rect, centred
                        let k = (w / sp.width as f32).min(h / sp.height as f32);
                        let (fw, fh) = (sp.width as f32 * k, sp.height as f32 * k);
                        node.left = Val::Px((w - fw) / 2.0 * s);
                        node.top = Val::Px((h - fh) / 2.0 * s);
                        node.width = Val::Px(fw * s);
                        node.height = Val::Px(fh * s);
                    }
                    let mode = if *sliced {
                        let b = sp.border;
                        // corners drawn at the size the canvas gives them: border pixels over the sprite's pixels per unit
                        let k = 100.0 / sp.pixels_per_unit * s;
                        NodeImageMode::Sliced(TextureSlicer {
                            border: BorderRect { min_inset: Vec2::new(b[0], b[3]), max_inset: Vec2::new(b[2], b[1]) },
                            center_scale_mode: SliceScaleMode::Stretch,
                            sides_scale_mode: SliceScaleMode::Stretch,
                            max_corner_scale: k,
                        })
                    } else {
                        NodeImageMode::Stretch
                    };
                    let tint = gradient.first().map_or(*color, |g| g.0);
                    e.insert((node, ImageNode { color: rgba(tint), image: up.sprite(sp), image_mode: mode, ..default() }, Graphic { id: *graphic, color: tint, stops: vec![] }));
                }
                None if gradient.len() > 1 => {
                    let stops = gradient.iter().map(|(c, t)| ColorStop::new(rgba(*c), Val::Percent(t * 100.0))).collect();
                    let g = LinearGradient { angle: LinearGradient::TO_RIGHT, stops, ..default() };
                    e.insert((node, BackgroundGradient(vec![Gradient::Linear(g)]), Graphic { id: *graphic, color: *color, stops: gradient.iter().map(|g| g.0).collect() }));
                }
                None => {
                    let c = gradient.first().map_or(*color, |g| g.0);
                    e.insert((node, BackgroundColor(rgba(c)), Graphic { id: *graphic, color: c, stops: vec![] }));
                }
            }
        }
        Paint::Tiles { sprite, tint, rows } => {
            // INFERRED: square tiles, `rows` of them down the box; the pattern's spacing, angle and drift are not drawn yet
            let tile = h * s / rows.max(1.0);
            let image_mode = NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: tile / sprite.height.max(1) as f32 };
            commands.spawn((full(w, h, s), ImageNode { color: rgba(*tint), image: up.sprite(sprite), image_mode, ..default() }, ChildOf(parent)));
        }
        Paint::Text { text, font, size, color, horizontal, vertical, wrap, margin, graphic } => {
            let mut node = full(w - margin[0] - margin[2], h - margin[1] - margin[3], s);
            node.left = Val::Px(margin[0] * s);
            node.top = Val::Px(margin[1] * s);
            node.flex_direction = FlexDirection::Column;
            node.justify_content = match vertical { Align::Start => JustifyContent::FlexStart, Align::Middle => JustifyContent::Center, Align::End => JustifyContent::FlexEnd };
            let boxed = commands.spawn((node, ChildOf(parent))).id();
            let justify = match horizontal { Align::Start => Justify::Left, Align::Middle => Justify::Center, Align::End => Justify::Right };
            let mut tf = TextFont { font_size: FontSize::Px(size * s), ..default() };
            if let Some(f) = font {
                tf.font = up.font(f).into();
            }
            commands.spawn((
                Text::new(text.clone()),
                tf,
                TextColor(rgba(*color)),
                Graphic { id: *graphic, color: *color, stops: vec![] },
                TextLayout::new(justify, if *wrap { LineBreak::WordBoundary } else { LineBreak::NoWrap }),
                Node { width: Val::Percent(100.0), ..default() },
                ChildOf(boxed),
            ));
        }
    }
}

/// Gives each graphic of `page` its colour times the tint its selectable's state gives it (gradients keep theirs).
pub fn tint(page: &Drawn, pointer: &Pointer, graphics: &mut Query<Tinted>) {
    let mut tints = HashMap::new();
    for i in selectables(page) {
        tints.extend(i.tints(pointer.state(i)));
    }
    for (g, image, background, text, gradient) in graphics.iter_mut() {
        let t = tints.get(&g.id).copied().unwrap_or([1.0; 4]);
        let times = |c: [f32; 4]| rgba([0, 1, 2, 3].map(|k| c[k] * t[k]));
        let c = times(g.color);
        // every node has a background colour: only a plain fill draws with it
        if let Some(mut i) = image {
            if i.color != c {
                i.color = c;
            }
        } else if let Some(mut x) = text {
            if x.0 != c {
                x.0 = c;
            }
        } else if let Some(mut gr) = gradient {
            let colors: Vec<Color> = g.stops.iter().map(|s| times(*s)).collect();
            if let Some(Gradient::Linear(l)) = gr.bypass_change_detection().0.first_mut() {
                if l.stops.iter().zip(&colors).any(|(s, c)| s.color != *c) {
                    l.stops.iter_mut().zip(colors).for_each(|(s, c)| s.color = c);
                    gr.set_changed();
                }
            }
        } else if let Some(mut b) = background.filter(|b| b.0 != c) {
            b.0 = c;
        }
    }
}

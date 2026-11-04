//! Puts a painted page on screen with Bevy UI: one absolutely placed node per element, its fills and texts as layers
//! under its children, scaled to fit the window and centred.
use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::text::{FontSize, LineBreak};
use bevy::ui::{BackgroundGradient, ColorStop, Gradient, LinearGradient};

use super::paint::{self, Align, Drawn, Paint};

/// The root of a page on screen.
#[derive(Component)]
pub struct Page;

/// Handles made for one page, so a sprite or font used twice is uploaded once.
struct Uploads<'a> {
    images: &'a mut Assets<Image>,
    fonts: &'a mut Assets<Font>,
    sprite: HashMap<usize, Handle<Image>>,
    font: HashMap<usize, Handle<Font>>,
}

impl Uploads<'_> {
    fn sprite(&mut self, s: &Arc<paint::Sprite>) -> Handle<Image> {
        let images = &mut *self.images;
        self.sprite.entry(Arc::as_ptr(s) as usize).or_insert_with(|| {
            images.add(Image::new(
                Extent3d { width: s.width, height: s.height, depth_or_array_layers: 1 },
                TextureDimension::D2,
                s.rgba.clone(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            ))
        }).clone()
    }

    fn font(&mut self, f: &Arc<paint::Font>) -> Handle<Font> {
        let fonts = &mut *self.fonts;
        self.font.entry(Arc::as_ptr(f) as usize).or_insert_with(|| fonts.add(Font::from_bytes(f.ttf.to_vec()))).clone()
    }
}

fn rgba(c: [f32; 4]) -> Color {
    Color::srgba(c[0], c[1], c[2], c[3])
}

/// Spawns `page` (laid out on a `canvas`-sized surface) fitted into `window`, returning the root entity.
pub fn spawn(commands: &mut Commands, images: &mut Assets<Image>, fonts: &mut Assets<Font>, page: &Drawn, window: Vec2) -> Entity {
    let (w, h) = (page.rect.w.max(1.0), page.rect.h.max(1.0));
    let scale = (window.x * 0.9 / w).min(window.y * 0.9 / h);
    let mut up = Uploads { images, fonts, sprite: HashMap::new(), font: HashMap::new() };
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
        Paint::Fill { color, sprite, sliced, preserve_aspect, gradient, radius } => {
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
                    e.insert((node, ImageNode { color: rgba(tint), image: up.sprite(sp), image_mode: mode, ..default() }));
                }
                None if gradient.len() > 1 => {
                    let stops = gradient.iter().map(|(c, t)| ColorStop::new(rgba(*c), Val::Percent(t * 100.0))).collect();
                    let g = LinearGradient { angle: LinearGradient::TO_RIGHT, stops, ..default() };
                    e.insert((node, BackgroundGradient(vec![Gradient::Linear(g)])));
                }
                None => {
                    let c = gradient.first().map_or(*color, |g| g.0);
                    e.insert((node, BackgroundColor(rgba(c))));
                }
            }
        }
        Paint::Tiles { sprite, tint, rows } => {
            // INFERRED: square tiles, `rows` of them down the box; the pattern's spacing, angle and drift are not drawn yet
            let tile = h * s / rows.max(1.0);
            let image_mode = NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: tile / sprite.height.max(1) as f32 };
            commands.spawn((full(w, h, s), ImageNode { color: rgba(*tint), image: up.sprite(sprite), image_mode, ..default() }, ChildOf(parent)));
        }
        Paint::Text { text, font, size, color, horizontal, vertical, wrap, margin } => {
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
                TextLayout::new(justify, if *wrap { LineBreak::WordBoundary } else { LineBreak::NoWrap }),
                Node { width: Val::Percent(100.0), ..default() },
                ChildOf(boxed),
            ));
        }
    }
}

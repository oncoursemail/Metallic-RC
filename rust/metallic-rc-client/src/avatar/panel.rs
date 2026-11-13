//! The avatar editor: C opens it (the camera turns round to face the robot), the mouse picks the colour zones (shell,
//! shirt, pants, screen, accent) from swatches and the eyes, head accessory and outfit from chips, Done (or C, or
//! Enter) closes it and saves the look to
//! `%LOCALAPPDATA%\Metallic-RC\avatar.json`; the room hears the new look at once. The keys still work too: Up/Down pick
//! a row, Left/Right step through it.
use bevy::{
    prelude::*,
    ui::{RelativeCursorPosition, UiTransform, Val2},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use super::look::{Accessory, Eyes, Look, Outfit, ACCENTS, PANTS, SHIRTS, SKINS, VISORS};

/// The local player's look; changing it repaints the avatar and tells the room.
#[derive(Resource, Clone, Copy)]
pub struct LocalLook(pub Look);

#[derive(Resource, Default)]
pub struct LookPanel {
    pub open: Option<Entity>,
    row: usize,
}

const ROWS: [&str; 8] = ["Shell", "Shirt", "Pants", "Screen", "Lights", "Eyes", "Head", "Body"];
/// The colour rows' palettes, in row order.
const PALETTES: [&[[u8; 3]; 8]; 5] = [&SKINS, &SHIRTS, &PANTS, &VISORS, &ACCENTS];

/// A colour row's colour in a look.
fn zone(l: &mut Look, row: usize) -> &mut [u8; 3] {
    match row {
        0 => &mut l.skin,
        1 => &mut l.shirt,
        2 => &mut l.pants,
        3 => &mut l.visor,
        _ => &mut l.accent,
    }
}

/// `list`'s entry `by` steps from `c` (round the end); one not on the list starts it again.
fn step<T: PartialEq + Copy>(list: &[T], c: T, by: i32) -> T {
    let i = list.iter().position(|&x| x == c).map_or(0, |i| (i as i32 + by).rem_euclid(list.len() as i32) as usize);
    list[i]
}

/// The look with row `row` stepped `by` choices.
pub fn change(mut l: Look, row: usize, by: i32) -> Look {
    match row {
        0..5 => *zone(&mut l, row) = step(PALETTES[row], *zone(&mut l, row), by),
        5 => l.eyes = step(Eyes::ALL, l.eyes, by),
        6 => l.accessory = step(Accessory::ALL, l.accessory, by),
        _ => l.outfit = step(Outfit::ALL, l.outfit, by),
    }
    l
}

/// What a click on a part of the editor does.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Colour(usize, [u8; 3]),
    Eyes(Eyes),
    Extra(Accessory),
    Outfit(Outfit),
    Done,
}

impl Pick {
    fn apply(self, mut l: Look) -> Look {
        match self {
            Pick::Colour(row, c) => *zone(&mut l, row) = c,
            Pick::Eyes(e) => l.eyes = e,
            Pick::Extra(a) => l.accessory = a,
            Pick::Outfit(o) => l.outfit = o,
            Pick::Done => {}
        }
        l
    }
}

const INK: Color = Color::srgb(0.93, 0.95, 0.98);
const DIM: Color = Color::srgb(0.62, 0.66, 0.74);
const EDGE: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);

fn rgb(c: [u8; 3]) -> Color {
    Color::srgb_u8(c[0], c[1], c[2])
}

fn draw(commands: &mut Commands, l: &Look, row: usize) -> Entity {
    let text = |s: &str, size: f32, colour: Color| (Text::new(s), TextFont::from_font_size(size), TextColor(colour));
    let mut chosen = *l;
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(24.0),
            top: percent(50.0),
            width: px(372.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(10.0),
            padding: UiRect::all(px(18.0)),
            border: UiRect::all(px(1.0)),
            border_radius: BorderRadius::all(px(14.0)),
            ..default()
        },
        UiTransform { translation: Val2::percent(0.0, -50.0), ..default() },
        BackgroundColor(Color::srgba(0.05, 0.06, 0.09, 0.88)),
        BorderColor::all(EDGE),
    )).with_children(|p| {
        p.spawn(text("Your robot", 22.0, INK));
        for (i, name) in ROWS.iter().enumerate() {
            let label = if i == row { rgb(l.accent) } else { DIM };
            // colour rows: the name beside its swatches; choice rows: the name over its chips
            let colours = i < PALETTES.len();
            let line = Node { flex_direction: if colours { FlexDirection::Row } else { FlexDirection::Column }, align_items: if colours { AlignItems::Center }
                else { AlignItems::Start }, row_gap: px(5.0), ..default() };
            p.spawn(line).with_children(|r| {
                r.spawn((Node { width: px(62.0), ..default() },)).with_children(|n| {
                    n.spawn(text(name, 13.0, label));
                });
                r.spawn(Node { column_gap: px(5.0), row_gap: px(5.0), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|row| {
                    if colours {
                        let now = *zone(&mut chosen, i);
                        for &c in PALETTES[i] {
                            let on = c == now;
                            row.spawn((
                                Node {
                                    width: px(28.0),
                                    height: px(24.0),
                                    border: UiRect::all(px(if on { 3.0 } else { 1.0 })),
                                    border_radius: BorderRadius::all(px(7.0)),
                                    ..default()
                                },
                                BackgroundColor(rgb(c)),
                                BorderColor::all(if on { INK } else { EDGE }),
                                RelativeCursorPosition::default(),
                                Pick::Colour(i, c),
                            ));
                        }
                    } else {
                        let chips: Vec<(&str, bool, Pick)> = match i {
                            5 => Eyes::ALL.iter().map(|&e| (e.name(), e == l.eyes, Pick::Eyes(e))).collect(),
                            6 => Accessory::ALL.iter().map(|&a| (a.name(), a == l.accessory, Pick::Extra(a))).collect(),
                            _ => Outfit::ALL.iter().map(|&o| (o.name(), o == l.outfit, Pick::Outfit(o))).collect(),
                        };
                        for (name, on, pick) in chips {
                            row.spawn((
                                Node {
                                    padding: UiRect::axes(px(10.0), px(4.0)),
                                    border: UiRect::all(px(1.0)),
                                    border_radius: BorderRadius::all(px(13.0)),
                                    ..default()
                                },
                                BackgroundColor(if on { rgb(l.accent).with_alpha(0.25) } else { Color::NONE }),
                                BorderColor::all(if on { rgb(l.accent) } else { EDGE }),
                                RelativeCursorPosition::default(),
                                pick,
                            )).with_children(|chip| {
                                chip.spawn(text(name, 13.0, if on { INK } else { DIM }));
                            });
                        }
                    }
                });
            });
        }
        p.spawn((
            Node {
                margin: UiRect::top(px(4.0)),
                padding: UiRect::axes(px(0.0), px(9.0)),
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(px(9.0)),
                ..default()
            },
            BackgroundColor(rgb(l.accent)),
            RelativeCursorPosition::default(),
            Pick::Done,
        )).with_children(|b| {
            b.spawn(text("Done", 16.0, Color::srgb(0.05, 0.06, 0.09)));
        });
        p.spawn(text("Click to choose. C or Enter: done", 12.0, DIM));
    }).id()
}

/// The part of the editor under the mouse.
fn picked(parts: &Query<(&Pick, &RelativeCursorPosition)>) -> Option<Pick> {
    parts.iter().find(|(_, at)| at.cursor_over()).map(|(p, _)| *p)
}

pub fn look_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    parts: Query<(&Pick, &RelativeCursorPosition)>,
    mut panel: ResMut<LookPanel>,
    mut look: ResMut<LocalLook>,
) {
    let Some(open) = panel.open else {
        if keys.just_pressed(KeyCode::KeyC) {
            panel.open = Some(draw(&mut commands, &look.0, panel.row));
        }
        return;
    };
    let click = if mouse.just_pressed(MouseButton::Left) { picked(&parts) } else { None };
    if keys.any_just_pressed([KeyCode::KeyC, KeyCode::Enter]) || click == Some(Pick::Done) {
        commands.entity(open).despawn();
        panel.open = None;
        if let Err(e) = look.0.save() {
            eprintln!("metallic-rc-client: saving the look: {e}");
        }
        return;
    }
    let (row, by) = (
        (panel.row as i32 + keys.just_pressed(KeyCode::ArrowDown) as i32 - keys.just_pressed(KeyCode::ArrowUp) as i32).rem_euclid(ROWS.len() as i32) as usize,
        keys.just_pressed(KeyCode::ArrowRight) as i32 - keys.just_pressed(KeyCode::ArrowLeft) as i32,
    );
    let before = look.0;
    if by != 0 {
        look.0 = change(look.0, row, by);
    }
    if let Some(p) = click {
        look.0 = p.apply(look.0);
    }
    if row != panel.row || look.0 != before {
        panel.row = row;
        commands.entity(open).despawn();
        panel.open = Some(draw(&mut commands, &look.0, row));
    }
}

/// While the panel is open the mouse is free to click it (after the game's own grabbing, so a click on the panel does
/// not take the mouse); closing it takes the mouse back for looking.
pub fn free_cursor(panel: Res<LookPanel>, mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, mut was_open: Local<bool>) {
    let open = panel.open.is_some();
    if open {
        if cursor.grab_mode != CursorGrabMode::None || !cursor.visible {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    } else if *was_open {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    *was_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_step_round_their_choices() {
        let l = Look::default();
        assert_eq!(change(l, 0, 1).skin, SKINS[1]);
        assert_eq!(change(l, 1, -1).shirt, SHIRTS[7]);
        assert_eq!(change(change(l, 4, 3), 4, -3), l);
        // a colour not on the palette (from --look) starts it again
        assert_eq!(change(Look { skin: [1, 2, 3], ..l }, 0, 1).skin, SKINS[0]);
        assert_eq!(change(l, 2, 1).pants, PANTS[1]);
        assert_eq!(change(l, 3, -1).visor, VISORS[7]);
        assert_eq!(change(l, 4, 1).accent, ACCENTS[1]);
        assert_eq!(change(l, 5, 2).eyes, Eyes::Happy);
        assert_eq!(change(Look { accessory: Accessory::Goggles, ..l }, 6, 1).accessory, Accessory::None);
        assert_eq!(change(l, 7, -1).outfit, Outfit::Bowtie);
    }

    #[test]
    fn clicks_set_the_part_they_are_on() {
        let l = Look::default();
        assert_eq!(Pick::Colour(0, SKINS[3]).apply(l).skin, SKINS[3]);
        assert_eq!(Pick::Colour(1, SHIRTS[2]).apply(l).shirt, SHIRTS[2]);
        assert_eq!(Pick::Colour(2, PANTS[4]).apply(l).pants, PANTS[4]);
        assert_eq!(Pick::Colour(3, VISORS[2]).apply(l).visor, VISORS[2]);
        assert_eq!(Pick::Colour(4, ACCENTS[5]).apply(l), Look { accent: ACCENTS[5], ..l });
        assert_eq!(Pick::Extra(Accessory::Halo).apply(l).accessory, Accessory::Halo);
        assert_eq!(Pick::Eyes(Eyes::Wide).apply(l).eyes, Eyes::Wide);
        assert_eq!(Pick::Outfit(Outfit::Scarf).apply(l).outfit, Outfit::Scarf);
        assert_eq!(Pick::Done.apply(l), l);
    }
}

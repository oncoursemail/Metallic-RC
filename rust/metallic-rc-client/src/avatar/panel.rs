//! The look panel: C opens it; Up/Down pick a row, Left/Right step through its choices, C or Enter closes it and saves
//! the look to `%LOCALAPPDATA%\Metallic-RC\avatar.json`.
use bevy::prelude::*;

use super::look::{Accessory, Look, ACCENTS, SHIRTS, SKINS};

/// The local player's look; changing it repaints the avatar and tells the room.
#[derive(Resource, Clone, Copy)]
pub struct LocalLook(pub Look);

#[derive(Resource, Default)]
pub struct LookPanel {
    pub open: Option<Entity>,
    row: usize,
}

const ROWS: [&str; 4] = ["Shell", "Shirt", "Accent", "Extra"];

fn step(list: &[[u8; 3]], c: [u8; 3], by: i32) -> [u8; 3] {
    let i = list.iter().position(|&x| x == c).map_or(0, |i| (i as i32 + by).rem_euclid(list.len() as i32) as usize);
    list[i]
}

/// The look with row `row` stepped `by` choices.
pub fn change(l: Look, row: usize, by: i32) -> Look {
    match row {
        0 => Look { skin: step(&SKINS, l.skin, by), ..l },
        1 => Look { shirt: step(&SHIRTS, l.shirt, by), ..l },
        2 => Look { accent: step(&ACCENTS, l.accent, by), ..l },
        _ => {
            let i = Accessory::ALL.iter().position(|&a| a == l.accessory).unwrap_or(0) as i32;
            Look { accessory: Accessory::ALL[(i + by).rem_euclid(4) as usize], ..l }
        }
    }
}

fn draw(commands: &mut Commands, l: &Look, row: usize) -> Entity {
    let text = |s: String| (Text::new(s), TextFont::from_font_size(18.0), TextColor(Color::WHITE));
    let swatch = |c: [u8; 3]| (Node { width: px(22.0), height: px(22.0), ..default() }, BackgroundColor(Color::srgb_u8(c[0], c[1], c[2])));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(16.0),
            top: px(60.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(8.0),
            padding: UiRect::all(px(14.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.05, 0.07, 0.8)),
    )).with_children(|p| {
        p.spawn(text("Your look".into()));
        for (i, name) in ROWS.iter().enumerate() {
            let value = match i {
                0 => l.skin,
                1 => l.shirt,
                2 => l.accent,
                _ => [0; 3],
            };
            p.spawn(Node { column_gap: px(10.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                r.spawn(text(format!("{} {:<7}", if i == row { ">" } else { " " }, name)));
                r.spawn(text("<".into()));
                if i < 3 {
                    r.spawn(swatch(value));
                } else {
                    r.spawn(text(l.accessory.name().into()));
                }
                r.spawn(text(">".into()));
            });
        }
        p.spawn((Text::new("Up/Down choose, Left/Right change, C done"), TextFont::from_font_size(13.0), TextColor(Color::srgb(0.7, 0.72, 0.78))));
    }).id()
}

pub fn look_panel(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<LookPanel>, mut look: ResMut<LocalLook>) {
    let Some(open) = panel.open else {
        if keys.just_pressed(KeyCode::KeyC) {
            panel.open = Some(draw(&mut commands, &look.0, panel.row));
        }
        return;
    };
    if keys.any_just_pressed([KeyCode::KeyC, KeyCode::Enter]) {
        commands.entity(open).despawn();
        panel.open = None;
        if let Err(e) = look.0.save() {
            eprintln!("metallic-rc-client: saving the look: {e}");
        }
        return;
    }
    let (row, by) = (
        (panel.row as i32 + keys.just_pressed(KeyCode::ArrowDown) as i32 - keys.just_pressed(KeyCode::ArrowUp) as i32).rem_euclid(4) as usize,
        keys.just_pressed(KeyCode::ArrowRight) as i32 - keys.just_pressed(KeyCode::ArrowLeft) as i32,
    );
    if row != panel.row || by != 0 {
        panel.row = row;
        if by != 0 {
            look.0 = change(look.0, row, by);
        }
        commands.entity(open).despawn();
        panel.open = Some(draw(&mut commands, &look.0, row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_step_round_their_choices() {
        let l = Look::default();
        assert_eq!(change(l, 0, 1).skin, SKINS[1]);
        assert_eq!(change(l, 1, -1).shirt, SHIRTS[7]);
        assert_eq!(change(change(l, 2, 3), 2, -3), l);
        // a colour not on the palette (from --look) starts it again
        assert_eq!(change(Look { skin: [1, 2, 3], ..l }, 0, 1).skin, SKINS[0]);
        assert_eq!(change(Look { accessory: Accessory::Halo, ..l }, 3, 1).accessory, Accessory::None);
    }
}

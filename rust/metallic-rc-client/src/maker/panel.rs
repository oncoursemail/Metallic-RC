//! The Maker Pen's palette (B): the shapes and props to make, the ShapeConfig colours and materials, snap, undo and
//! redo, and the room: save, publish as a room of one's own, let the players here build. Our own card, in the
//! avatar editor's style; the mouse is free while it is open.
use bevy::{
    prelude::*,
    ui::{RelativeCursorPosition, UiTransform, Val2},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use super::{kit::PROPS, save, Item, Pen, Request, SHAPE_ITEMS};

const INK: Color = Color::srgb(0.93, 0.95, 0.98);
const DIM: Color = Color::srgb(0.62, 0.66, 0.74);
const EDGE: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
const ACCENT: Color = Color::srgb(1.0, 0.6, 0.15);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Item(Item),
    Color(i32),
    Finish(i32),
    Snap,
    Act(Request),
    Done,
}

fn text(s: &str, size: f32, colour: Color) -> impl Bundle {
    (Text::new(s), TextFont::from_font_size(size), TextColor(colour))
}

fn chip(p: &mut ChildSpawnerCommands, label: &str, on: bool, pick: Pick) {
    p.spawn((
        Node { padding: UiRect::axes(px(9.0), px(3.0)), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(11.0)), ..default() },
        BackgroundColor(if on { ACCENT.with_alpha(0.25) } else { Color::NONE }),
        BorderColor::all(if on { ACCENT } else { EDGE }),
        RelativeCursorPosition::default(),
        pick,
    )).with_children(|c| {
        c.spawn(text(label, 12.0, if on { INK } else { DIM }));
    });
}

fn row(p: &mut ChildSpawnerCommands, title: &str, fill: impl FnOnce(&mut ChildSpawnerCommands)) {
    p.spawn(text(title, 13.0, DIM));
    p.spawn(Node { column_gap: px(5.0), row_gap: px(5.0), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(fill);
}

fn draw(commands: &mut Commands, pen: &Pen) -> Entity {
    let room = match (&pen.info, save::link()) {
        (Some(i), Some(l)) if i.room == l.room => format!("{}  |  {}  |  {}", i.room, i.owner.as_deref().map_or("no owner".into(), |o| format!("owner {o}")),
            if i.can_edit { "you can build" } else { "view only" }),
        (_, Some(l)) => format!("{}  |  asking the server ...", l.room),
        _ => "offline".into(),
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(24.0),
            top: percent(50.0),
            width: px(540.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(7.0),
            padding: UiRect::all(px(16.0)),
            border: UiRect::all(px(1.0)),
            border_radius: BorderRadius::all(px(14.0)),
            ..default()
        },
        UiTransform { translation: Val2::percent(0.0, -50.0), ..default() },
        BackgroundColor(Color::srgba(0.05, 0.06, 0.09, 0.9)),
        BorderColor::all(EDGE),
    )).with_children(|p| {
        p.spawn(text("Maker Pen", 22.0, INK));
        p.spawn(text(&room, 12.0, DIM));
        row(p, "Shapes", |r| {
            for it in SHAPE_ITEMS {
                chip(r, it.label(), pen.item == it, Pick::Item(it));
            }
        });
        row(p, "Props", |r| {
            for i in 0..PROPS.len() {
                chip(r, PROPS[i].label, pen.item == Item::Prop(i), Pick::Item(Item::Prop(i)));
            }
        });
        row(p, &format!("Colour: {}", pen.color_name(pen.color)), |r| {
            for c in &pen.colors {
                let on = c.id == pen.color;
                r.spawn((
                    Node { width: px(22.0), height: px(18.0), border: UiRect::all(px(if on { 3.0 } else { 1.0 })), border_radius: BorderRadius::all(px(5.0)), ..default() },
                    BackgroundColor(Color::srgb(c.rgb[0], c.rgb[1], c.rgb[2])),
                    BorderColor::all(if on { INK } else { EDGE }),
                    RelativeCursorPosition::default(),
                    Pick::Color(c.id),
                ));
            }
        });
        row(p, "Material", |r| {
            let mut f = pen.finishes.clone();
            f.sort_by_key(|f| f.id);
            for f in f {
                chip(r, &f.name, f.id == pen.material, Pick::Finish(f.id));
            }
        });
        row(p, "Edit and room", |r| {
            chip(r, if pen.snap { "Snap: on" } else { "Snap: off" }, pen.snap, Pick::Snap);
            chip(r, "Undo", false, Pick::Act(Request::Undo));
            chip(r, "Redo", false, Pick::Act(Request::Redo));
            chip(r, "Save room", false, Pick::Act(Request::Save));
            chip(r, "Publish as my room", false, Pick::Act(Request::Publish));
            chip(r, "Let players here build", false, Pick::Act(Request::Builders));
        });
        p.spawn((
            Node { margin: UiRect::top(px(4.0)), padding: UiRect::axes(px(0.0), px(8.0)), justify_content: JustifyContent::Center, border_radius: BorderRadius::all(px(9.0)), ..default() },
            BackgroundColor(ACCENT),
            RelativeCursorPosition::default(),
            Pick::Done,
        )).with_children(|b| {
            b.spawn(text("Done", 15.0, Color::srgb(0.05, 0.06, 0.09)));
        });
        p.spawn(text("Click to choose. B: close", 12.0, DIM));
    }).id()
}

/// B opens and closes the palette (with the pen out); clicks choose.
pub fn panel(mut commands: Commands, time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>,
             parts: Query<(&Pick, &RelativeCursorPosition)>, mut pen: ResMut<Pen>, mut last_redraw: Local<Option<(u64, bool)>>) {
    let Some(open) = pen.panel else {
        if pen.equipped && keys.just_pressed(KeyCode::KeyB) {
            pen.panel = Some(draw(&mut commands, &pen));
            *last_redraw = Some(info_key(&pen));
        }
        return;
    };
    let click = if mouse.just_pressed(MouseButton::Left) { parts.iter().find(|(_, at)| at.cursor_over()).map(|(p, _)| *p) } else { None };
    if keys.just_pressed(KeyCode::KeyB) || click == Some(Pick::Done) || !pen.equipped {
        commands.entity(open).despawn();
        pen.panel = None;
        return;
    }
    let now = time.elapsed_secs_f64();
    match click {
        Some(Pick::Item(it)) => pen.item = it,
        Some(Pick::Color(c)) => pen.color = c,
        Some(Pick::Finish(f)) => pen.material = f,
        Some(Pick::Snap) => pen.snap = !pen.snap,
        Some(Pick::Act(r)) => {
            pen.request = Some(r);
            pen.say("...", now);
        }
        _ => {}
    }
    // redraw on a choice, or when the server's answer about the room arrives
    if click.is_some() || *last_redraw != Some(info_key(&pen)) {
        commands.entity(open).despawn();
        pen.panel = Some(draw(&mut commands, &pen));
        *last_redraw = Some(info_key(&pen));
    }
}

fn info_key(pen: &Pen) -> (u64, bool) {
    pen.info.as_ref().map_or((u64::MAX, false), |i| (i.version, i.can_edit))
}

/// While the palette is open the mouse is free to click it; closing it takes the mouse back for looking.
pub fn free_cursor(pen: Res<Pen>, mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, mut was_open: Local<bool>) {
    let open = pen.panel.is_some();
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

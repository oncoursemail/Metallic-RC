//! The match on screen (Bevy UI, our own layout): team scores and the timer at the top, this player's team under
//! them, ammo bottom right (the gun has no magazine: `supportsAmmunition` off, so infinite), a hit marker in the
//! middle when one of this player's paintballs hits, the kill feed top right, a banner for phase changes and
//! captures, a grey screen while out, the keys in the lobby, and the results table after a round.
use bevy::{prelude::*, ui::UiTransform};
use rr_net::games::{Phase, BLUE, RED};

use super::{mode_name, paintball::PaintballGun, Game, TEAM_COLOURS, TEAM_NAMES};
use crate::interact::Held;

#[derive(Component)]
enum Hud {
    Root,
    Red,
    Blue,
    Timer,
    Team,
    Ammo,
    Marker,
    Feed,
    Banner,
    Out,
    OutText,
    Hint,
    Results,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(Update, show.after(super::pump));
    }
}

fn text(s: &str, size: f32, colour: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont::from_font_size(size), TextColor(colour))
}

fn spawn(mut commands: Commands) {
    let root = commands.spawn((Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() }, Hud::Root, Visibility::Hidden, Pickable::IGNORE)).id();
    // grey while out
    let out = commands.spawn((Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(Color::srgba(0.15, 0.15, 0.18, 0.55)), Visibility::Hidden, Hud::Out)).id();
    let out_text = commands.spawn((text("", 44.0, Color::WHITE), Hud::OutText)).id();
    commands.entity(out).add_child(out_text);
    // scores and timer
    let top = commands.spawn(Node { position_type: PositionType::Absolute, top: px(10.0), width: percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }).id();
    let row = commands.spawn((Node { column_gap: px(14.0), padding: UiRect::axes(px(16.0), px(6.0)), border_radius: BorderRadius::all(px(10.0)), align_items: AlignItems::Center, ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)))).id();
    let red = commands.spawn((text("0", 34.0, TEAM_COLOURS[0]), Hud::Red)).id();
    let timer = commands.spawn((text("", 26.0, Color::WHITE), Hud::Timer)).id();
    let blue = commands.spawn((text("0", 34.0, TEAM_COLOURS[1]), Hud::Blue)).id();
    commands.entity(row).add_children(&[red, timer, blue]);
    let team = commands.spawn((text("", 18.0, Color::WHITE), Hud::Team)).id();
    let banner = commands.spawn((text("", 40.0, Color::WHITE), Node { margin: UiRect::top(px(90.0)), ..default() }, Hud::Banner)).id();
    commands.entity(top).add_children(&[row, team, banner]);
    // hit marker: an X of two bars round the middle
    let marker = commands.spawn((Node { position_type: PositionType::Absolute, left: percent(50.0), top: percent(50.0), width: px(0.0), height: px(0.0), ..default() }, Visibility::Hidden, Hud::Marker)).id();
    for deg in [45.0f32, -45.0] {
        let bar = commands.spawn((Node { position_type: PositionType::Absolute, left: px(-14.0), top: px(-1.5), width: px(28.0), height: px(3.0), ..default() },
            UiTransform { rotation: Rot2::degrees(deg), ..default() }, BackgroundColor(Color::srgb(1.0, 0.95, 0.3)))).id();
        commands.entity(marker).add_child(bar);
    }
    let feed = commands.spawn((text("", 17.0, Color::WHITE), Node { position_type: PositionType::Absolute, top: px(12.0), right: px(14.0), ..default() }, TextLayout::justify(Justify::Right), Hud::Feed)).id();
    let ammo = commands.spawn((text("", 24.0, Color::WHITE), Node { position_type: PositionType::Absolute, bottom: px(40.0), right: px(16.0), ..default() }, Hud::Ammo)).id();
    let hint = commands.spawn((text("", 16.0, Color::srgba(1.0, 1.0, 1.0, 0.85)), Node { position_type: PositionType::Absolute, bottom: px(14.0), width: percent(100.0), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center), Hud::Hint)).id();
    let results = commands.spawn((text("", 20.0, Color::WHITE), Node { position_type: PositionType::Absolute, top: px(150.0), left: percent(30.0), width: percent(40.0), padding: UiRect::all(px(16.0)), border_radius: BorderRadius::all(px(10.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)), Visibility::Hidden, Hud::Results)).id();
    commands.entity(root).add_children(&[out, top, marker, feed, ammo, hint, results]);
}

fn clock(secs: f32) -> String {
    let s = secs.ceil().max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Whether this client sits on a Paintball map (`--world` one of them), for the start keys hint.
fn on_paintball_map() -> bool {
    matches!(super::map::world_arg().as_deref(), Some("level13" | "level14" | "level15" | "level16" | "level17" | "level102"))
}

#[allow(clippy::type_complexity)]
fn show(game: Res<Game>, held: Query<(), (With<PaintballGun>, With<Held>)>, mut parts: Query<(&Hud, Option<&mut Text>, &mut Visibility, Option<&mut TextColor>)>) {
    let state = game.state.as_ref();
    let me = game.me.and_then(|m| game.member(m));
    let set = |t: &mut Option<Mut<Text>>, s: String| {
        if let Some(t) = t {
            if t.0 != s {
                t.0 = s;
            }
        }
    };
    for (hud, mut t, mut vis, colour) in &mut parts {
        let shown = match hud {
            Hud::Root => state.is_some() || on_paintball_map(),
            Hud::Red | Hud::Blue => {
                let team = if matches!(hud, Hud::Red) { RED } else { BLUE };
                set(&mut t, state.map_or("0".into(), |s| s.scores[team as usize].to_string()));
                state.is_some()
            }
            Hud::Timer => {
                set(&mut t, match state {
                    Some(s) => match s.phase {
                        Phase::Lobby => format!("{}  -  lobby", mode_name(s.mode)),
                        Phase::Countdown => format!("starts in {}", s.time_left.ceil() as u32),
                        Phase::Playing => format!("{}   {}  (to {})", clock(s.time_left), mode_name(s.mode), s.target),
                        Phase::RoundEnd | Phase::Results => "round over".into(),
                    },
                    None => String::new(),
                });
                state.is_some()
            }
            Hud::Team => {
                set(&mut t, me.map_or(String::new(), |m| format!("You are on {}", TEAM_NAMES[m.team as usize])));
                if let (Some(mut c), Some(m)) = (colour, me) {
                    c.0 = TEAM_COLOURS[m.team as usize];
                }
                me.is_some()
            }
            Hud::Ammo => {
                set(&mut t, "Paint  \u{221e}".into());
                !held.is_empty()
            }
            Hud::Marker => game.hit_marker > 0.0,
            Hud::Feed => {
                set(&mut t, game.feed.iter().map(|f| f.1.as_str()).collect::<Vec<_>>().join("\n"));
                !game.feed.is_empty()
            }
            Hud::Banner => {
                set(&mut t, game.banner.as_ref().map_or(String::new(), |b| b.1.clone()));
                game.banner.is_some()
            }
            Hud::Out => me.is_some_and(|m| m.out),
            Hud::OutText => {
                set(&mut t, "You're out!  Back at your base in a moment".into());
                true
            }
            Hud::Hint => {
                let s = match state.map(|s| s.phase) {
                    None | Some(Phase::Lobby) => "F6 Team Battle    F7 Capture the Flag    F8 add bots (Shift: remove)    F10 switch team",
                    Some(Phase::Playing | Phase::Countdown) => "Left click a paintball gun to pick it up, left click to fire, Q to drop    F9 end the match",
                    _ => "",
                };
                set(&mut t, s.into());
                !s.is_empty()
            }
            Hud::Results => {
                let Some(s) = state.filter(|s| s.phase == Phase::Results) else {
                    vis.set_if_neq(Visibility::Hidden);
                    continue;
                };
                let mut rows: Vec<_> = s.members.iter().collect();
                rows.sort_by_key(|m| (m.team, std::cmp::Reverse(m.kills + 3 * m.caps)));
                let mut lines = vec![match s.winner {
                    w @ 0..=1 => format!("{} team wins  {} - {}", TEAM_NAMES[w as usize], s.scores[0], s.scores[1]),
                    _ => format!("A draw  {} - {}", s.scores[0], s.scores[1]),
                }, String::new(), format!("{:<16} {:<5} {:>4} {:>4} {:>4}", "", "team", "hits", "outs", "caps")];
                for m in rows {
                    lines.push(format!("{:<16} {:<5} {:>4} {:>4} {:>4}", game.name(m.id), TEAM_NAMES[m.team as usize], m.kills, m.outs, m.caps));
                }
                set(&mut t, lines.join("\n"));
                true
            }
        };
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
}

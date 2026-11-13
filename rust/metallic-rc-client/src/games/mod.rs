//! Built-in game modes, run by the room server (`rr_net::games`): this side sends what the player does (start a
//! match, add bots, pick a team, fire) and shows what the server says (phase, timer, scores, teams, hits, flags).
//! Paintball is the first mode ([`paintball`]); [`hud`] draws scores, timer, team, ammo, hit marker and kill feed;
//! [`map`] reads the map's spawns, flags and field from the scene.
//!
//! Keys (screen mode, outside text fields): F6 starts Team Battle, F7 Capture the Flag, F8 adds two bots, Shift+F8
//! removes the bots, F9 goes back to the lobby, F10 switches team (lobby). The map is the room the client was
//! started in (`--world level13` .. `level17`, `level102`, read from the reference install's scene); anywhere else a
//! small practice field is laid out around the player. Test aid: `METALLIC_RC_PAINTBALL=tb|ctf[,bots=N]` starts a
//! match (and adds bots) a few seconds after joining the room.
pub mod hud;
pub mod map;
pub mod paintball;

use std::collections::HashMap;

use bevy::prelude::*;
use rr_net::games::{Down, MapInfo, Member, Mode, NoteKind, Phase, State, Up, BLUE, RED};

use crate::netobj::RoomLink;
use crate::player::{Controls, Player, View};

/// Red and blue.
pub const TEAM_COLOURS: [Color; 2] = [Color::srgb(0.93, 0.22, 0.2), Color::srgb(0.2, 0.45, 0.95)];
pub const TEAM_NAMES: [&str; 2] = ["Red", "Blue"];

/// What the server last said about the match in this room, and what to show of it.
#[derive(Resource, Default)]
pub struct Game {
    /// the room has a match (the server has sent its state)
    pub state: Option<State>,
    pub map: Option<MapInfo>,
    /// this player's actor number in the room
    pub me: Option<i32>,
    /// players' names by actor, and bots' by id
    pub names: HashMap<i32, String>,
    /// kill feed: seconds left on screen, line
    pub feed: Vec<(f32, String)>,
    /// seconds left of the hit marker
    pub hit_marker: f32,
    /// one-off line in the middle of the screen (seconds left, text)
    pub banner: Option<(f32, String)>,
    /// the map from this client's own scene, made once
    pub local_map: Option<Result<MapInfo, String>>,
}

impl Game {
    pub fn member(&self, id: i32) -> Option<&Member> {
        self.state.as_ref()?.members.iter().find(|m| m.id == id)
    }

    pub fn my_team(&self) -> Option<u8> {
        self.member(self.me?).map(|m| m.team)
    }

    pub fn phase(&self) -> Phase {
        self.state.as_ref().map_or(Phase::Lobby, |s| s.phase)
    }

    pub fn name(&self, id: i32) -> String {
        if Some(id) == self.me {
            return "You".into();
        }
        self.names.get(&id).cloned().unwrap_or_else(|| format!("Player {id}"))
    }

    fn say(&mut self, line: String) {
        self.feed.push((6.0, line));
        if self.feed.len() > 6 {
            self.feed.remove(0);
        }
    }
}

/// What the server told this frame, for the systems that draw it.
#[derive(Message, Clone, Debug)]
pub enum GameEvent {
    Shot { shooter: i32, origin: Vec3, velocity: Vec3, life: f32 },
    Hit { shooter: i32, victim: i32, at: Vec3 },
    Respawn { id: i32, at: Vec3, yaw: f32 },
}

pub struct GamesPlugin;

impl Plugin for GamesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Game>()
            .add_message::<GameEvent>()
            .add_systems(Update, (pump, keys.after(pump), autostart.after(pump)))
            .add_plugins((paintball::PaintballPlugin, hud::HudPlugin));
    }
}

/// Sends a game message to the room server, if in a room.
pub fn send(link: &RoomLink, up: Up) {
    if let Some(rt) = link.get() {
        rt.room.lock().unwrap().send_game(&up);
    }
}

/// Takes the server's game messages and the room's names.
fn pump(link: Res<RoomLink>, mut game: ResMut<Game>, mut events: MessageWriter<GameEvent>, time: Res<Time>) {
    let dt = time.delta_secs();
    game.hit_marker = (game.hit_marker - dt).max(0.0);
    game.feed.retain_mut(|f| {
        f.0 -= dt;
        f.0 > 0.0
    });
    if let Some(b) = &mut game.banner {
        b.0 -= dt;
        if b.0 <= 0.0 {
            game.banner = None;
        }
    }
    let Some(rt) = link.get() else {
        game.state = None;
        return;
    };
    let mut got = vec![];
    {
        let mut room = rt.room.lock().unwrap();
        let me = room.actor();
        if game.me != me {
            game.me = me;
        }
        for s in room.shown() {
            if game.names.get(&s.actor) != Some(&s.name) {
                game.names.insert(s.actor, s.name.clone());
            }
        }
        while let Some(b) = room.poll_game() {
            got.push(b);
        }
    }
    for b in got {
        let Ok(d) = Down::decode(&b) else { continue };
        match d {
            Down::State(s) => {
                let before = game.state.as_ref().map(|s| s.phase);
                for m in &s.members {
                    if let Some(b) = &m.bot {
                        if game.names.get(&m.id) != Some(&b.name) {
                            game.names.insert(m.id, b.name.clone());
                        }
                    }
                }
                if before != Some(s.phase) {
                    let line = match s.phase {
                        Phase::Countdown => Some(format!("{} - get ready", mode_name(s.mode))),
                        Phase::Playing => Some("Go!".to_string()),
                        Phase::RoundEnd if s.winner < 2 => Some(format!("{} team wins!", TEAM_NAMES[s.winner as usize])),
                        Phase::RoundEnd => Some("A draw".to_string()),
                        _ => None,
                    };
                    game.banner = line.map(|l| (3.0, l));
                }
                game.state = Some(s);
            }
            Down::Map(m) => game.map = Some(m),
            Down::Shot { shooter, origin, velocity, life } => {
                events.write(GameEvent::Shot { shooter, origin: origin.into(), velocity: velocity.into(), life });
            }
            Down::Hit { shooter, victim, at } => {
                if Some(shooter) == game.me {
                    game.hit_marker = 0.35;
                }
                let line = format!("{} hit {}", game.name(shooter), game.name(victim));
                game.say(line);
                events.write(GameEvent::Hit { shooter, victim, at: at.into() });
            }
            Down::Respawn { id, spot } => {
                events.write(GameEvent::Respawn { id, at: spot.pos.into(), yaw: spot.yaw });
            }
            Down::Note { kind, actor, team } => {
                let (who, flag) = (game.name(actor), TEAM_NAMES[team as usize]);
                let line = match kind {
                    NoteKind::FlagTaken => format!("{who} took the {flag} flag"),
                    NoteKind::FlagDropped => format!("{who} dropped the {flag} flag"),
                    NoteKind::FlagReturned if actor == 0 => format!("The {flag} flag went home"),
                    NoteKind::FlagReturned => format!("{who} returned the {flag} flag"),
                    NoteKind::Captured => format!("{who} captured the {flag} flag!"),
                };
                if kind == NoteKind::Captured {
                    game.banner = Some((2.5, line.clone()));
                }
                game.say(line);
            }
        }
    }
}

pub fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::TeamBattle => "Team Battle",
        Mode::CaptureTheFlag => "Capture the Flag",
    }
}

/// The map to start a match on: the room's scene, else a practice field around the player.
fn map_for(game: &mut Game, feet: Vec3, yaw: f32) -> MapInfo {
    if game.local_map.is_none() {
        let read = match (map::world_arg(), crate::levels::ref_install()) {
            (Some(w), Some(data)) => map::read(data, &w),
            (Some(w), None) => Err(format!("no reference install to read {w}'s spawns from")),
            (None, _) => Err("not on a Paintball map".into()),
        };
        if let Err(e) = &read {
            warn!("paintball map: {e}; using a practice field");
        }
        game.local_map = Some(read);
    }
    match &game.local_map {
        Some(Ok(m)) => m.clone(),
        _ => map::practice(feet, yaw),
    }
}

/// Default round: Team Battle to 15 in 5 minutes, CTF to 3 in 10 (INFERRED; the game's own values are in obscured
/// fields).
fn start(link: &RoomLink, game: &mut Game, mode: Mode, feet: Vec3, yaw: f32) {
    let map = map_for(game, feet, yaw);
    let (target, seconds) = match mode {
        Mode::TeamBattle => (15, 300),
        Mode::CaptureTheFlag => (3, 600),
    };
    send(link, Up::Start { mode, target, seconds, map });
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    link: Res<RoomLink>,
    mut game: ResMut<Game>,
    player: Single<(&Player, &Transform), Without<View>>,
) {
    if controls.typing {
        return;
    }
    let (p, t) = *player;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::F6) {
        start(&link, &mut game, Mode::TeamBattle, p.feet(t), p.yaw);
    }
    if keys.just_pressed(KeyCode::F7) {
        start(&link, &mut game, Mode::CaptureTheFlag, p.feet(t), p.yaw);
    }
    if keys.just_pressed(KeyCode::F8) {
        send(&link, Up::Bots { count: if shift { 0 } else { 2 } });
    }
    if keys.just_pressed(KeyCode::F9) {
        send(&link, Up::Stop);
    }
    if keys.just_pressed(KeyCode::F10) {
        let team = if game.my_team() == Some(RED) { BLUE } else { RED };
        send(&link, Up::Team { team });
    }
}

/// `METALLIC_RC_PAINTBALL=tb|ctf[,bots=N]`: start a match a few seconds after joining (the room master only).
fn autostart(link: Res<RoomLink>, mut game: ResMut<Game>, time: Res<Time>, player: Single<(&Player, &Transform), Without<View>>, mut since: Local<Option<f32>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(v) = std::env::var("METALLIC_RC_PAINTBALL") else {
        *done = true;
        return;
    };
    let Some(rt) = link.get() else { return };
    if !rt.room.lock().unwrap().is_spawned() {
        return;
    }
    let now = time.elapsed_secs();
    if now < *since.get_or_insert(now) + 4.0 {
        return;
    }
    *done = true;
    if !rt.room.lock().unwrap().is_master() {
        return;
    }
    let (p, t) = *player;
    for part in v.split(',') {
        if let Some(n) = part.strip_prefix("bots=").and_then(|n| n.parse().ok()) {
            send(&link, Up::Bots { count: n });
        }
    }
    let mode = if v.starts_with("ctf") { Mode::CaptureTheFlag } else { Mode::TeamBattle };
    start(&link, &mut game, mode, p.feet(t), p.yaw);
}

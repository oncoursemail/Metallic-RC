//! Dodgeball game mode in the client. When the `dodgeball` arena (the decrypted `level11` scene,
//! Activities/Dodgeball/Scenes/dodgeball.unity) is the loaded world, this sets up a playable round: it spawns the
//! real `(DodgeballBall)` props on the centre line — grabbable and throwable through the client's own `interact`
//! grab/throw (E to grab, E again to throw, Q to drop) — and runs the ported [`gamemodes::DodgeballManager`] so the
//! round/score logic ticks alongside play.
//!
//! The ball prop is the game's own asset (bundle 43ea1176…, a 0.22 m sphere with its Rigidbody, SphereCollider,
//! `Dodgeball` script, `NetworkTransform` and trail); the client's prop system makes a spawned prop a `Grabbable`
//! `NetObject`, so grab/throw and rr-net sync come for free. The thrown-ball → player hit that drives the manager's
//! elimination/scoring is only meaningful with opponents, so it lands with the networked player roster (marked below);
//! the single-client slice is "real balls you grab and throw in the real arena".
//!
//! Keep it muted when running the client (owner's standing request): set the volume_* settings to 0.

use bevy::prelude::*;
use gamemodes::dodgeball::{DodgeballConfig, DodgeballManager};

use crate::world::{self, Loader, RoomPiece};

/// The real Dodgeball ball prop's Addressables key (the parentheses are part of the key).
const BALL_ADDRESS: &str = "(DodgeballBall)";
/// How many balls sit on the centre line at the start of a round (the real game's count).
const BALL_COUNT: usize = 6;

/// Set when the client is launched into the Dodgeball arena (`--world dodgeball`); off for every other world.
#[derive(Resource, Clone, Copy)]
pub struct DodgeballMode(pub bool);

/// The running match state (the ported rules engine), kept so play and scoring tick together.
#[derive(Resource)]
pub struct DodgeballGame {
    pub mgr: DodgeballManager,
}

pub struct DodgeballPlugin;

impl Plugin for DodgeballPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (setup_court, tick_match).chain());
    }
}

/// Once, after the arena's pieces are in, spawn the real balls on the centre line and start the match. A no-op unless
/// the loaded world is the Dodgeball arena.
fn setup_court(
    mut commands: Commands,
    mode: Option<Res<DodgeballMode>>,
    loader: Option<Res<Loader>>,
    pieces: Query<(), With<RoomPiece>>,
    game: Option<Res<DodgeballGame>>,
    mut done: Local<bool>,
) {
    if *done || game.is_some() {
        return;
    }
    // only in the Dodgeball arena, only once the room has been built, and only with a loader to fetch the prop
    let (Some(DodgeballMode(true)), Some(loader), false) = (mode.as_deref(), loader, pieces.is_empty()) else {
        return;
    };
    *done = true;

    // the six balls, in a row across the centre line (z = 0), just above the floor
    for i in 0..BALL_COUNT {
        let x = (i as f32 - (BALL_COUNT as f32 - 1.0) * 0.5) * 1.1;
        let at = Transform::from_xyz(x, 0.6, 0.0);
        world::spawn_prop(&mut commands, &loader, BALL_ADDRESS, at);
    }

    commands.insert_resource(DodgeballGame { mgr: DodgeballManager::new(DodgeballConfig::default()) });
    info!("dodgeball: court set up — {BALL_COUNT} balls on the centre line; grab with E, throw with E again");
    // 1:1: with the networked player roster, register each player on a team here (local + remote avatars), and add
    // the ball-hit → manager.on_player_hit / on_catch bridge from the thrown ball's collision so rounds actually
    // score. Single-client has no opponents to hit, so that stays with the net layer.
}

/// Advances the match clock so the round/score lifecycle runs. (Elimination fires from the net hit bridge above.)
fn tick_match(time: Res<Time>, game: Option<ResMut<DodgeballGame>>) {
    let Some(mut game) = game else { return };
    let mut noop = NoEffects;
    game.mgr.tick(time.delta_secs().min(0.1), &mut noop);
}

/// No-op sink for the manager's effects until the networked VFX/SFX/scoreboard bridge is wired.
struct NoEffects;
impl gamemodes::dodgeball::DodgeballEffects for NoEffects {}

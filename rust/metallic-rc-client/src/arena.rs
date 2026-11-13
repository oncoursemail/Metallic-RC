//! Arena (Team Warzone / Lasertag) in-client over rr-net, master-tracked — the third COMBAT vertical (game #10) on the
//! projectile-combat foundation ([`crate::combat`]), two-team kill-point scoring. `--world arena` (level75 Arena_Hangar3
//! / level76 Arena_Cyberjunk_City) runs the real [`gamemodes::arena::ArenaMode`]: every kill awards the KILLER's team
//! from the real table (enemy player +10, teammate −10, normal enemy +2, elite +10, enemy friendly-fire −2, loot +1),
//! team score clamped at 0; first team to `ScoreToWin` = 400, or the higher team at `GameDuration` = 480 s, wins.
//!
//! A kill is a real rapier projectile → [`Health`] damage → [`CombatantDown`], mapped to [`GameInput::Kill`]. This
//! vertical does the PvP path — player kills: cross-team = [`KillKind::EnemyPlayer`] (+10), same-team friendly-fire =
//! [`KillKind::Teammate`] (−10), read straight off CombatantDown's killer/victim teams. The PvE horde (normal/elite
//! enemies) + loot chests arrive with the enemy-AI vertical (the waves/AI are gameplay-depth); when NPC combatants
//! exist they carry their kind. Real weapons = the Arena PvP set (sweet-galileo_paintball-weapon-recon): Arena Pistol
//! 80 m/s, radius 0.02, FLAT, airDrag 0, 4 s, 6 shots/s, body damage 18 at 100 HP (headshot 30 pending avatar
//! hitboxes). Keep the client muted (owner's standing request): run with `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::arena::{ArenaConfig, ArenaMode as ArenaRules};
use gamemodes::mode::{GameEvent, GameInput, GameMode, KillKind, RoundPhase};
use rr_net::objects::RpcTarget;

use crate::combat::{spawn_projectile, Combatant, CombatantDown, Health, ProjectileSpec};
use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const GAME_VIEW_INDEX: u32 = 47120; // distinct authority view (bowling 47117, paintball 47118, BR 47119)
const RPC_KILL: &str = "ArKill"; // guest → master: a kill (killer team, kind)
const RPC_SCORE: &str = "ArScore"; // master → others: a team's running score (team, score)
const RPC_WIN: &str = "ArWin"; // master → others: match over (winner team, or draw)

// REAL Arena Pistol (sweet-galileo_paintball-weapon-recon): the Arena PvP weapon set. Speed 80, radius 0.02, FLAT
// (applyGravity 0), airDrag 0, 4 s life, 6 shots/s; body damage 18 at 100 HP (headshot 30 pending per-body-part
// avatar hitboxes). Kill-point VALUES are the real ArenaMode table (gamemodes::arena).
const AR_SPEED: f32 = 80.0; // Arena Pistol bulletFireSpeed (m/s)
const AR_FIRE_COOLDOWN: f32 = 1.0 / 6.0; // Arena Pistol fire rate 6 shots/s
const AR_HEALTH: f32 = 100.0; // CONFIRMED combat default HP
const AR_SPEC: ProjectileSpec = ProjectileSpec { radius: 0.02, mass: 0.02, gravity_scale: 0.0, air_drag: 0.0, life: 4.0, damage: 18.0 }; // Arena Pistol body damage
// self-test score cap (the real `ScoreToWin` is 400 — used in the real game via ArenaConfig::default(); a small cap
// keeps the headless self-test quick: 3 enemy-player kills × +10 = 30).
const AR_SELFTEST_CAP: u32 = 30;

#[derive(Resource, Clone, Copy)]
pub struct ArenaActive(pub bool);

#[derive(Resource, Default)]
pub struct Arena {
    rules: Option<ArenaRules>,
    game: Option<Entity>,
    /// queued kills awaiting the scorer (killer team + kind), from combat CombatantDown or a guest RPC
    pending: Vec<GameInput>,
    self_test: bool,
    fire_cd: f32,
    shots: u32,
    over: bool,
}

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Arena>().add_systems(Update, (join_game, recv_rpcs, recv_downs, master_tick, ar_combat).chain());
    }
}

/// Encode an optional winner team for the win RPC: 0/1 = team, 255 = draw.
fn win_byte(w: Option<u8>) -> u8 {
    w.unwrap_or(255)
}

/// Once the arena is loaded and we're in a room, register the shared game-authority object; the master runs the
/// kill-point scorer and, for the combat self-test, spawns the two-team combatants the projectiles kill.
fn join_game(
    mut commands: Commands,
    active: Option<Res<ArenaActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Arena>,
) {
    if !matches!(active.as_deref(), Some(ArenaActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(_actor) = actor else { return };

    let e = commands.spawn((Name::new("arena-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/ArenaGame", Transform::default());
    db.game = Some(e);

    if master {
        let self_test = std::env::var("METALLIC_AR_PHYSICS").is_ok();
        // the real two-team kill-point scorer (ScoreToWin 400, 480 s) — or a small cap for the headless self-test.
        let cfg = if self_test { ArenaConfig { score_to_win: AR_SELFTEST_CAP, game_duration: 480.0 } } else { ArenaConfig::default() };
        db.rules = Some(ArenaRules::new(cfg));
        info!("arena: authority — two-team kill-points to {} (480 s); real table enemy +10 / teammate −10 / normal +2 / elite +10 / loot +1", cfg.score_to_win);

        if self_test {
            // COMBAT self-test: team-0 (red) shooter vs 3 team-1 (blue) targets, all with Health, in clear space above
            // the room (the foundation mirrors the arena's static colliders; keep the lane clear so only our combatants
            // + projectiles exist). Red focus-fires blue; each cross-team kill = EnemyPlayer +10 → red reaches the cap.
            let y = 50.9_f32;
            let targets = 3u32;
            commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(0.0, y, 0.0), Combatant { id: 10, team: 0 }, Health::full(AR_HEALTH)));
            for i in 0..targets {
                let x = (i as f32 - (targets as f32 - 1.0) / 2.0) * 2.0;
                commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, y, 10.0), Combatant { id: 20 + i, team: 1 }, Health::full(AR_HEALTH)));
            }
            db.self_test = true;
            info!("arena: COMBAT self-test — red shooter killing {targets} blue players (EnemyPlayer +10) to the {AR_SELFTEST_CAP}-point cap");
        }
    } else {
        info!("arena: guest; the master tracks the team kill-points");
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Arena>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_KILL => {
                if rpc.bytes.len() >= 2 {
                    let team = rpc.bytes[0];
                    let kind = kind_from_byte(rpc.bytes[1]);
                    db.pending.push(GameInput::Kill { team, kind });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 5 {
                    let team = rpc.bytes[0];
                    let score = u32::from_le_bytes(rpc.bytes[1..5].try_into().unwrap_or_default());
                    info!("arena: team {team} score {score}");
                }
            }
            RPC_WIN => {
                if !rpc.bytes.is_empty() {
                    match rpc.bytes[0] {
                        255 => info!("arena: MATCH OVER — DRAW"),
                        t => info!("arena: MATCH OVER — team {t} WINS"),
                    }
                }
            }
            _ => {}
        }
    }
}

/// A combat kill → the Arena kill-point input. The kind is read off the teams: a cross-team kill is an enemy player
/// (+10), a same-team kill is friendly fire (−10). (PvE normal/elite + loot come with the horde/AI vertical.)
fn recv_downs(mut downs: MessageReader<CombatantDown>, mut db: ResMut<Arena>) {
    if db.rules.is_none() {
        return;
    }
    for d in downs.read() {
        let kind = if d.killer_team == d.victim_team { KillKind::Teammate } else { KillKind::EnemyPlayer };
        db.pending.push(GameInput::Kill { team: d.killer_team, kind });
    }
}

/// Master only: tick the match clock, drain queued kills through the real scorer, broadcast team scores + the win.
fn master_tick(time: Res<Time>, mut net: NetObjects, mut db: ResMut<Arena>) {
    if db.rules.is_none() || db.over {
        return;
    }
    let dt = time.delta_secs().min(0.1);

    let inputs: Vec<GameInput> = std::mem::take(&mut db.pending);
    let mut events: Vec<GameEvent> = Vec::new();
    {
        let rules = db.rules.as_mut().unwrap();
        for input in inputs {
            rules.input(input, &mut events);
        }
        rules.tick(dt, &mut events); // the 480 s match clock (higher team wins at expiry)
    }

    let game = db.game;
    for ev in events {
        match ev {
            GameEvent::Score { team, score } => {
                if let Some(g) = game {
                    let mut v = vec![team];
                    v.extend_from_slice(&score.to_le_bytes());
                    net.send_rpc(g, RPC_SCORE, v, RpcTarget::Others);
                }
                info!("arena: team {team} score {score}");
            }
            GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } => {
                if let Some(g) = game {
                    net.send_rpc(g, RPC_WIN, vec![win_byte(winner)], RpcTarget::Others);
                }
                db.over = true;
                match winner {
                    Some(t) => info!("arena: MATCH OVER — team {t} WINS"),
                    None => info!("arena: MATCH OVER — DRAW"),
                }
            }
            _ => {}
        }
    }
}

/// Master + self-test: the red shooter focus-fires the nearest living blue target; each kill scores EnemyPlayer +10.
fn ar_combat(mut commands: Commands, time: Res<Time>, mut db: ResMut<Arena>, combatants: Query<(&Combatant, &Transform, &Health)>) {
    if !db.self_test || db.rules.is_none() || db.over {
        return;
    }
    db.fire_cd -= time.delta_secs();
    if db.fire_cd > 0.0 || db.shots > 400 {
        return;
    }
    db.fire_cd = AR_FIRE_COOLDOWN;

    let Some(from) = combatants.iter().find(|(c, _, _)| c.team == 0).map(|(_, t, _)| t.translation) else { return };
    let mut living: Vec<(u32, Vec3)> = combatants.iter().filter(|(c, _, h)| c.team != 0 && h.current > 0.0).map(|(c, t, _)| (c.id, t.translation)).collect();
    living.sort_by_key(|&(id, _)| id);
    let Some(&(_, to)) = living.first() else { return };
    let dir = (to - from).normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    spawn_projectile(&mut commands, from + dir * 1.0, dir * AR_SPEED, 10, 0, AR_SPEC);
    db.shots += 1;
}

/// Decode a kill kind from the RPC byte, in `KillKind` declaration order (the guest→master kill RPC path; the master
/// itself computes the kind locally from a CombatantDown's teams).
fn kind_from_byte(b: u8) -> KillKind {
    match b {
        0 => KillKind::EnemyPlayer,
        1 => KillKind::Teammate,
        2 => KillKind::NormalEnemy,
        3 => KillKind::EliteEnemy,
        4 => KillKind::EnemyFriendlyFire,
        _ => KillKind::LootChest,
    }
}

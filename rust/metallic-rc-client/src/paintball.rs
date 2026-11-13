//! Paintball in-client over rr-net, master-tracked — the first COMBAT vertical (game #8) on the reusable rapier
//! projectile-combat foundation ([`crate::combat`]). `--world paintball` (or a level14–18 arena) runs the real
//! [`gamemodes::paintball::PaintballMode`] scoreboard; a paintball TAG is a real fired PROJECTILE that flies (CCD) and
//! HITS a [`Combatant`] — [`ProjectileHit`] — not an injected number. The master validates the tag exactly like the
//! rules (round running, both in, different teams, not self) and scores it (+1 the shooter's team, to the TeamBattle
//! cap of 50), then broadcasts the running team scores and the win.
//!
//! WHAT'S REAL vs PENDING (to the 1:1 bar): the SCORING is the real body-verified `PaintballMode` (sweet-galileo
//! paintball-recon — defaults 50/600s/respawn). The HIT MECHANISM is the real rapier projectile→collision→tag
//! pipeline (de-risked, examples/projectile_hit.rs). The projectile DYNAMICS (speed/mass/radius/lifetime/gravity/fire
//! rate) are the only pending piece — they live in `WeaponSetPaintballModBase` + the paintball projectile prefab,
//! requested from the decompile lane (sweet-galileo_paintball-weapon-recon.md). They're isolated as the `PB_*`
//! constants below and flagged `// 1:1:`; the self-test uses placeholders to prove the pipeline and they reconcile in
//! one place when the dump values land. Keep the client muted (owner's standing request): run with `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase};
use gamemodes::paintball::{PaintballConfig, PaintballMode as PaintballRules};
use rr_net::objects::RpcTarget;

use crate::combat::{spawn_projectile, Combatant, ProjectileHit};
use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const GAME_VIEW_INDEX: u32 = 47118; // distinct authority view (bowling = 47117)
const RPC_TAG: &str = "PbTag"; // guest → master: a projectile tag (thrower, victim)
const RPC_SCORE: &str = "PbScore"; // master → others: a team's running score (team, score)
const RPC_WIN: &str = "PbWin"; // master → others: match over (winner team, or draw)

// 1:1: PENDING `WeaponSetPaintballModBase` + the paintball projectile prefab (requested from the decompile lane,
// sweet-galileo_paintball-weapon-recon.md). These are PLACEHOLDER projectile dynamics so the self-test can prove the
// fire→fly→hit→tag→score pipeline; they are NOT claimed 1:1. Reconcile every one to the real dump value when it lands.
const PB_SPEED: f32 = 40.0; // launch speed along the aim ray (m/s)
const PB_RADIUS: f32 = 0.03; // projectile collider radius (m)
const PB_MASS: f32 = 0.01; // projectile mass (kg)
const PB_GRAVITY: f32 = 0.0; // gravity scale (0 = flat; the real paintball may arc slightly)
const PB_LIFE: f32 = 2.0; // seconds before despawn
const PB_FIRE_COOLDOWN: f32 = 0.3; // s between shots per shooter (self-test cadence)

#[derive(Resource, Clone, Copy)]
pub struct PaintballActive(pub bool);

#[derive(Resource, Default)]
pub struct Paintball {
    rules: Option<PaintballRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    pending: Vec<GameInput>,
    score: [u32; 2],
    /// the combat self-test is running (master + `METALLIC_PB_PHYSICS`)
    self_test: bool,
    /// fire-cooldown accumulator for the self-test
    fire_cd: f32,
    /// a safety cap on self-test projectiles so a headless run always terminates
    shots: u32,
}

pub struct PaintballPlugin;

impl Plugin for PaintballPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Paintball>().add_systems(Update, (join_game, recv_rpcs, recv_hits, master_tick, pb_combat).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Encode an optional winner team for the win RPC: 0/1 = team, 255 = draw.
fn win_byte(w: Option<u8>) -> u8 {
    w.unwrap_or(255)
}

/// Once the arena is loaded and we're in a room, register the shared game-authority object; the master runs the
/// scoreboard and, for the combat self-test, spawns the synthetic combatants that the projectiles tag.
fn join_game(
    mut commands: Commands,
    active: Option<Res<PaintballActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<Paintball>,
) {
    if !matches!(active.as_deref(), Some(PaintballActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };

    let e = commands.spawn((Name::new("paintball-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/PaintballGame", Transform::default());
    db.game = Some(e);

    if master {
        // the real TeamBattle scoreboard (defaults: score-to-win 50, 600 s, respawn-based) — the body-verified rules.
        let mut rules = PaintballRules::new(PaintballConfig::team_battle());
        rules.add_player(pid(actor)); // master = player, team 0
        db.roster.push(actor);
        db.rules = Some(rules);
        info!("paintball: authority (actor {actor}) — TeamBattle to 50, master-validated projectile tags");

        if std::env::var("METALLIC_PB_PHYSICS").is_ok() {
            // COMBAT self-test: spawn synthetic combatants — team 0 shooters vs team 1 targets — in clear space above
            // the room (the foundation mirrors the arena's static colliders as fixed bodies; keep the test lane clear
            // so only our combatants + projectiles exist, exactly as the bowling physics lane does). Team 0 fires real
            // projectiles at team 1; each hit flies through the full combat pipeline into the real scoreboard.
            let y = 50.9_f32; // combatant centre height in clear space
            let shooters = 3;
            for i in 0..shooters {
                let x = (i as f32 - (shooters as f32 - 1.0) / 2.0) * 2.0;
                // team 0 shooter (id 100+i) at the near line
                commands.spawn((
                    RigidBody::Fixed,
                    Collider::cuboid(0.3, 0.9, 0.3),
                    ActiveEvents::COLLISION_EVENTS,
                    Transform::from_xyz(x, y, 0.0),
                    Combatant { id: 100 + i, team: 0 },
                ));
                // team 1 target (id 200+i) 10 m downrange, on the real roster so tags validate
                commands.spawn((
                    RigidBody::Fixed,
                    Collider::cuboid(0.3, 0.9, 0.3),
                    ActiveEvents::COLLISION_EVENTS,
                    Transform::from_xyz(x, y, 10.0),
                    Combatant { id: 200 + i, team: 1 },
                ));
            }
            // seed the rules with the exact self-test roster so the synthetic combatants are on the teams we spawned
            // (PaintballMode assigns teams by join order, so seed_team_rules interleaves the joins to land them).
            db.rules = Some(seed_team_rules(actor, shooters));
            db.self_test = true;
            info!("paintball: COMBAT self-test — {shooters} team-0 shooters firing real projectiles at {shooters} team-1 targets (10 m), tags drive the real scoreboard to 50");
        }
    } else {
        info!("paintball: guest (actor {actor}); the master validates tags + tracks the scoreboard");
    }
}

/// Build the rules with the deterministic self-test roster: the master + shooters 100.. on team 0, targets 200.. on
/// team 1. PaintballMode assigns teams by alternating join order (even slot → team 0, odd slot → team 1), so with the
/// master on slot 0 we add each (target, shooter) pair to consume an odd slot then an even slot: targets land on
/// team 1, shooters on team 0.
fn seed_team_rules(actor: i32, shooters: u32) -> PaintballRules {
    let mut r = PaintballRules::new(PaintballConfig::team_battle());
    r.add_player(pid(actor)); // slot 0 → team 0
    for i in 0..shooters {
        r.add_player(200 + i); // odd slot → team 1 (target)
        r.add_player(100 + i); // even slot → team 0 (shooter)
    }
    r
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<Paintball>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_TAG => {
                if rpc.bytes.len() >= 8 {
                    let thrower = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let victim = u32::from_le_bytes(rpc.bytes[4..8].try_into().unwrap_or_default());
                    db.pending.push(GameInput::Hit { thrower, victim });
                }
            }
            RPC_SCORE => {
                if rpc.bytes.len() >= 5 {
                    let team = rpc.bytes[0];
                    let score = u32::from_le_bytes(rpc.bytes[1..5].try_into().unwrap_or_default());
                    info!("paintball: team {team} score {score}");
                }
            }
            RPC_WIN => {
                if !rpc.bytes.is_empty() {
                    match rpc.bytes[0] {
                        255 => info!("paintball: MATCH OVER — DRAW"),
                        t => info!("paintball: MATCH OVER — team {t} WINS"),
                    }
                }
            }
            _ => {}
        }
    }
}

/// A real projectile struck a combatant → feed it to the master's scoreboard as a tag (thrower = shooter). This is the
/// combat pipeline's join to the game rules: [`ProjectileHit`] → [`GameInput::Hit`].
fn recv_hits(mut hits: MessageReader<ProjectileHit>, mut db: ResMut<Paintball>) {
    if db.rules.is_none() {
        return;
    }
    for h in hits.read() {
        db.pending.push(GameInput::Hit { thrower: h.shooter, victim: h.victim });
    }
}

/// Master only: register remote players, drain queued tags through the real scoreboard, broadcast running team scores
/// and the win.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<Paintball>) {
    if db.rules.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);

    // register real remote players who joined (self-test combatants are already seeded)
    if !db.self_test {
        let remote_actors: Vec<i32> = match link.get() {
            Some(rt) => rt.room.lock().unwrap().remotes(rt.now(), dt as f64).into_iter().map(|s| s.actor).filter(|&a| a > 0 && a < 9000).collect(),
            None => Vec::new(),
        };
        for a in &remote_actors {
            if !db.roster.contains(a) {
                db.roster.push(*a);
                if let Some(r) = db.rules.as_mut() {
                    r.add_player(pid(*a));
                }
                info!("paintball: player {a} joined");
            }
        }
    }

    let inputs: Vec<GameInput> = std::mem::take(&mut db.pending);
    let mut events: Vec<GameEvent> = Vec::new();
    {
        let rules = db.rules.as_mut().unwrap();
        for input in inputs {
            rules.input(input, &mut events);
        }
    }

    let game = db.game;
    for ev in events {
        if let Some(g) = game {
            match &ev {
                GameEvent::Score { team, score } => {
                    let mut v = vec![*team];
                    v.extend_from_slice(&score.to_le_bytes());
                    net.send_rpc(g, RPC_SCORE, v, RpcTarget::Others);
                }
                GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } => {
                    net.send_rpc(g, RPC_WIN, vec![win_byte(*winner)], RpcTarget::Others);
                }
                _ => {}
            }
        }
        match ev {
            GameEvent::Score { team, score } => {
                if (team as usize) < 2 {
                    db.score[team as usize] = score;
                }
                info!("paintball: team {team} score {score}");
            }
            GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } => match winner {
                Some(t) => info!("paintball: MATCH OVER — team {t} WINS ({} vs {})", db.score[0], db.score[1]),
                None => info!("paintball: MATCH OVER — DRAW ({} vs {})", db.score[0], db.score[1]),
            },
            _ => {}
        }
    }
}

/// Master + self-test: every cooldown, each team-0 shooter fires a real projectile at its matched team-1 target. The
/// projectile flies (CCD), hits the target's collider, and the combat plugin raises [`ProjectileHit`] → the master
/// scores the tag. This is the full fire→fly→hit→score combat pipeline on the real scoreboard.
fn pb_combat(mut commands: Commands, time: Res<Time>, mut db: ResMut<Paintball>, combatants: Query<(&Combatant, &Transform)>) {
    if !db.self_test || db.rules.is_none() {
        return;
    }
    db.fire_cd -= time.delta_secs();
    if db.fire_cd > 0.0 || db.shots > 400 {
        return;
    }
    db.fire_cd = PB_FIRE_COOLDOWN;

    // pair i-th team-0 shooter with i-th team-1 target and fire
    let mut shooters: Vec<(u32, Vec3)> = combatants.iter().filter(|(c, _)| c.team == 0 && c.id >= 100).map(|(c, t)| (c.id, t.translation)).collect();
    let mut targets: Vec<Vec3> = combatants.iter().filter(|(c, _)| c.team == 1).map(|(_, t)| t.translation).collect();
    shooters.sort_by_key(|&(id, _)| id);
    targets.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    for (i, (id, from)) in shooters.iter().enumerate() {
        let Some(&to) = targets.get(i) else { continue };
        let dir = (to - *from).normalize_or_zero();
        if dir == Vec3::ZERO {
            continue;
        }
        // spawn at the muzzle (1 m ahead) so the projectile doesn't collide with its own shooter on frame one
        let origin = *from + dir * 1.0;
        spawn_projectile(&mut commands, origin, dir * PB_SPEED, *id, 0, PB_RADIUS, PB_MASS, PB_GRAVITY, PB_LIFE);
        db.shots += 1;
    }
}

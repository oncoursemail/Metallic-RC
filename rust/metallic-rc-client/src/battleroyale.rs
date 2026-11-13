//! BattleRoyale in-client over rr-net, master-tracked — the second COMBAT vertical (game #11) on the projectile-combat
//! foundation ([`crate::combat`]), this one KILL-based. `--world battleroyale` (level78, RecRoyale_Frontier) runs the
//! real [`gamemodes::battleroyale::BattleRoyaleMode`] (purely last-standing, elimination-rank — no score, no timer).
//! Combatants carry [`Health`]; a projectile's damage accumulates and the lethal hit raises [`CombatantDown`], which
//! the master maps to [`GameInput::Eliminated`] → the mode assigns the elimination rank (first out = highest number,
//! last standing = rank 1 = winner) and ends the match when one player (Solos) or one team (Squads) remains.
//!
//! WHAT'S REAL vs PENDING (to the 1:1 bar): the SCORING is the body-verified `BattleRoyaleMode`
//! (sweet-galileo_battleroyale-recon — last-standing, reverse-placement ranks, permanent elimination). The KILL
//! pipeline is the real rapier projectile → Health damage → CombatantDown path. The projectile DYNAMICS are now REAL:
//! BR weapons are loot-drop (no BR-specific prefabs) and resolve to the Arena PvP weapon set — the Arena Pistol (80
//! m/s, radius 0.02, FLAT / no arc, 4 s, 6 shots/s), from sweet-galileo_paintball-weapon-recon. PENDING: the HP +
//! per-hit DAMAGE — BattleRoyaleManager has no HP/damage field; HP is a BR GameConfiguration value (RR standard combat
//! is 100 — plausible, UNCONFIRMED) and damage is master-computed per hit as an int from (weapon, damage type, body
//! part, charge), whose table needs a Ghidra pass (requested). So BR_HEALTH + BR_SPEC.damage are a working model,
//! flagged `// 1:1:`. The killzone (the OTHER death source) is a separate vertical. Keep the client muted: `METALLIC_NO_MIC=1`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::battleroyale::{BattleRoyaleConfig, BattleRoyaleMode as BrRules};
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase};
use rr_net::objects::RpcTarget;

use crate::combat::{spawn_projectile, Combatant, CombatantDown, Health, ProjectileSpec};
use crate::netobj::{NetObjects, NetRpc, RoomLink};
use crate::world::RoomPiece;

const GAME_VIEW_INDEX: u32 = 47119; // distinct authority view (bowling 47117, paintball 47118)
const RPC_ELIM: &str = "BrElim"; // guest → master: a combat elimination (victim id)
const RPC_RANK: &str = "BrRank"; // master → others: a player's elimination rank (player, rank)
const RPC_WIN: &str = "BrWin"; // master → others: match over (winner player id)

// Projectile DYNAMICS = the REAL Arena Pistol (sweet-galileo_paintball-weapon-recon): BR weapons are loot-drop and
// resolve to the Arena PvP weapon set, which fly FLAT (applyGravity 0 — unlike paintball), speed 80, radius 0.02,
// airDrag 0, 4 s life, 6 shots/s. HEALTH + DAMAGE are the pending piece: BattleRoyaleManager has no HP/damage field —
// HP is a BR GameConfiguration value (RR standard combat is 100; plausible but UNCONFIRMED) and damage is
// master-computed per hit as an INT from (weapon, damage type, body part, charge) — the exact table needs a Ghidra
// pass (requested). So BR_HEALTH/BR_SPEC.damage are a WORKING model (34 dmg vs 100 HP = 3 hits to down), flagged 1:1.
const BR_SPEED: f32 = 80.0; // Arena Pistol bulletFireSpeed (m/s)
const BR_FIRE_COOLDOWN: f32 = 1.0 / 6.0; // Arena Pistol fire rate 6 shots/s
const BR_HEALTH: f32 = 100.0; // 1:1: RR standard combat HP — UNCONFIRMED (BR GameConfiguration, requested)
const BR_SPEC: ProjectileSpec = ProjectileSpec { radius: 0.02, mass: 0.02, gravity_scale: 0.0, air_drag: 0.0, life: 4.0, damage: 34.0 };

#[derive(Resource, Clone, Copy)]
pub struct BattleRoyaleActive(pub bool);

#[derive(Resource, Default)]
pub struct BattleRoyale {
    rules: Option<BrRules>,
    game: Option<Entity>,
    roster: Vec<i32>,
    /// victim ids awaiting elimination through the rules (from combat CombatantDown or a guest RPC)
    pending: Vec<u32>,
    self_test: bool,
    fire_cd: f32,
    shots: u32,
    over: bool,
}

pub struct BattleRoyalePlugin;

impl Plugin for BattleRoyalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BattleRoyale>().add_systems(Update, (join_game, recv_rpcs, recv_downs, master_tick, br_combat).chain());
    }
}

fn pid(actor: i32) -> u32 {
    actor as u32
}

/// Once the arena is loaded and we're in a room, register the shared game-authority object; the master runs the
/// elimination scorer and, for the combat self-test, spawns the synthetic combatants (with Health) the projectiles kill.
fn join_game(
    mut commands: Commands,
    active: Option<Res<BattleRoyaleActive>>,
    link: Res<RoomLink>,
    pieces: Query<(), With<RoomPiece>>,
    mut net: NetObjects,
    mut db: ResMut<BattleRoyale>,
) {
    if !matches!(active.as_deref(), Some(BattleRoyaleActive(true))) || pieces.is_empty() || db.game.is_some() {
        return;
    }
    let Some(rt) = link.get() else { return };
    let (actor, master) = {
        let room = rt.room.lock().unwrap();
        (room.actor(), room.is_master())
    };
    let Some(actor) = actor else { return };

    let e = commands.spawn((Name::new("battleroyale-authority"), Transform::default(), Visibility::Hidden)).id();
    net.register_scene(e, GAME_VIEW_INDEX, "debug/BattleRoyaleGame", Transform::default());
    db.game = Some(e);

    if master {
        // the real Solos scorer (last player standing, reverse-placement ranks) — the body-verified rules.
        let mut rules = BrRules::new(BattleRoyaleConfig::solos());
        rules.add_player(pid(actor)); // master = a player
        db.roster.push(actor);
        db.rules = Some(rules);
        info!("battleroyale: authority (actor {actor}) — Solos last-standing, master-validated eliminations");

        if std::env::var("METALLIC_BR_PHYSICS").is_ok() {
            // COMBAT self-test: one team-0 shooter (never shot) + 3 team-1 targets, all with Health, in clear space
            // above the room (the foundation mirrors the arena's static colliders; keep the test lane clear so only our
            // combatants + projectiles exist, as the bowling/paintball lanes do). The shooter focus-fires living
            // enemies; damage accumulates to a kill; each kill flies through the full pipeline into the real scorer.
            let y = 50.9_f32;
            let targets = 3u32;
            // shooter (id 100) on the rules roster as a player; it survives and must end as rank 1
            commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(0.0, y, 0.0), Combatant { id: 100, team: 0 }, Health::full(BR_HEALTH)));
            for i in 0..targets {
                let x = (i as f32 - (targets as f32 - 1.0) / 2.0) * 2.0;
                commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, y, 10.0), Combatant { id: 200 + i, team: 1 }, Health::full(BR_HEALTH)));
            }
            // seed the rules roster: the shooter + the 3 targets are the players (Solos → each its own team)
            if let Some(r) = db.rules.as_mut() {
                r.add_player(100);
                for i in 0..targets {
                    r.add_player(200 + i);
                }
            }
            db.self_test = true;
            info!("battleroyale: COMBAT self-test — 1 shooter focus-firing {targets} targets ({BR_HEALTH} HP, {} dmg/hit); eliminations rank in order, last standing (the shooter) = rank 1", BR_SPEC.damage);
        }
    } else {
        info!("battleroyale: guest (actor {actor}); the master validates eliminations + tracks ranks");
    }
}

fn recv_rpcs(mut rpcs: MessageReader<NetRpc>, mut db: ResMut<BattleRoyale>) {
    for rpc in rpcs.read() {
        match rpc.method.as_str() {
            RPC_ELIM => {
                if rpc.bytes.len() >= 4 {
                    let victim = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    db.pending.push(victim);
                }
            }
            RPC_RANK => {
                if rpc.bytes.len() >= 8 {
                    let player = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    let rank = u32::from_le_bytes(rpc.bytes[4..8].try_into().unwrap_or_default());
                    info!("battleroyale: player {player} eliminated — rank {rank}");
                }
            }
            RPC_WIN => {
                if rpc.bytes.len() >= 4 {
                    let winner = u32::from_le_bytes(rpc.bytes[0..4].try_into().unwrap_or_default());
                    info!("battleroyale: MATCH OVER — player {winner} WINS (rank 1)");
                }
            }
            _ => {}
        }
    }
}

/// A combatant's Health hit zero → feed the kill to the master's scorer as an elimination (combat → [`GameInput::Eliminated`]).
fn recv_downs(mut downs: MessageReader<CombatantDown>, mut db: ResMut<BattleRoyale>) {
    if db.rules.is_none() {
        return;
    }
    for d in downs.read() {
        db.pending.push(d.victim);
    }
}

/// Master only: register remote players, drain queued eliminations through the real scorer, broadcast ranks + the win.
fn master_tick(time: Res<Time>, link: Res<RoomLink>, mut net: NetObjects, mut db: ResMut<BattleRoyale>) {
    if db.rules.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);

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
                info!("battleroyale: player {a} joined");
            }
        }
    }

    let victims: Vec<u32> = std::mem::take(&mut db.pending);
    if victims.is_empty() {
        return;
    }
    let mut events: Vec<GameEvent> = Vec::new();
    {
        let rules = db.rules.as_mut().unwrap();
        for v in &victims {
            rules.input(GameInput::Eliminated { player: *v }, &mut events);
        }
    }
    // the rank each just-eliminated player was assigned (reverse placement)
    let ranks: Vec<(u32, u32)> = {
        let rules = db.rules.as_ref().unwrap();
        victims.iter().filter_map(|v| rules.rank_of(*v).map(|r| (*v, r))).collect()
    };

    let game = db.game;
    for (player, rank) in ranks {
        if let Some(g) = game {
            let mut v = player.to_le_bytes().to_vec();
            v.extend_from_slice(&rank.to_le_bytes());
            net.send_rpc(g, RPC_RANK, v, RpcTarget::Others);
        }
        info!("battleroyale: player {player} eliminated — rank {rank}");
    }
    for ev in events {
        match ev {
            GameEvent::MatchOver { winner } => {
                if let Some(g) = game {
                    net.send_rpc(g, RPC_WIN, winner.to_le_bytes().to_vec(), RpcTarget::Others);
                }
                db.over = true;
                info!("battleroyale: MATCH OVER — player {winner} WINS (rank 1)");
            }
            GameEvent::Round { phase: RoundPhase::GameOver, winner: Some(team), .. } => {
                // Squads: the last team standing (encode the team as the winner id)
                if let Some(g) = game {
                    net.send_rpc(g, RPC_WIN, (team as u32).to_le_bytes().to_vec(), RpcTarget::Others);
                }
                db.over = true;
                info!("battleroyale: MATCH OVER — team {team} WINS (last squad standing)");
            }
            _ => {}
        }
    }
}

/// Master + self-test: every cooldown the shooter fires a real projectile at the nearest LIVING enemy (focus fire).
/// Damage accumulates on that target's Health until it dies (CombatantDown → elimination), then the shooter moves to
/// the next — so eliminations happen in a deterministic order and the sole survivor (the shooter) ends as rank 1.
fn br_combat(mut commands: Commands, time: Res<Time>, mut db: ResMut<BattleRoyale>, combatants: Query<(&Combatant, &Transform, &Health)>) {
    if !db.self_test || db.rules.is_none() || db.over {
        return;
    }
    db.fire_cd -= time.delta_secs();
    if db.fire_cd > 0.0 || db.shots > 400 {
        return;
    }
    db.fire_cd = BR_FIRE_COOLDOWN;

    // the shooter (team 0) and the nearest living enemy (team != 0, Health > 0), lowest id first for determinism
    let Some((_sid, from)) = combatants.iter().find(|(c, _, _)| c.team == 0).map(|(c, t, _)| (c.id, t.translation)) else { return };
    let mut living: Vec<(u32, Vec3)> = combatants.iter().filter(|(c, _, h)| c.team != 0 && h.current > 0.0).map(|(c, t, _)| (c.id, t.translation)).collect();
    living.sort_by_key(|&(id, _)| id);
    let Some(&(_tid, to)) = living.first() else { return };
    let dir = (to - from).normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    spawn_projectile(&mut commands, from + dir * 1.0, dir * BR_SPEED, 100, 0, BR_SPEC);
    db.shots += 1;
}

//! Verify the BattleRoyale COMBAT vertical's pipeline headless + deterministic: a shooter fires REAL rapier
//! projectiles ([`metallic_rc_client::combat`]) at targets carrying [`Health`]; damage accumulates to a kill
//! ([`CombatantDown`]), the real [`gamemodes::battleroyale::BattleRoyaleMode`] assigns elimination ranks (reverse
//! placement), and the last standing (the shooter) wins as rank 1. Proves fire→fly→damage→down→eliminate→rank end to
//! end, the exact wiring the in-client plugin runs over rr-net (minus the net transport). Placeholder projectile/health
//! values (pending the BR weapon dump) — this verifies the MECHANISM, not the 1:1 numbers.
//!
//! `cargo run --profile fast -p metallic-rc-client --example br_combat`
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::battleroyale::{BattleRoyaleConfig, BattleRoyaleMode};
use gamemodes::mode::{GameEvent, GameInput, GameMode};
use metallic_rc_client::combat::{spawn_projectile, CombatPlugin, Combatant, CombatantDown, Health, ProjectileHit, ProjectileSpec};

#[derive(Resource)]
struct Rules(BattleRoyaleMode);

#[derive(Resource, Default)]
struct Fire {
    cd: f32,
    shots: u32,
}

#[derive(Resource, Default)]
struct Outcome {
    winner: Option<u32>,
    elim_order: Vec<(u32, u32)>, // (player, rank) in the order they were eliminated
    over: bool,
    hits: u32,
    downs: u32,
}

const TARGETS: u32 = 3;
const SPEED: f32 = 80.0; // real Arena Pistol bulletFireSpeed (BR uses the Arena PvP set)
const HEALTH: f32 = 100.0; // CONFIRMED combat default HP
// real Arena Pistol: r0.02, FLAT, airDrag 0, 4 s; BODY damage 18 (100 HP → ~6 hits; headshot 30 pending hitboxes)
const SPEC: ProjectileSpec = ProjectileSpec { radius: 0.02, mass: 0.02, gravity_scale: 0.0, air_drag: 0.0, life: 4.0, damage: 18.0 };

fn main() {
    let mut rules = BattleRoyaleMode::new(BattleRoyaleConfig::solos());
    rules.add_player(100); // shooter — must survive as rank 1
    for i in 0..TARGETS {
        rules.add_player(200 + i);
    }

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(CombatPlugin)
        .insert_resource(Rules(rules))
        .init_resource::<Fire>()
        .init_resource::<Outcome>()
        .add_systems(Startup, setup)
        .add_systems(Update, (fire, resolve, count_hits));

    for _ in 0..2400 {
        // up to 40 s of sim
        app.update();
        if app.world().resource::<Outcome>().over {
            break;
        }
    }

    let shots = app.world().resource::<Fire>().shots;
    let o = app.world().resource::<Outcome>();
    let order: Vec<String> = o.elim_order.iter().map(|(p, r)| format!("p{p}=rank{r}")).collect();
    println!("br combat: shots={shots}, hits={}, kills={}", o.hits, o.downs);
    match o.winner {
        Some(w) => println!("br combat: player {w} WINS (rank 1) — eliminations [{}] — fire→fly→damage→down→eliminate→rank pipeline OK", order.join(", ")),
        None => println!("br combat: NO WIN in 40 s — pipeline incomplete (targets never died)"),
    }
}

fn setup(mut commands: Commands) {
    // shooter (survives) + targets with Health, 10 m downrange — player-sized cuboids emitting collision events
    commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(0.0, 0.9, 0.0), Combatant { id: 100, team: 0 }, Health::full(HEALTH)));
    for i in 0..TARGETS {
        let x = (i as f32 - (TARGETS as f32 - 1.0) / 2.0) * 2.0;
        commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, 0.9, 10.0), Combatant { id: 200 + i, team: 1 }, Health::full(HEALTH)));
    }
}

/// Focus-fire: every 0.25 s (SIM time) the shooter fires at the nearest LIVING enemy (lowest id), so targets die in
/// order. Cadence is driven by the FIXED sim step, not `Time::delta` — in a tight headless `app.update()` loop real
/// wall-clock barely advances while physics steps a fixed 1/60 each update, so a real-time cooldown would under-fire.
fn fire(mut commands: Commands, mut f: ResMut<Fire>, combatants: Query<(&Combatant, &Transform, &Health)>) {
    f.cd -= 1.0 / 60.0; // TimestepMode::Fixed { dt: 1/60 }
    if f.cd > 0.0 || f.shots > 400 {
        return;
    }
    f.cd = 0.25;
    let Some(from) = combatants.iter().find(|(c, _, _)| c.team == 0).map(|(_, t, _)| t.translation) else { return };
    let mut living: Vec<(u32, Vec3)> = combatants.iter().filter(|(c, _, h)| c.team != 0 && h.current > 0.0).map(|(c, t, _)| (c.id, t.translation)).collect();
    living.sort_by_key(|&(id, _)| id);
    let Some(&(_, to)) = living.first() else { return };
    let dir = (to - from).normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    spawn_projectile(&mut commands, from + dir * 1.0, dir * SPEED, 100, 0, SPEC);
    f.shots += 1;
}

/// A kill → the real BattleRoyaleMode eliminates the victim and assigns a rank; record the order + the win.
fn resolve(mut downs: MessageReader<CombatantDown>, mut rules: ResMut<Rules>, mut out: ResMut<Outcome>) {
    for d in downs.read() {
        out.downs += 1;
        let mut events = Vec::new();
        rules.0.input(GameInput::Eliminated { player: d.victim }, &mut events);
        if let Some(r) = rules.0.rank_of(d.victim) {
            out.elim_order.push((d.victim, r));
        }
        for ev in events {
            if let GameEvent::MatchOver { winner } = ev {
                out.winner = Some(winner);
                out.over = true;
            }
        }
    }
}

/// Diagnostic: count projectile hits on combatants.
fn count_hits(mut hits: MessageReader<ProjectileHit>, mut out: ResMut<Outcome>) {
    for _ in hits.read() {
        out.hits += 1;
    }
}

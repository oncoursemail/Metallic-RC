//! Verify the Paintball COMBAT vertical's full pipeline headless + deterministic: team-0 shooters fire REAL rapier
//! projectiles ([`metallic_rc_client::combat`]) at team-1 combatants; each projectile flies (CCD), hits a target,
//! raises [`ProjectileHit`], and the REAL [`gamemodes::paintball::PaintballMode`] scores it as a tag — team 0 reaches
//! the TeamBattle cap and wins. This proves fire→fly→hit→tag→score end to end, the exact wiring the in-client plugin
//! runs over rr-net (minus the net transport). The cap is lowered to 8 here only to keep the de-risk quick; the real
//! default is 50 and the pipeline is identical. Projectile dynamics are the plugin's placeholder `PB_*` values
//! (pending `WeaponSetPaintballModBase` from the dump) — this verifies the MECHANISM, not the 1:1 numbers.
//!
//! `cargo run --profile fast -p metallic-rc-client --example paintball_combat`
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode, RoundPhase};
use gamemodes::paintball::{PaintballConfig, PaintballMode, PaintballVariant};
use metallic_rc_client::combat::{spawn_projectile, CombatPlugin, Combatant, ProjectileHit, ProjectileSpec};

#[derive(Resource)]
struct Rules(PaintballMode);

#[derive(Resource, Default)]
struct Fire {
    cd: f32,
    shots: u32,
}

#[derive(Resource, Default)]
struct Outcome {
    winner: Option<Option<u8>>, // Some(Some(team)) = win, Some(None) = draw, None = still running
    team0: u32,
    tags: u32,
}

const SHOOTERS: u32 = 3;
const SPEED: f32 = 33.0; // real PaintballGun bulletFireSpeed

fn main() {
    // the real TeamBattle rules, cap lowered to 8 for a quick headless run (50 is the real default; pipeline identical)
    let mut rules = PaintballMode::new(PaintballConfig { variant: PaintballVariant::TeamBattle, score_to_win: 8, game_duration: 600.0 });
    rules.add_player(1); // master, slot 0 → team 0
    for i in 0..SHOOTERS {
        rules.add_player(200 + i); // odd slot → team 1 (target)
        rules.add_player(100 + i); // even slot → team 0 (shooter)
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
        .add_systems(Update, (fire, score).chain());

    for _ in 0..1800 {
        // up to 30 s of sim
        app.update();
        if app.world().resource::<Outcome>().winner.is_some() {
            break;
        }
    }

    let o = app.world().resource::<Outcome>();
    match o.winner {
        Some(Some(t)) => println!("paintball combat: team {t} WINS at the cap after {} tags (team0 {}) — fire→fly→hit→tag→score pipeline OK", o.tags, o.team0),
        Some(None) => println!("paintball combat: DRAW — unexpected"),
        None => println!("paintball combat: NO WIN in 30 s — pipeline incomplete (projectiles never scored)"),
    }
}

fn setup(mut commands: Commands) {
    // team-0 shooters at the near line, team-1 targets 10 m downrange — player-sized cuboids emitting collision events
    for i in 0..SHOOTERS {
        let x = (i as f32 - (SHOOTERS as f32 - 1.0) / 2.0) * 2.0;
        commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, 0.9, 0.0), Combatant { id: 100 + i, team: 0 }));
        commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, 0.9, 10.0), Combatant { id: 200 + i, team: 1 }));
    }
}

/// Every 0.3 s (SIM time) each team-0 shooter fires a flat projectile at its matched team-1 target (CCD, no gravity).
/// Cadence is the FIXED sim step, not `Time::delta` — a tight headless loop barely advances real wall-clock.
fn fire(mut commands: Commands, mut f: ResMut<Fire>, combatants: Query<(&Combatant, &Transform)>) {
    f.cd -= 1.0 / 60.0; // TimestepMode::Fixed { dt: 1/60 }
    if f.cd > 0.0 || f.shots > 600 {
        return;
    }
    f.cd = 0.3;
    let mut shooters: Vec<(u32, Vec3)> = combatants.iter().filter(|(c, _)| c.team == 0).map(|(c, t)| (c.id, t.translation)).collect();
    let mut targets: Vec<Vec3> = combatants.iter().filter(|(c, _)| c.team == 1).map(|(_, t)| t.translation).collect();
    shooters.sort_by_key(|&(id, _)| id);
    targets.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    for (i, (id, from)) in shooters.iter().enumerate() {
        let Some(&to) = targets.get(i) else { continue };
        let dir = (to - *from).normalize_or_zero();
        if dir == Vec3::ZERO {
            continue;
        }
        // real PaintballGun Bullet: r0.02, full-gravity ARC, airDrag 0, 4 s life (damage unused — tag game)
        spawn_projectile(&mut commands, *from + dir * 1.0, dir * SPEED, *id, 0, ProjectileSpec { radius: 0.02, mass: 0.01, gravity_scale: 1.0, air_drag: 0.0, life: 4.0, damage: 1.0 });
        f.shots += 1;
    }
}

/// A projectile hit → the real PaintballMode scores it as a tag; record team-0's score and the win.
fn score(mut hits: MessageReader<ProjectileHit>, mut rules: ResMut<Rules>, mut out: ResMut<Outcome>) {
    for h in hits.read() {
        let mut events = Vec::new();
        rules.0.input(GameInput::Hit { thrower: h.shooter, victim: h.victim }, &mut events);
        out.tags += 1;
        for ev in events {
            match ev {
                GameEvent::Score { team: 0, score } => out.team0 = score,
                GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } => out.winner = Some(winner),
                _ => {}
            }
        }
    }
}

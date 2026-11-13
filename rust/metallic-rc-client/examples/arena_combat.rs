//! Verify the Arena COMBAT vertical headless + deterministic: a red (team 0) shooter fires REAL rapier projectiles at
//! blue (team 1) players carrying [`Health`]; each kill → [`CombatantDown`] → the real [`gamemodes::arena::ArenaMode`]
//! as an `EnemyPlayer` kill (+10 to red); red reaches the score cap and team 0 wins. Proves
//! fire→fly→damage→down→kill→team-score, the exact wiring the in-client plugin runs over rr-net (minus the net
//! transport). Real Arena Pistol values (80 m/s, flat, body damage 18 vs 100 HP). The cap is reduced to 30 (3 kills)
//! here to keep the run quick; the real `ScoreToWin` is 400 and the pipeline is identical.
//!
//! `cargo run --profile fast -p metallic-rc-client --example arena_combat`
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gamemodes::arena::{ArenaConfig, ArenaMode};
use gamemodes::mode::{GameEvent, GameInput, GameMode, KillKind, RoundPhase};
use metallic_rc_client::combat::{spawn_projectile, CombatPlugin, Combatant, CombatantDown, Health, ProjectileSpec};

#[derive(Resource)]
struct Rules(ArenaMode);

#[derive(Resource, Default)]
struct Fire {
    cd: f32,
    shots: u32,
}

#[derive(Resource, Default)]
struct Outcome {
    winner: Option<Option<u8>>, // Some(Some(team)) = win, Some(None) = draw, None = running
    red: u32,
    kills: u32,
    over: bool,
}

const TARGETS: u32 = 3;
const SPEED: f32 = 80.0; // real Arena Pistol bulletFireSpeed
const HEALTH: f32 = 100.0;
const SPEC: ProjectileSpec = ProjectileSpec { radius: 0.02, mass: 0.02, gravity_scale: 0.0, air_drag: 0.0, life: 4.0, damage: 18.0 }; // Arena Pistol body damage

fn main() {
    let rules = ArenaMode::new(ArenaConfig { score_to_win: 30, game_duration: 480.0 }); // 3 enemy-player kills × +10

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
        .add_systems(Update, (fire, resolve));

    for _ in 0..2400 {
        app.update();
        if app.world().resource::<Outcome>().over {
            break;
        }
    }

    let o = app.world().resource::<Outcome>();
    match o.winner {
        Some(Some(t)) => println!("arena combat: team {t} WINS — red {} pts after {} kills (EnemyPlayer +10) — fire→fly→damage→down→kill→team-score pipeline OK", o.red, o.kills),
        Some(None) => println!("arena combat: DRAW — unexpected"),
        None => println!("arena combat: NO WIN in 40 s — pipeline incomplete (kills never scored)"),
    }
}

fn setup(mut commands: Commands) {
    // red (team 0) shooter + blue (team 1) targets with Health, 10 m downrange
    commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(0.0, 0.9, 0.0), Combatant { id: 10, team: 0 }, Health::full(HEALTH)));
    for i in 0..TARGETS {
        let x = (i as f32 - (TARGETS as f32 - 1.0) / 2.0) * 2.0;
        commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, 0.9, 10.0), Combatant { id: 20 + i, team: 1 }, Health::full(HEALTH)));
    }
}

/// Focus-fire: the red shooter fires at the nearest living blue target (fixed sim-step cadence).
fn fire(mut commands: Commands, mut f: ResMut<Fire>, combatants: Query<(&Combatant, &Transform, &Health)>) {
    f.cd -= 1.0 / 60.0; // TimestepMode::Fixed { dt: 1/60 } — not Time::delta (near-zero in a tight headless loop)
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
    spawn_projectile(&mut commands, from + dir * 1.0, dir * SPEED, 10, 0, SPEC);
    f.shots += 1;
}

/// A kill → the real ArenaMode as a kill-point input (cross-team = EnemyPlayer +10); record red's score + the win.
fn resolve(mut downs: MessageReader<CombatantDown>, mut rules: ResMut<Rules>, mut out: ResMut<Outcome>) {
    for d in downs.read() {
        out.kills += 1;
        let kind = if d.killer_team == d.victim_team { KillKind::Teammate } else { KillKind::EnemyPlayer };
        let mut events = Vec::new();
        rules.0.input(GameInput::Kill { team: d.killer_team, kind }, &mut events);
        for ev in events {
            match ev {
                GameEvent::Score { team: 0, score } => out.red = score,
                GameEvent::Round { phase: RoundPhase::GameOver, winner, .. } => {
                    out.winner = Some(winner);
                    out.over = true;
                }
                _ => {}
            }
        }
    }
}

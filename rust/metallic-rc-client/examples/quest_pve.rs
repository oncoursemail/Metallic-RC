//! Verify the ENEMY-AI + Quest PvE loop headless: a co-op team (players with Health) faces a wave of enemies
//! ([`metallic_rc_client::enemy`]). The enemies PURSUE the players and ATTACK (melee → player Health down). The
//! players auto-fire real projectiles and KILL the enemies → [`EnemyKilled`] → the real
//! [`gamemodes::quest::QuestMode`] awards loot; clearing the wave is the objective → campaign WON. Proves
//! pursue→attack and player-kills-enemy→loot→objective→win in one run. Enemy + weapon stats are placeholders (pending
//! the enemy dump). Deterministic (fixed-step Time).
//!
//! `cargo run --profile fast -p metallic-rc-client --example quest_pve`
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_rapier3d::prelude::*;
use gamemodes::mode::{GameEvent, GameInput, GameMode};
use gamemodes::quest::{QuestConfig, QuestMode};
use metallic_rc_client::combat::{spawn_projectile, CombatPlugin, Combatant, CombatantDown, Health, ProjectileSpec};
use metallic_rc_client::enemy::{spawn_enemy, Enemy, EnemyAttack, EnemyKilled, EnemyPlugin};

const PLAYERS: u32 = 2;
const ENEMIES: u32 = 4;
const PLAYER_HP: f32 = 100.0;
const ENEMY_HP: f32 = 50.0;
const ENEMY_TEAM: u8 = 9;
const PISTOL: ProjectileSpec = ProjectileSpec { radius: 0.02, mass: 0.02, gravity_scale: 0.0, air_drag: 0.0, life: 4.0, damage: 18.0 };

#[derive(Resource)]
struct Rules(QuestMode);

#[derive(Resource, Default)]
struct Fire {
    cd: f32,
}

#[derive(Resource, Default)]
struct Outcome {
    kills: u32,
    loot: i32,
    enemy_hits: u32,
    objective_done: bool,
    won: bool,
    lost: bool,
    over: bool,
}

fn main() {
    let mut rules = QuestMode::new(QuestConfig { story_states: 1, continue_budget: 0, death_penalty: 1200, ff_penalty: 1200, collectible_value: 50 });
    for i in 0..PLAYERS {
        rules.add_player(1 + i); // players ids 1..=PLAYERS (enemies use ids 100+)
    }

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TransformPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 4 })
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(CombatPlugin)
        .add_plugins(EnemyPlugin)
        .insert_resource(Rules(rules))
        .init_resource::<Fire>()
        .init_resource::<Outcome>()
        .add_systems(Startup, setup)
        .add_systems(Update, (player_fire, resolve));

    for _ in 0..1800 {
        // up to 30 s
        app.update();
        if app.world().resource::<Outcome>().over {
            break;
        }
    }

    let o = app.world().resource::<Outcome>();
    let result = if o.won && o.kills == ENEMIES && o.enemy_hits > 0 { "pipeline OK" } else { "CHECK" };
    println!(
        "quest pve: {} — enemies killed {}/{}, loot {}, enemy attacks landed {}, objective {}, {} — pursue→attack + kill→loot→objective→win {result}",
        if o.won { "campaign WON" } else if o.lost { "team WIPED (lost)" } else { "unresolved" },
        o.kills,
        ENEMIES,
        o.loot,
        o.enemy_hits,
        o.objective_done,
        if o.won { "WIN" } else { "no win" },
    );
}

fn setup(mut commands: Commands) {
    commands.spawn((RigidBody::Fixed, Collider::cuboid(100.0, 0.1, 100.0), Transform::from_xyz(0.0, -0.1, 0.0))); // ground
    // co-op team near the origin (team 0), player-sized cuboids with Health, emitting collision events
    for i in 0..PLAYERS {
        let x = (i as f32 - (PLAYERS as f32 - 1.0) / 2.0) * 1.5;
        commands.spawn((RigidBody::Fixed, Collider::cuboid(0.3, 0.9, 0.3), ActiveEvents::COLLISION_EVENTS, Transform::from_xyz(x, 0.9, 0.0), Combatant { id: 1 + i, team: 0 }, Health::full(PLAYER_HP)));
    }
    // a wave of melee enemies ringing the team at ~5 m (close enough to land attacks before they die)
    for i in 0..ENEMIES {
        let a = i as f32 / ENEMIES as f32 * std::f32::consts::TAU;
        let (x, z) = (a.cos() * 5.0, a.sin() * 5.0 + 3.0);
        spawn_enemy(&mut commands, Transform::from_xyz(x, 0.6, z), Combatant { id: 100 + i, team: ENEMY_TEAM }, ENEMY_HP, Vec3::new(0.3, 0.6, 0.3), Enemy::melee_grunt());
    }
}

/// Every 0.2 s (sim) each living player fires at the nearest living enemy.
fn player_fire(mut commands: Commands, mut f: ResMut<Fire>, players: Query<(&Combatant, &Transform, &Health), Without<Enemy>>, enemies: Query<(&Transform, &Health), With<Enemy>>) {
    f.cd -= 1.0 / 60.0;
    if f.cd > 0.0 {
        return;
    }
    f.cd = 0.2;
    let targets: Vec<Vec3> = enemies.iter().filter(|(_, h)| h.current > 0.0).map(|(t, _)| t.translation).collect();
    if targets.is_empty() {
        return;
    }
    for (c, tf, h) in &players {
        if h.current <= 0.0 {
            continue;
        }
        let from = tf.translation;
        let Some(&to) = targets.iter().min_by(|a, b| from.distance(**a).partial_cmp(&from.distance(**b)).unwrap_or(std::cmp::Ordering::Equal)) else { continue };
        let dir = (to - from).normalize_or_zero();
        if dir == Vec3::ZERO {
            continue;
        }
        spawn_projectile(&mut commands, from + dir * 1.0, dir * 80.0, c.id, c.team, PISTOL);
    }
}

/// Score the loop: enemy kills → loot (AwardLoot); a player down → PlayerDown; wave cleared → ObjectiveComplete; track
/// the Quest win/lose.
fn resolve(mut killed: MessageReader<EnemyKilled>, mut attacks: MessageReader<EnemyAttack>, mut downs: MessageReader<CombatantDown>, mut rules: ResMut<Rules>, mut out: ResMut<Outcome>) {
    if out.over {
        return;
    }
    let mut events: Vec<GameEvent> = Vec::new();
    for k in killed.read() {
        out.kills += 1;
        rules.0.input(GameInput::AwardLoot { amount: k.loot }, &mut events);
    }
    for _ in attacks.read() {
        out.enemy_hits += 1;
    }
    for d in downs.read() {
        if d.victim < 100 {
            // a player went down (enemy ids are 100+)
            rules.0.input(GameInput::PlayerDown { player: d.victim, friendly_fire: false }, &mut events);
        }
    }
    // the objective: clear the wave (every enemy accounted as killed — gating on the count ensures the final kill's
    // loot is tallied before the campaign completes)
    if !out.objective_done && out.kills >= ENEMIES {
        out.objective_done = true;
        rules.0.input(GameInput::ObjectiveComplete, &mut events);
    }
    for ev in events {
        match ev {
            GameEvent::PlayerScore { score, .. } => out.loot = score,
            GameEvent::Cue(c) if c == "questWon" => {
                out.won = true;
                out.over = true;
            }
            GameEvent::Cue(c) if c == "questLost" => {
                out.lost = true;
                out.over = true;
            }
            _ => {}
        }
    }
}

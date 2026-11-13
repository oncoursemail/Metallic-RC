//! Reusable ENEMY AI on the combat foundation ([`crate::combat`]) — NPC enemies for Quest (PvE) and the Arena horde.
//! An enemy is a [`Combatant`] + [`Health`] (so the players' projectile-combat kills it the normal way) + an [`Enemy`]
//! marker (class + behavior + stats). The AI pursues the nearest player and attacks: MELEE (damage the player's Health
//! on contact, on a cooldown) or RANGED (fire a projectile via the combat foundation). When an enemy's Health hits 0
//! the module raises [`EnemyKilled`] (class + loot), which the game scores — Arena NormalEnemy/EliteEnemy kill-points,
//! Quest loot + the objective. When an enemy's hit downs a PLAYER it raises the foundation's [`CombatantDown`] (killer
//! = the enemy), which the game reads as a PlayerDown. Enemy stats are the real per-enemy dump values; this module is
//! the value-independent AI mechanism (stats flagged `// 1:1:` pending the enemy recon).

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::combat::{spawn_projectile, Combatant, CombatantDown, Health, ProjectileSpec};

/// Enemy classification — the Arena kill table (NormalEnemy +2, EliteEnemy +10).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyClass {
    Normal,
    Elite,
}

/// How an enemy attacks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyBehavior {
    Melee,
    Ranged,
}

/// An NPC enemy's AI + stats. 1:1: the stats are PLACEHOLDERS pending the per-enemy dump (requested) — reconcile each
/// to the real serialized value when the enemy recon lands. Real enemy types: QuestKnight/Goblin/Melee/Lunging/
/// Explosive/Cannoneer + Dracula/Jumbotron bosses (+ the Arena horde).
#[derive(Component)]
pub struct Enemy {
    pub class: EnemyClass,
    pub behavior: EnemyBehavior,
    pub move_speed: f32,
    pub attack_range: f32,
    pub attack_damage: f32,
    pub attack_cooldown: f32,
    pub projectile_speed: f32, // ranged only
    pub loot: i32,             // loot / kill-points on death
    pub cd: f32,               // internal attack-cooldown timer
}

impl Enemy {
    /// A placeholder NORMAL melee grunt (1:1: pending real stats).
    pub fn melee_grunt() -> Self {
        Enemy { class: EnemyClass::Normal, behavior: EnemyBehavior::Melee, move_speed: 4.0, attack_range: 2.0, attack_damage: 20.0, attack_cooldown: 1.0, projectile_speed: 0.0, loot: 50, cd: 0.0 }
    }
    /// A placeholder NORMAL ranged enemy (1:1: pending real stats).
    pub fn ranged() -> Self {
        Enemy { class: EnemyClass::Normal, behavior: EnemyBehavior::Ranged, move_speed: 3.0, attack_range: 18.0, attack_damage: 12.0, attack_cooldown: 1.5, projectile_speed: 40.0, loot: 50, cd: 0.0 }
    }
    /// A placeholder ELITE (tougher, hits harder; Arena +10) (1:1: pending real stats).
    pub fn elite() -> Self {
        Enemy { class: EnemyClass::Elite, behavior: EnemyBehavior::Melee, move_speed: 5.0, attack_range: 2.5, attack_damage: 35.0, attack_cooldown: 1.2, projectile_speed: 0.0, loot: 100, cd: 0.0 }
    }

    fn projectile_spec(&self) -> ProjectileSpec {
        ProjectileSpec { radius: 0.05, mass: 0.02, gravity_scale: 0.0, air_drag: 0.0, life: 4.0, damage: self.attack_damage }
    }
}

/// Raised when an enemy is killed (its Health hit 0). The game scores it (Arena kill-points / Quest loot + kill stat).
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyKilled {
    pub killer: u32,
    pub killer_team: u8,
    pub class: EnemyClass,
    pub loot: i32,
}

/// Raised when an enemy's attack lands on a player (feedback; the game may also react to the resulting CombatantDown).
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyAttack {
    pub enemy_team: u8,
    pub target: u32,
    pub damage: f32,
}

/// The reusable enemy-AI plugin: pursue + attack + death→[`EnemyKilled`] + the messages. Add alongside [`crate::combat::CombatPlugin`].
pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EnemyKilled>().add_message::<EnemyAttack>().add_systems(Update, (enemy_pursue, enemy_attack, enemy_deaths));
    }
}

/// Spawn an NPC enemy: a dynamic rapier body (upright + spin locked, moves via velocity) that is a combatant with
/// health + the [`Enemy`] AI. `combatant` sets its id + horde team; `collider_half` is its body half-extents.
pub fn spawn_enemy(commands: &mut Commands, transform: Transform, combatant: Combatant, health: f32, collider_half: Vec3, enemy: Enemy) -> Entity {
    commands
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(collider_half.x, collider_half.y, collider_half.z),
            ActiveEvents::COLLISION_EVENTS,
            LockedAxes::ROTATION_LOCKED_X | LockedAxes::ROTATION_LOCKED_Y | LockedAxes::ROTATION_LOCKED_Z,
            Velocity::zero(),
            combatant,
            Health::full(health),
            enemy,
            transform,
        ))
        .id()
}

fn horiz_dist(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

/// Pursue the nearest living player: move horizontally toward it at `move_speed` until within attack range, then hold
/// (the vertical velocity is preserved for gravity / the ground).
fn enemy_pursue(mut enemies: Query<(&Enemy, &Transform, &mut Velocity, &Health)>, players: Query<(&Combatant, &Transform, &Health), Without<Enemy>>) {
    let targets: Vec<Vec3> = players.iter().filter(|(_, _, h)| h.current > 0.0).map(|(_, t, _)| t.translation).collect();
    for (enemy, tf, mut vel, eh) in &mut enemies {
        if eh.current <= 0.0 {
            vel.linear = Vec3::new(0.0, vel.linear.y, 0.0);
            continue;
        }
        let Some(&nearest) = targets.iter().min_by(|a, b| horiz_dist(tf.translation, **a).partial_cmp(&horiz_dist(tf.translation, **b)).unwrap_or(std::cmp::Ordering::Equal)) else {
            vel.linear = Vec3::new(0.0, vel.linear.y, 0.0);
            continue;
        };
        let to = Vec3::new(nearest.x - tf.translation.x, 0.0, nearest.z - tf.translation.z);
        if to.length() > enemy.attack_range * 0.8 {
            vel.linear = to.normalize_or_zero() * enemy.move_speed + Vec3::Y * vel.linear.y;
        } else {
            vel.linear = Vec3::new(0.0, vel.linear.y, 0.0); // in range: stop and attack
        }
    }
}

/// Attack the nearest player within range on cooldown: melee damages its Health directly (raising CombatantDown on a
/// kill); ranged fires a projectile. Always emits [`EnemyAttack`].
fn enemy_attack(
    time: Res<Time>,
    mut commands: Commands,
    mut enemies: Query<(&Combatant, &mut Enemy, &Transform, &Health)>,
    mut players: Query<(Entity, &Combatant, &Transform, &mut Health), Without<Enemy>>,
    mut atk_out: MessageWriter<EnemyAttack>,
    mut down_out: MessageWriter<CombatantDown>,
) {
    let dt = time.delta_secs();
    let snap: Vec<(Entity, u32, u8, Vec3)> = players.iter().filter(|(_, _, _, h)| h.current > 0.0).map(|(e, c, t, _)| (e, c.id, c.team, t.translation)).collect();
    if snap.is_empty() {
        return;
    }
    for (ec, mut enemy, etf, eh) in &mut enemies {
        if eh.current <= 0.0 {
            continue;
        }
        enemy.cd -= dt;
        let epos = etf.translation;
        let &(pe, pid, pteam, ppos) = snap.iter().min_by(|a, b| horiz_dist(epos, a.3).partial_cmp(&horiz_dist(epos, b.3)).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
        if horiz_dist(epos, ppos) > enemy.attack_range || enemy.cd > 0.0 {
            continue;
        }
        enemy.cd = enemy.attack_cooldown;
        atk_out.write(EnemyAttack { enemy_team: ec.team, target: pid, damage: enemy.attack_damage });
        match enemy.behavior {
            EnemyBehavior::Melee => {
                if let Ok((_, _, _, mut ph)) = players.get_mut(pe) {
                    let was_alive = ph.current > 0.0;
                    ph.current -= enemy.attack_damage;
                    if was_alive && ph.current <= 0.0 {
                        down_out.write(CombatantDown { victim: pid, victim_team: pteam, killer: ec.id, killer_team: ec.team });
                    }
                }
            }
            EnemyBehavior::Ranged => {
                let dir = Vec3::new(ppos.x - epos.x, 0.0, ppos.z - epos.z).normalize_or_zero();
                if dir != Vec3::ZERO {
                    spawn_projectile(&mut commands, epos + dir * 1.0 + Vec3::Y * 0.5, dir * enemy.projectile_speed, ec.id, ec.team, enemy.projectile_spec());
                }
            }
        }
    }
}

/// On any combatant death, if the victim was an [`Enemy`], raise [`EnemyKilled`] (class + loot) for the game to score.
fn enemy_deaths(mut downs: MessageReader<CombatantDown>, enemies: Query<(&Combatant, &Enemy)>, mut killed_out: MessageWriter<EnemyKilled>) {
    for d in downs.read() {
        if let Some((_, enemy)) = enemies.iter().find(|(c, _)| c.id == d.victim) {
            killed_out.write(EnemyKilled { killer: d.killer, killer_team: d.killer_team, class: enemy.class, loot: enemy.loot });
        }
    }
}

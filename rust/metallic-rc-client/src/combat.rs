//! Reusable PROJECTILE-COMBAT system (bevy_rapier) — the shared mechanism for the shooter games (Paintball,
//! BattleRoyale, Arena, Quest). A gun fires a projectile ([`spawn_projectile`] with a per-weapon [`ProjectileSpec`]);
//! the projectile flies (CCD, so it can't tunnel at speed) and, on hitting a [`Combatant`], raises a generic
//! [`ProjectileHit`]. Combatants that carry [`Health`] take the projectile's damage, and when a hit drops one to zero
//! the system raises [`CombatantDown`] — so the two gameplay shapes are both covered:
//!
//! - **Tag games** (Paintball): combatants have NO `Health`; each hit is a [`ProjectileHit`] the game scores as a tag.
//! - **Kill games** (BattleRoyale / Arena / Quest): combatants carry `Health`; damage accumulates and the lethal hit
//!   raises [`CombatantDown`], which the game maps to an elimination / kill.
//!
//! A projectile is spent on any impact (combatant or wall). The projectile dynamics + damage are per-game REAL dump
//! values carried in the `ProjectileSpec` — this module is the value-independent mechanism. De-risked headless
//! (examples/projectile_hit.rs): a fast CCD projectile reliably fires rapier's collision event on impact.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// A combatant a projectile can hit — a player or an enemy — tagged with its gameplay id + team. Put it on the entity
/// that carries the body collider (the avatar capsule, the enemy). Add [`Health`] for a kill game; omit it for a tag
/// game.
#[derive(Component, Clone, Copy)]
pub struct Combatant {
    pub id: u32,
    pub team: u8,
}

/// Hit points for a kill-game combatant. When a projectile's damage drops `current` to `<= 0`, the combat system
/// raises [`CombatantDown`] once (on the lethal transition). Tag-game combatants omit this component.
#[derive(Component, Clone, Copy)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn full(max: f32) -> Self {
        Health { current: max, max }
    }
}

/// A live projectile: who fired it (so it can't hit its own shooter), its remaining lifetime (s), and its damage.
#[derive(Component)]
pub struct Projectile {
    pub shooter: u32,
    pub shooter_team: u8,
    pub life: f32,
    pub damage: f32,
}

/// Per-weapon projectile dynamics + damage — the REAL dump values for a game's weapon (speed is applied by the caller
/// as the launch velocity; these are the body + payload). The real RR bullets are custom-integrated (not rigidbodies,
/// so massless) — our rapier proxy is a ball collider + gravity + linear damping, which reproduces the trajectory
/// because gravity and drag are mass-independent, so `mass` is a nominal value that only matters if a projectile ever
/// imparts impulse (ours despawn on impact). `gravity_scale` 0 = flat flight (Arena/Quest), 1 = full arc (Paintball);
/// `air_drag` = the bullet's airDrag (linear damping, usually 0).
#[derive(Clone, Copy)]
pub struct ProjectileSpec {
    pub radius: f32,
    pub mass: f32,
    pub gravity_scale: f32,
    pub air_drag: f32,
    pub life: f32,
    pub damage: f32,
}

/// Raised when a projectile strikes a combatant (never its own shooter). Tag games map it to their scoring; it carries
/// the projectile's `damage` for UI/feedback (kill games act on [`CombatantDown`] instead).
#[derive(Message, Clone, Copy, Debug)]
pub struct ProjectileHit {
    pub shooter: u32,
    pub shooter_team: u8,
    pub victim: u32,
    pub victim_team: u8,
    pub damage: f32,
}

/// Raised once when accumulated damage drops a [`Health`] combatant to zero — the kill. Kill games map it to an
/// elimination (BattleRoyale) or a kill (Arena / Quest).
#[derive(Message, Clone, Copy, Debug)]
pub struct CombatantDown {
    pub victim: u32,
    pub victim_team: u8,
    pub killer: u32,
    pub killer_team: u8,
}

/// The reusable combat plugin: the projectile lifetime + hit/damage systems + the [`ProjectileHit`] and
/// [`CombatantDown`] messages. Add once; the shooter games fire via [`spawn_projectile`] and consume the messages.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProjectileHit>().add_message::<CombatantDown>().add_systems(Update, (projectile_lifetime, projectile_hits));
    }
}

/// Fire a projectile: a CCD rapier ball from `origin` at `velocity`, by `shooter` (team), with the weapon's `spec`.
/// Spawn it at the muzzle (slightly in front of the shooter) so it doesn't collide with the shooter on frame one.
pub fn spawn_projectile(commands: &mut Commands, origin: Vec3, velocity: Vec3, shooter: u32, shooter_team: u8, spec: ProjectileSpec) -> Entity {
    commands
        .spawn((
            RigidBody::Dynamic,
            Collider::ball(spec.radius),
            ColliderMassProperties::Mass(spec.mass),
            Ccd::enabled(),
            GravityScale(spec.gravity_scale),
            Damping { linear_damping: spec.air_drag, angular_damping: 0.0 },
            Velocity { linear: velocity, angular: Vec3::ZERO },
            ActiveEvents::COLLISION_EVENTS,
            Transform::from_translation(origin),
            Projectile { shooter, shooter_team, life: spec.life, damage: spec.damage },
        ))
        .id()
}

/// Despawn projectiles that have outlived their lifetime.
fn projectile_lifetime(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Projectile)>) {
    let dt = time.delta_secs();
    for (e, mut p) in &mut q {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

/// On a projectile impact: if it struck a combatant (not its own shooter), raise [`ProjectileHit`], apply damage to
/// the combatant's [`Health`] if it has any, and raise [`CombatantDown`] on the lethal hit. The projectile is spent on
/// any impact (combatant or wall).
fn projectile_hits(
    mut commands: Commands,
    mut ev: MessageReader<CollisionEvent>,
    projectiles: Query<&Projectile>,
    mut combatants: Query<(&Combatant, Option<&mut Health>)>,
    mut hit_out: MessageWriter<ProjectileHit>,
    mut down_out: MessageWriter<CombatantDown>,
) {
    for e in ev.read() {
        let CollisionEvent::Started(a, b, _) = e else { continue };
        // which side is the projectile?
        let (proj_e, other) = if projectiles.contains(*a) {
            (*a, *b)
        } else if projectiles.contains(*b) {
            (*b, *a)
        } else {
            continue;
        };
        if let Ok(p) = projectiles.get(proj_e) {
            if let Ok((c, health)) = combatants.get_mut(other) {
                if p.shooter != c.id {
                    hit_out.write(ProjectileHit { shooter: p.shooter, shooter_team: p.shooter_team, victim: c.id, victim_team: c.team, damage: p.damage });
                    if let Some(mut h) = health {
                        let was_alive = h.current > 0.0;
                        h.current -= p.damage;
                        if was_alive && h.current <= 0.0 {
                            down_out.write(CombatantDown { victim: c.id, victim_team: c.team, killer: p.shooter, killer_team: p.shooter_team });
                        }
                    }
                }
            }
        }
        commands.entity(proj_e).despawn(); // spent on any impact
    }
}

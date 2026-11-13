//! Reusable PROJECTILE-COMBAT system (bevy_rapier) — the shared mechanism for the shooter games (Paintball,
//! BattleRoyale, Arena, Quest). A gun fires a projectile ([`spawn_projectile`]); the projectile flies (CCD, so it
//! can't tunnel at speed) and, on hitting a [`Combatant`], raises a generic [`ProjectileHit`] that each game maps to
//! its own scoring — Paintball tags (+ drops a CTF flag), BattleRoyale eliminates, Arena/Quest kill/damage. Spent on
//! any impact (combatant or wall). The projectile physics (radius / mass / gravity scale / speed / lifetime) are the
//! per-game REAL dump values, passed in by the caller — this module is the value-independent mechanism.
//!
//! De-risked headless (examples/projectile_hit.rs): a fast CCD projectile reliably fires rapier's collision event on
//! impact with a target.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// A combatant a projectile can hit — a player or an enemy — tagged with its gameplay id + team. Put it on the entity
/// that carries the body collider (the avatar capsule, the enemy).
#[derive(Component, Clone, Copy)]
pub struct Combatant {
    pub id: u32,
    pub team: u8,
}

/// A live projectile: who fired it (so it can't tag its own shooter) + its remaining lifetime (seconds).
#[derive(Component)]
pub struct Projectile {
    pub shooter: u32,
    pub shooter_team: u8,
    pub life: f32,
}

/// Raised when a projectile strikes a combatant (never its own shooter). Each game maps it to its scoring.
#[derive(Message, Clone, Copy, Debug)]
pub struct ProjectileHit {
    pub shooter: u32,
    pub shooter_team: u8,
    pub victim: u32,
    pub victim_team: u8,
}

/// The reusable combat plugin: the projectile lifetime + hit systems + the [`ProjectileHit`] message. Add once; the
/// shooter games fire via [`spawn_projectile`] and consume `ProjectileHit`.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProjectileHit>().add_systems(Update, (projectile_lifetime, projectile_hits));
    }
}

/// Fire a projectile: a small fast CCD rapier ball from `origin` at `velocity`, by `shooter` (team), living `life` s.
/// `radius` / `mass` / `gravity_scale` are the real per-game dump values (gravity_scale 0 = flat flight). Spawn it at
/// the muzzle (slightly in front of the shooter) so it doesn't collide with the shooter on frame one.
#[allow(clippy::too_many_arguments)]
pub fn spawn_projectile(commands: &mut Commands, origin: Vec3, velocity: Vec3, shooter: u32, shooter_team: u8, radius: f32, mass: f32, gravity_scale: f32, life: f32) -> Entity {
    commands
        .spawn((
            RigidBody::Dynamic,
            Collider::ball(radius),
            ColliderMassProperties::Mass(mass),
            Ccd::enabled(),
            GravityScale(gravity_scale),
            Velocity { linear: velocity, angular: Vec3::ZERO },
            ActiveEvents::COLLISION_EVENTS,
            Transform::from_translation(origin),
            Projectile { shooter, shooter_team, life },
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

/// On a projectile impact: raise [`ProjectileHit`] if it struck a combatant (not its own shooter), and despawn the
/// projectile — it's spent on any impact (combatant or wall).
fn projectile_hits(mut commands: Commands, mut ev: MessageReader<CollisionEvent>, projectiles: Query<&Projectile>, combatants: Query<&Combatant>, mut out: MessageWriter<ProjectileHit>) {
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
        if let (Ok(p), Ok(c)) = (projectiles.get(proj_e), combatants.get(other)) {
            if p.shooter != c.id {
                out.write(ProjectileHit { shooter: p.shooter, shooter_team: p.shooter_team, victim: c.id, victim_team: c.team });
            }
        }
        commands.entity(proj_e).despawn(); // spent on any impact
    }
}

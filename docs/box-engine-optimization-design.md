# Box engine — scale-physics optimization (design note, PARKED)

Status: **captured, parked for the optimization phase.** Do NOT build yet. The current priority is getting the
games 1:1 *playable* (the combat / kart / AI verticals) on `bevy_rapier`, which is proven for that (Bowling
pin-toppling vertical + projectile hit-detection are de-risked). Optimization comes **after** the games are
playable and we can measure real limits on target devices. This note exists so the idea isn't lost and so the
optimization phase has empirical targets to aim at. Owner must approve before any build (brainstorming gate).

## Intent / why

The owner's goal is **scale performance for games**: many players, many assets, and many *moving* objects per
game, running far faster than a general-purpose rigidbody solver can. Rec Room–style play (dodgeball volleys,
paintball/BR/Arena/Quest projectile crowds, props, enemies, karts, and many players at once) is bounded by how
many dynamic bodies the physics can push per frame — especially on Quest / mobile. The insight: *most* game
objects don't need PhysX-perfect realism, they need **fast, good-enough** collision and response at high counts.

## Goals

- 1:1 *gameplay* behavior preserved (collision outcomes match Rec Room for each ported game).
- Far higher moving-object + player counts per frame than the current rapier-per-body approach, on target devices.
- Deterministic (enables rollback / lockstep netcode at scale — ties to the device-aware replication already built).

## Non-goals / anti-goals

- NOT a from-scratch rewrite that throws away rapier's precise dynamics where they matter.
- NOT perfect rigid-body realism for every object (that's the overhead we're cutting).
- Failure = a faster engine whose gameplay no longer matches the dump-verified 1:1 behavior. Fidelity first.

## Where rapier's limits actually bite (measure before committing)

- **Per-body / per-collider overhead** at high counts (broad-phase, island solving, contact graph) — the Bowling
  lane already mirrors ~818 static box colliders; a combat arena adds projectiles + props + many players.
- **General-purpose solver** doing full constraint work for objects that only need "bounce / push / fly."
- **Determinism** across devices for netcode is not rapier's strong suit.
- These are *hypotheses* — the optimization phase measures them on real playable games on Quest/desktop first.

## Approaches

1. **Hybrid box engine (recommended).** A custom box/sphere collision layer for the *many* — players, projectiles,
   moving props, enemies, and the static box world — with: a spatial grid/BVH over OBBs for broad-phase that scales
   to thousands of objects; fast narrow-phase (OBB-vs-OBB SAT, sphere-vs-OBB, ray-vs-OBB, SIMD-friendly); a simple
   stable velocity/push response; deterministic math. Keep **rapier only for the handful needing true rigidbody
   realism** (e.g. Bowling pin-toppling, where exact 1:1 matters). Box engine carries the crowd; rapier carries the
   few precise bits. Grows alongside what runs today; lowest risk for the fidelity bar.
2. **Full custom** (no rapier). Max performance + control, but the precise 1:1 dynamics must be re-derived — higher
   risk against the fidelity bar.
3. **Just optimize rapier** (merge static boxes, sleeping/islands, LOD, fixed timestep). Least work; ceiling capped
   by rapier's general-purpose design — likely won't reach "way better" at thousands of objects.

## Hybrid split (the decided shape, if approved)

- **Box-engine path (the many):** static room boxes, players, projectiles, generic moving props, enemies, karts.
- **Rapier path (the precise few):** objects whose 1:1 behavior needs the real solver (Bowling pins; anything the
  dump shows uses true rigidbody dynamics).
- A shared broadphase so the two paths can collide against each other (e.g. a projectile from the box engine
  registering a hit on a rapier body, or vice-versa) — the integration seam is the main design risk to prototype.

## Ties to device-aware replication

The project already replicates with device awareness (see `docs/agent-reports/37-player-count.md` and the
realtime/replication work). A deterministic box engine lets the simulation scale with the replication budget:
higher object/player counts where the device + network allow, graceful LOD where they don't — the box engine is
the sim side of the same "beyond player limits / device-aware" goal.

## What's left to the builder (at optimization time)

- Measure rapier's real limits on the *playable* games (Quest + desktop) to set empirical targets.
- Prototype the box broadphase + narrow-phase + the hybrid integration seam (a spike/benchmark: N moving boxes,
  custom vs rapier, headless frame cost).
- Decide the determinism strategy (careful f32 vs fixed-point) against the netcode needs.
- Migrate object classes from rapier → box engine one at a time, verifying 1:1 behavior at each step.

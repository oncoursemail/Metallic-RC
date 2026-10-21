> **Direction update — 20 October 2026:** use the owner's actual compiled client as the starting point. The [compiled-client rework](../COMPILED_CLIENT_REWORK.md) supersedes earlier replacement-engine/hub proposals below. These plans do not establish shipped gameplay.

# ADR-0002: Build the Metallic creator runtime before attempting a ground-up 3D engine

- **Date:** 2026-10-18
- **Status:** proposed; technical engine choice **not yet decided** by benchmark
- **Owners:** Metallic engine/game team
- **Related:** [ADR/architecture](../ARCHITECTURE.md), [engine benchmark #1](https://github.com/oncoursemail/Metallic-RC/issues/1)

## Decision

Implement original **Metallic Runtime** semantics (scene package format, typed Circuits graph VM, asset catalog and moderation, authoritative event/achievement API, deterministic NPC behaviors, creator permissions, watch UI contracts and plugins) **on top of one measured production-quality rendering/physics/XR engine** first.

Do **not** claim a full proprietary 3D renderer/physics/device stack is built; Unity or Godot is still to be selected after the same two-player desktop/VR/Quest demo is measured.

## Why

The user's priority is an independent game where people can play, create and teach. Rewriting OpenXR integrations, a production physics engine, renderer, editor, build pipeline, animation and platform SDKs from scratch would delay that outcome and worsen early reliability. The independent parts that differentiate Metallic are creator UX, content/runtime format, multiplayer logic and safer AI systems.

## Boundary

```text
[Worlds/Scenes/Actors/Watch/CV3/Pets/Workshops/Asset manifests]
                         |
              [Metallic Runtime API]
        [typed rules | permissions | event log]
          |                               |
   [Engine adapter]               [Owned game services]
   graphics/input/VR              sessions, identity, UGC,
   physics/animation              moderation, economy later
```

Stable versioned JSON/Protobuf contracts and test fixtures avoid coupling creators to a particular engine. Runtime rules remain independent of UI. Game-server evidence is necessary before trusting any award, purchase, scoring or private data action.

## Prototypes in this PR

- Hub manifest validator / watch-map projection.
- Creator catalog validation with published-revision and age safety filters.
- Bounded Roomie state machine with simulated emotion cues.
- Workshop definition validation and a sample advanced curriculum.
- Achievement trust-contract and idempotent award projection.
- Cross-input watch tab model.

**Explicit limitations:** these are dependency-free Node modules and design JSON, not Unity/Godot plugins; no networking, sessions, save database, 3D models, ML inference or actual user-visible gameplay yet.

## Decision gates

1. Full CPU/GPU profiling on representative Quest/PCVR/desktop hardware of identical original scene/content loads, including OpenXR hand poses, controller mapping and comfort checks.
2. Two live players + dedicated authority with identical interactions, reconnect, objects and baseline voice; quantify p50/p95 frame/render/network cost.
3. Licensing, source access, current toolchain support, target-platform CI, package size and operational skills comparison.
4. Provider/engine escape-hatch design: creators retain schema/code ownership, asset package versioning and migrations tested.
5. If custom rendering or physics subsystem is proposed later, require a measured performance or maintainability win and an A/B migration path.

## Consequences

- Immediate progress goes into distinctive creator systems, safer behavior, original art and a usable game first.
- We accept some engine-specific implementation and training cost, but isolate it behind adapters.
- We avoid unfounded promises that custom code necessarily runs faster than established engines.
- This ADR should be superseded when real comparative evidence and a shipping client commit exist.

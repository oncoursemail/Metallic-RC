# ADR: a bounded native Metallic Meadow experiment

**Date:** 2026-10-20

**Status:** Accepted for a private desktop experiment. The production engine and VR decisions remain open.

**Implementation:** [Native playtest and operating instructions](../game/native/README.md)

## Context

Metallic-RC's existing architecture describes an independent social game with native desktop and VR clients, authoritative rooms, creator systems, and a separate service/control plane. It proposes comparing Unity/OpenXR with Godot/OpenXR before selecting the production engine. Repository contracts and platform plans alone do not provide a player-visible loop for testing movement, shared activity, reconnect behavior, visual quality, or builds.

The project now needs a small executable experiment whose result can be inspected and tested. It also needs an explicit boundary so that one prototype cannot silently settle the larger engine, physics, networking, or VR choices.

## Decision

Implement **Metallic Meadow** in a contained `game/native` Godot project, pinned to the official **Godot 4.5.2 stable** editor and matching export templates. Use GDScript, the Compatibility renderer, and a single authoritative fixed-step simulation with an ENet session adapter. Export separate Windows, Linux desktop, and Linux dedicated-server targets. The dedicated path detects the server export feature or `--server` and constructs no graphical world.

The experiment includes original script-authored environment geometry, articulated visitors, a compact HUD, a live map, walking/jumping, three shared trail markers, and a barn bell. Shared world data connects the authored layout to authority collision and interaction rules. Client input requests movement and actions; clients do not supply authoritative positions or round completion.

Local hosting and explicit-address joining are sufficient for this experiment. The 32-player admission limit bounds its state and test matrix. It is not evidence that a public room, Internet service, VR client, or specific device can sustain 32 players at a promised frame rate.

## Boundaries retained

| Area | Decision retained |
| --- | --- |
| Production engine | Unity/OpenXR versus Godot/OpenXR comparison remains unresolved. Godot 4.5.2 is pinned only for this executable experiment. |
| VR and mobile | No inferred support from desktop rendering. Device interaction, comfort, tracking, graphics budgets, permissions, and packaging need measured device work. |
| Production room transport | ENet is the private prototype adapter. Account-bound admission, encryption requirements, public abuse resistance, and infrastructure integration need separate designs and evidence. |
| Custom server backend | The existing Go/Rust/control-plane research remains available. This GDScript authority does not establish a final multi-region server architecture. |
| Persistence | An optional validated activity save stores round progress. Accounts, durable player state, economies, and historical recovery are separate concerns. |
| Content scope | Golf and treehouse scenery establish landmarks. They do not imply complete golf, climbing, creation, or minigame systems. |
| Delivery | Build receipts identify inputs and outputs. Signed installation, updater trust, public release, and launcher integration remain separate release gates. |

## Why this experiment is useful

A shared simulation and data module keep early client/server behavior inspectable without maintaining two independent implementations. Separate visual and session modules allow the deterministic authority to run headlessly. Procedural authored assets make the experiment self-contained and keep imported content dependencies out of the first build.

Compatibility rendering gives this desktop experiment a modest graphical baseline. This is a design choice, not a hardware performance result. Linear tone mapping, restrained light energy, low specular intensity on matte surfaces, and limited fog influence on the sky were selected through rendered review; performance claims still require target-device measurements.

The implementation deliberately keeps the existing roadmap intact. A playable artifact can now reveal deficiencies in movement, layout, access, and synchronization while the more consequential platform decisions remain reviewable.

## Exit criteria before a production engine decision

1. Compare equivalent authored content and interactions in the candidate production paths, including actual VR device tracking and comfort requirements.
2. Record CPU/GPU frame timing, memory, loading, executable size, and power/thermal behavior on the intended minimum desktop and VR hardware.
3. Measure real session admission, movement, loss/jitter handling, disconnection, and recovery with traceable binaries and versioned protocols.
4. Review account-bound admission, transport privacy, authority boundaries, operational monitoring, and public hosting requirements.
5. Reassess creator workflows, content iteration, animation, accessibility, platform export work, engine licensing, and maintainability using the working prototypes.

A later ADR can adopt, replace, or retire this implementation. The current experiment should not close broad roadmap issues unless each issue's own acceptance criteria are satisfied.

## Evidence and references

- [Existing Metallic-RC architecture and engine comparison](ARCHITECTURE.md).
- [Pinned build helper](../scripts/build_native.py) and [export presets](../game/native/export_presets.cfg).
- [Shared simulation](../game/native/scripts/simulation.gd) and [session adapter](../game/native/scripts/session.gd).
- [Godot 4.5 dedicated-server export documentation](https://docs.godotengine.org/en/4.5/tutorials/export/exporting_for_dedicated_servers.html), including the `dedicated_server` feature and headless execution.
- [Godot 4.5 high-level multiplayer documentation](https://docs.godotengine.org/en/4.5/tutorials/networking/high_level_multiplayer.html).
- [Godot 4.5 Environment reference](https://docs.godotengine.org/en/4.5/classes/class_environment.html), including tone mapping and fog/sky controls.

Specific test counts, hardware results, screenshots, and export digests belong to a verification receipt for the tested source and artifacts. They are not assumed by this decision record.

# Metallic Meadow — original social hub and cinematic design

**Design revision:** 2026-10-18 | **Status:** specification + machine-validated navigation manifest only; **not a playable scene**.\
**Source of truth:** [hub manifest](../game/hub/metallic-meadow.v1.json), [validator](../game/hub/manifest.mjs) and [contract tests](../game/tests/contracts.test.mjs).\
**Related work:** existing [P2 original social hub #13](https://github.com/oncoursemail/Metallic-RC/issues/13).

## The core creative idea

**Metallic Meadow: The Commons** is an outdoor, warm, playful gathering world. It has an identifiable silhouette and intentional sightlines, not a collection of generic buildings.

The visitor enters at Arrival Square. Ahead stands a **tall central barn**, the social heart of the world. A nearby putting green and a longer looping mini-golf practice route sit to the east. An orchard and working farm garden are west of the square. A spectacular climbable treehouse with a branching walkway anchors the northwest. Maker Pavilion, Learning Grove and Companion Meadow form a connected arc to the south.

Use carefully authored woodland materials, warm sunlight and believable scale; machined metallic details and restrained plum accents reference Metallic's identity. The world should be bright and human-crafted, not a dark software-dashboard aesthetic translated into a game environment.

## Schematic layout, not measured level geometry

```text
                     [CENTRAL BARN / SOCIAL]
                 [ORCHARD]     |      [GOLF GREEN]
                      \       |       /       \
             [TREEHOUSE] — [ARRIVAL]      [COMPANION MEADOW]
                    \        /   \            /
                [LEARNING GROVE]  [MAKER PAVILION]
```

The eight area IDs and ten connecting paths are in `metallic-meadow.v1.json`. All public map destinations are reachable from spawn, verified by tests. **These are conceptual scene coordinates, not actual terrain, navmesh or collision**.

| Area | Player experience | First interactive milestone |
| --- | --- | --- |
| Arrival Square | Friends meet; onboarding, world news, accessibility, watch map | Join, spawn, open map, teleport request |
| Central Barn | Social seating, community boards, seasonal celebrations, creator showcases | Private/public venue logic + event portal |
| Meadow Golf Green | Putting court, optional course, friendly leaderboard | Server-validated putt + first-putt achievement |
| Orchard & Farm | Watering, harvesting, cooperative decoration | One synchronized interactable crop |
| Canopy Treehouse | Exploring, photo viewpoints, controlled traversal | Accessible alternative route and collision tests |
| Maker Pavilion | Creation demos, asset preview and sandboxed workshops | Spawn approved prefab in an authorized editing room |
| Learning Grove | Teacher-led courses and short AI-assisted exercises | Course registration and project rubric screen |
| Companion Meadow | Adopt/customize a pet and explore NPC behaviors | Scripted pet follow/wait/scared/comfort demo |

### Cinematic: *From Memory to Tomorrow*

The request is to acknowledge the older Rec Room center being destroyed and replaced by the new home. **Recreating Rec Room's exact copyrighted room, trademarks, music, props, textures or original cinematic would require appropriate rights**. Metallic's release-ready alternative is an **original fictional meeting hall / memory archive**, with an unmistakable emotional beat of change and a new world being born. If licensing is obtained, separately assess what can actually be used.

1. **Memory, 4 s:** a deliberately original former hall, seen through a quiet framed vignette with subtle ambient noise.
2. **Transformation, 3.5 s:** timber and metal abstract into light, leaves and floating components; gentle camera movement, no flashing destruction.
3. **Reveal, 6 s:** high wide composition of the outdoor barn, golf landscape, forest/treehouse silhouette and community paths.
4. **Welcome, 3 s:** dissolve to player-controlled arrival with the watch displaying "Explore the Commons".

The timeline is a **shot list, not rendered animation**. Cutscene is skippable, avoid replay on every join, offer **reduced-motion/no-cutscene mode**, and provide captions/audio-description options. VR should prefer fixed viewpoint or 2D theater panel rather than an involuntary sweeping camera. Do not silently move the player's head.

### Art & gameplay polish rules

- Create a **composition sheet**, floor plan, grayscale blockout, color script, material sheet, original prop and signage standards, lighting studies, soundscape, animation beat sheet, and day/night accessibility pass before final modeling.
- Blockout in meters and compare avatars, doors, rails, ramps and social seating with actual PC/VR poses. Prefer wide routes and smooth walkable gradients, collision on every rail and floor.
- Favor authored hero assets, trim sheets and modular pieces; reserve high-detail clusters for focal points. Integrate occlusion, mesh LOD, lightmap probes, pooled particles and at-distance animation throttling.
- Golf must have repeatable ball handling, consistent collision, scoring authority, grounded club haptics/input parity and a recovery/reset rule; don't assume local desktop physics accurately equals multiplayer physics.
- The farm and treehouse add emergent play only when networked interactions are verified. Do not present screenshot-only set dressing as finished gameplay.
- Express interaction affordances using original icons, label states, tactile feedback and PC/VR control cues. HUD-free cinematic framing should still preserve accessibility.
- Capture *actual* walkthrough video/screenshots and independent human visual critique. No stock environment kit dropped into a prefab village and called final.

### Objective acceptance gates

**H0 / schema:** the JSON loads, all IDs unique, all destinations connected, spawn valid, cutscene skippable (automated test implemented).\
**H1 / blockout:** editor scene with original greybox, locomotion, 8 accessible points of interest, functional watch map and correctly scaled collision (not implemented).\
**H2 / interactive slice:** two networked players spawn together; server-side scored golf shot, farm interaction, one persistent creator object and Roomie NPC (not implemented).\
**H3 / beauty + comfort:** art director approves the distinctive original composition; actual VR/desktop footage, options for 72/90 Hz targets and motion comfort pass on named devices; capture p95 frames, GPU and RAM rather than assuming success (not implemented).\
**H4 / release:** region latency, reconnect, join/no-autoplay, voice mute/report, content safety, rollback and launch evidence (not implemented).

**Frame budgets are engineering hypotheses:** 72 Hz = 13.89 ms per frame; 90 Hz = 11.11 ms. Reserve a realistic margin for compositor/OS and vary art density by platform. Never market these as measured numbers before profiling.

### Design review checklist

The hub should be recognizable in silhouette from a single original screenshot. Test first-time comprehension of spawn, barn, golf, workshop, classes and pets with volunteers. Ask whether visitors can find the golf green without a tutorial, whether a new creator recognizes Maker Pavilion, whether VR players tolerate the cinematic, and whether users can reach the treehouse without feeling physically unsafe. Record video and quantitative outcomes, not only preferences.

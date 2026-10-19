# Metallic-RC — Expanded Capability Catalog

**Status:** proposals, not implemented. **Research baseline:** October 15, 2026.

The catalog describes 240 candidate features across 20 areas. It does not inflate [live GitHub issue completion](PROGRESS.md): only features converted into acceptance-tested `[P0]–[P6]` issues become tracked scope. Groups are ordered for readability, not release sequence.

**Priority tier:** **Foundation** = priority within this area, **Expansion** = after foundational dependencies, **Advanced** = longer-term candidate. Phasing and technical risk still govern actual build order. **Proof** is a suggested acceptance test, not a claim of passing status.

## Core gameplay and platform

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `CORE-01` | Independent username and login | Foundation | New accounts can sign in and recover access without Steam running |
| `CORE-02` | Fast room join | Foundation | Measure cached room join p95 by region |
| `CORE-03` | Reconnect to interrupted session | Foundation | Restore user to safe checkpoint after network interruption |
| `CORE-04` | Crossplay room admission | Foundation | Two supported device classes can join same room |
| `CORE-05` | Favorites and recently played | Expansion | Room discovery restores recent/favorite selections |
| `CORE-06` | Matchmaking skill and party settings | Expansion | Rules can be changed without client patch |
| `CORE-07` | Quick invite/party links | Expansion | Expired or revoked invitations fail closed |
| `CORE-08` | Personal spawn settings | Expansion | Comfort, handedness and display choices persist |
| `CORE-09` | Private/instance rooms | Advanced | Access-control matrix passes tests |
| `CORE-10` | Spectate and replay | Advanced | Recorded synthetic match can be replayed without desync |
| `CORE-11` | Room seasons and version channels | Advanced | Creator rolls version forward/back without breaking invites |
| `CORE-12` | Original encounter/activity framework | Advanced | Reusable quest, score and checkpoint templates pass a demo |

## VR comfort and embodiment

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `VR-01` | Head and both hands tracked with IK | Foundation | Two headsets replicate avatar pose without jitter spikes |
| `VR-02` | Snap/smooth turn and teleport | Foundation | Comfort menu settings apply instantly |
| `VR-03` | Seated/standing height calibration | Foundation | Height offsets remain consistent across sessions |
| `VR-04` | Controller remapping and left-handed mode | Foundation | Common actions available on alternate controllers |
| `VR-05` | Gesture interaction state | Expansion | Gestures replicate as low-bandwidth events |
| `VR-06` | Direct grab and toss of props | Expansion | Server authorizes collisions and release physics |
| `VR-07` | Per-hand tool selection | Expansion | Tool state and object ownership sync reliably |
| `VR-08` | Spatial voice attenuation | Expansion | Distance and privacy settings audibly tested |
| `VR-09` | Hand tracking fallback to controllers | Advanced | No soft lock when tracking becomes unavailable |
| `VR-10` | Full-body optional tracker calibration | Advanced | Unsupported devices degrade gracefully |
| `VR-11` | Haptics vocabulary | Advanced | Interactions use bounded configurable haptic feedback |
| `VR-12` | Mirror and photo booth | Advanced | Avatar render and camera privacy controls work on supported platforms |

## Desktop, Linux and mobile clients

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `DEVICE-01` | Keyboard/mouse control parity | Foundation | All critical actions usable without VR controllers |
| `DEVICE-02` | Controller support and remapping | Foundation | Gamepad testers can complete original quest |
| `DEVICE-03` | Graphics quality profiles | Foundation | Low profile reduces CPU/GPU time in measured scene |
| `DEVICE-04` | Linux Vulkan native prototype | Foundation | Release only with tested device/driver combinations |
| `DEVICE-05` | Touch controls and safe touch zones | Expansion | Mobile users can interact while walking |
| `DEVICE-06` | Adaptive mobile layout | Expansion | UI matches phone/tablet safe areas with no clipping |
| `DEVICE-07` | Device capability detection | Expansion | Unsupported features hidden with explanation |
| `DEVICE-08` | Offline launcher repair | Expansion | Installed manifest verifies/repairs cached packages |
| `DEVICE-09` | Resume interrupted downloads | Advanced | Large files recover without corruption |
| `DEVICE-10` | Low-bandwidth texture streaming | Advanced | Download and play metrics quantified |
| `DEVICE-11` | Battery/thermal-aware mobile renderer | Advanced | No uncontrolled thermal runaway in long test |
| `DEVICE-12` | Account/session continuity across devices | Advanced | Settings and ownership consistent under device switch |

## Avatar customization and expression

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `AVATAR-01` | Original avatar rig and facial expressions | Foundation | All supported animations are reproducible in build |
| `AVATAR-02` | Custom outfits and saved presets | Foundation | Loadouts can be created and restored |
| `AVATAR-03` | Approved accessory publishing | Foundation | Content review before item visible in catalog |
| `AVATAR-04` | Skin tone and body variants | Foundation | Avatar quality tests on supported rig sizes |
| `AVATAR-05` | Animation emotes and gesture wheel | Expansion | Emotes visible to nearby players and safely interruptible |
| `AVATAR-06` | Accessory fit auto-validator | Expansion | No clipping threshold exceptions without creator warning |
| `AVATAR-07` | Dynamic costume recolor zones | Expansion | Material channels stay within performance budgets |
| `AVATAR-08` | Simple face/voice privacy presets | Expansion | Anonymous/private rooms do not expose hidden attributes |
| `AVATAR-09` | Avatar thumbnail and turntable | Advanced | Server generated preview matches in-game avatar |
| `AVATAR-10` | Creator-made facial animation sets | Advanced | Graph budgets/asset format validated |
| `AVATAR-11` | Cosmetic gifting | Advanced | Inventory ledger is atomic and abuse-resistant |
| `AVATAR-12` | Avatar export preview | Advanced | Supported user's local legacy export can be previewed with missing-asset warnings |

## Friends and community

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `SOCIAL-01` | Friends, blocks, follow and invite | Foundation | Mutual and non-mutual permissions tested |
| `SOCIAL-02` | Parties with leader handoff | Foundation | Leader disconnect does not disband joined members unexpectedly |
| `SOCIAL-03` | Profile cards and configurable visibility | Foundation | Blocked users cannot view restricted fields |
| `SOCIAL-04` | Clubs and event calendars | Foundation | Club roles and RSVP permissions tested |
| `SOCIAL-05` | Cross-room social invites | Expansion | No accidental invite to private restricted room |
| `SOCIAL-06` | Room messaging and announcements | Expansion | User opt-in, mute and moderation controls |
| `SOCIAL-07` | Shared watch/party activities | Expansion | Playback synchronization handles user desync |
| `SOCIAL-08` | Event scheduling and reminders | Expansion | Event times account for attendee locale |
| `SOCIAL-09` | Community voting on featured rooms | Advanced | Anti-manipulation and quality controls tested |
| `SOCIAL-10` | Original in-game photo album | Advanced | Image moderation and sharing permissions enforced |
| `SOCIAL-11` | Streamer privacy mode | Advanced | Hides sensitive room names, invites and join codes |
| `SOCIAL-12` | Verified creator programs | Advanced | Verified status tied to evidence and revocable |

## Maker Pen and room editing

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `MAKER-01` | Object creation/transforms | Foundation | Precise translate/rotate/scale works on VR and desktop |
| `MAKER-02` | Snap, grid, align/distribute | Foundation | Manipulation produces stable saved transforms |
| `MAKER-03` | Unlimited undo/redo with safe scope | Foundation | Edits can be reversed through last saved sequence within quotas |
| `MAKER-04` | Prefab and invention library | Foundation | Shared saved object bundles load with IDs intact |
| `MAKER-05` | Multiselect hierarchy and grouping | Expansion | Grouped edits preserve relative transforms |
| `MAKER-06` | Per-room resource budgets | Expansion | Editor reports draw, CPU, physics and network costs |
| `MAKER-07` | Versioned room saving | Expansion | Creator recovers last working version after interruption |
| `MAKER-08` | Owner/collaborator edit permissions | Expansion | Unauthorized edits rejected server side |
| `MAKER-09` | Concurrent multi-editor locks/merges | Advanced | Conflicting edits resolved visibly without data loss |
| `MAKER-10` | Custom mesh/texture/animation import | Advanced | Validators reject unsafe or oversized assets |
| `MAKER-11` | Visual terrain and water tools | Advanced | Asset and lighting costs profiled on target devices |
| `MAKER-12` | Offline draft editor | Advanced | Local drafts import with version reconciliation |

## Circuits V3 / node scripting

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `CV3-01` | Typed event, value and execution pins | Foundation | Compiler rejects invalid connections and reports locations |
| `CV3-02` | Variables, math and logic nodes | Foundation | Graph evaluation deterministic for reference fixtures |
| `CV3-03` | Timers/triggers/input interactions | Foundation | Door timer example runs for all players |
| `CV3-04` | Graph debugger and breakpoints | Foundation | Pause and inspect state on controlled test scene |
| `CV3-05` | Networked player/room properties | Expansion | Replicated state has owner and validation policy |
| `CV3-06` | State machines and flow branches | Expansion | Transitions replay deterministically from test input |
| `CV3-07` | Quotas for tick/event/memory usage | Expansion | Malicious loops halted without crashing session |
| `CV3-08` | Persistent game state API | Expansion | Versioned save/load with limits and permissions |
| `CV3-09` | Physics/animation/audio adapters | Advanced | Scripts invoke only approved engine capabilities |
| `CV3-10` | Reusable modules and templates | Advanced | Pinned dependencies avoid breaking published rooms |
| `CV3-11` | Graph profiler with hot-path warnings | Advanced | Overbudget node visually identified |
| `CV3-12` | Versioned bytecode migration and tests | Advanced | Older published graphs migrate or fail clearly |

## Creator desktop studio and asset workflow

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `STUDIO-01` | Professional hierarchy/inspector UI | Foundation | Creator edits nested scene without overlapping labels |
| `STUDIO-02` | Material and PBR property editor | Foundation | Preview matches game rendering profile |
| `STUDIO-03` | Scene asset references and dependency graph | Foundation | Missing assets surfaced before publish |
| `STUDIO-04` | Animation timeline editor | Foundation | Replay reproducible with consistent rig |
| `STUDIO-05` | Audio import and spatial emitter editor | Expansion | Verified sample rates/codec limits |
| `STUDIO-06` | Creator source-control-like revisions | Expansion | Diff and rollback shown before publish |
| `STUDIO-07` | Collaborative editing session viewer | Expansion | Invite/role and conflict resolution tested |
| `STUDIO-08` | Prefab package publishing | Expansion | Package validation, attribution and version fields complete |
| `STUDIO-09` | Creator sandbox playtest client | Advanced | Start private session from editor without public deploy |
| `STUDIO-10` | Automated scene lint/repair suggestions | Advanced | Diagnostics are actionable and false positives tracked |
| `STUDIO-11` | Batch asset converter and import log | Advanced | Preserves IDs and reports unknown fields |
| `STUDIO-12` | Editor plugin API | Advanced | Plugins sandbox permissions and declare capabilities |

## Quests, RRO-style originals and live events

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `GAMES-01` | Social lobby and matchmaking hub | Foundation | Players join, interact, invite and leave reliably |
| `GAMES-02` | Cooperative quest starter kit | Foundation | Checkpoint, enemy event, scoring and rewards demo |
| `GAMES-03` | Capture-the-flag template | Foundation | Scoring and reset state consistent under disconnect |
| `GAMES-04` | Laser-like arena mode | Foundation | Original assets and server hit validation documented |
| `GAMES-05` | Parkour/obby creator kit | Expansion | Respawn and checkpoint graph verified |
| `GAMES-06` | Team PvP match rules | Expansion | Team assignment and results protected server side |
| `GAMES-07` | Time-trial racing tools | Expansion | Deterministic laps and leaderboards under rollback |
| `GAMES-08` | Seasonal events and calendars | Expansion | Time-zone safe scheduling and content rollbacks |
| `GAMES-09` | Procedural optional challenge rooms | Advanced | Seed produces reproducible environment and cap |
| `GAMES-10` | NPC encounter/companion system | Advanced | AI CPU budgets enforced per room |
| `GAMES-11` | Achievements and original quest rewards | Advanced | Entitlements granted exactly once |
| `GAMES-12` | Community tournament organizer | Advanced | Registration, brackets and anti-disconnect rules tested |

## Creator economy and free features

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `ECON-01` | Free access to core maker tools | Foundation | Users can build/publish without subscription feature wall |
| `ECON-02` | Auditable virtual token ledger | Foundation | Double entry and idempotency tests pass |
| `ECON-03` | Approved item/room marketplace | Foundation | Unreviewed content invisible to general users |
| `ECON-04` | Creator price and storefront controls | Foundation | Entitlements match price and published revision |
| `ECON-05` | Purchase refunds and disputes | Expansion | Duplicate payments cannot double credit |
| `ECON-06` | Creator dashboards and sales attribution | Expansion | Aggregated analytics match ledger records |
| `ECON-07` | Cosmetic and avatar item stores | Expansion | Availability and entitlement links tested |
| `ECON-08` | Game/experience purchase offers | Expansion | Receipt and access rollback supported |
| `ECON-09` | Creator collaboration revenue splits | Advanced | Revenue allocations deterministic for synthetic records |
| `ECON-10` | Discounts, bundles and promotional events | Advanced | Stacking and expiry constraints tested |
| `ECON-11` | Optional premium utility membership | Advanced | Core creative functions stay available to non-members |
| `ECON-12` | Fraud detection and transaction holds | Advanced | Suspicious automated abuse enters review queue |

## Moderation, privacy and accessibility

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `SAFETY-01` | Text blocking/muting and user reporting | Foundation | No user can bypass a block with room invites |
| `SAFETY-02` | Room labels and age-aware discovery | Foundation | Unapproved content omitted from restricted profiles |
| `SAFETY-03` | Voice controls and report entrypoint | Foundation | User can mute, report and manage permitted captures |
| `SAFETY-04` | Role-based admin and moderator permissions | Foundation | Least-privilege matrix validated in API tests |
| `SAFETY-05` | Case management and appeals | Expansion | Every case has evidence receipt and audit timeline |
| `SAFETY-06` | Content review for meshes/text/audio | Expansion | Rejected uploads cannot be listed or served |
| `SAFETY-07` | Moderation incident dashboard | Expansion | Moderators see actionable trends without exposing secrets |
| `SAFETY-08` | Rate limits for invites/rooms/auth | Expansion | Synthetic spam blocked without harming baseline traffic |
| `SAFETY-09` | Accessible captions and UI scale | Advanced | Support keyboard navigation and readable labels |
| `SAFETY-10` | Comfort/epilepsy graphics settings | Advanced | Reduced flashing/motion respected in each supported client |
| `SAFETY-11` | Localization and bidi layout testing | Advanced | No clipped/overlapping controls in long translations |
| `SAFETY-12` | Privacy settings export/delete | Advanced | Data handling and toggles tested end-to-end |

## Web, admin, server engineering and federation

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `OPS-01` | Public website with signed downloads | Foundation | Checksums and signing metadata match releases |
| `OPS-02` | Website live service status page | Foundation | Synthetic monitors report age/staleness of samples |
| `OPS-03` | Server performance dashboard | Foundation | p50/p95 latency, CPU, memory and room usage from real metrics |
| `OPS-04` | GitHub-linked roadmap progress page | Foundation | Site displays verified issues-based progress with update time |
| `OPS-05` | Private owner operations tools | Expansion | Strong auth and scoped audit logs protect destructive actions |
| `OPS-06` | CDN and asset integrity checks | Expansion | Invalid artifact hashes rejected before loading |
| `OPS-07` | Protected game UDP ingress design | Expansion | Upstream provider security assumptions documented and tested |
| `OPS-08` | Backup/restore and operational runbooks | Expansion | Restore tested on a separate machine |
| `OPS-09` | Discord bot and opt-in Rich Presence | Advanced | Immediate privacy opt-out hides game/room details |
| `OPS-10` | Opt-in partner manifest and player counts | Advanced | Signed partner counts include timestamp and provenance |
| `OPS-11` | Secure cross-project launcher deep links | Advanced | Untrusted partner binary cannot bypass manifest checks |
| `OPS-12` | Versioned partner capability API | Advanced | Project incompatibility shown rather than silent failure |

## Differentiating longer-term innovations

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `ADV-01` | Creator replayable bug reports | Foundation | Synthetic replay of circuit and physics bug matches original |
| `ADV-02` | VR native graphical profiler | Foundation | Graphs and performance hints visible in creator room |
| `ADV-03` | Federated trust health score | Foundation | Visibility based only on opt-in timestamped evidence |
| `ADV-04` | Deterministic circuit unit testing | Foundation | Graph creators run tests before publish |
| `ADV-05` | Creator room accessibility auditor | Expansion | Warns about contrast, reachability and comfort risks |
| `ADV-06` | Live performance autotuning | Expansion | Server suggests scene budget changes not destructive patches |
| `ADV-07` | Cross-room original portal experiences | Expansion | Permissions/versions prevent unauthorized travel |
| `ADV-08` | Room-specific audio privacy zones | Expansion | Voice visibility configurable within supported spaces |
| `ADV-09` | Collaborative event-based original quests | Advanced | User-created quest state sync with rollback |
| `ADV-10` | Local-first creator draft merges | Advanced | Offline creation merge produces explicit conflict previews |
| `ADV-11` | Automatic low-end asset variant build | Advanced | LOD and texture tiers produced from validated assets |
| `ADV-12` | Community quest pack creator toolkit | Advanced | Composable objectives produce repeatable missions |

## In-game staff roles, identity and moderation

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `STAFF-01` | Grant/revoke Volunteer Moderator | Foundation | Only authorized staff can grant temporary or indefinite VMod scopes |
| `STAFF-02` | Grant/revoke Developer role | Foundation | Developer powers are separately scoped from moderation |
| `STAFF-03` | Volunteer Mod overhead title toggle | Foundation | Authoritative title display saved and replicated; hidden mode works |
| `STAFF-04` | Developer overhead title toggle | Foundation | No player can spoof title or unlock developer controls |
| `STAFF-05` | Global audit receipt for every sanction | Expansion | In-game, web and bot routes share one durable log |
| `STAFF-06` | VR wrist-mounted staff tools | Expansion | Legible panels with controller and hand fallback |
| `STAFF-07` | Room moderator role separation | Expansion | Creator local powers cannot cause global sanctions |
| `STAFF-08` | Appeals workflow with second reviewer | Expansion | Appeal reversals preserve evidence chain |
| `STAFF-09` | Staff roster and training status | Advanced | Inactive/expired role cannot use privileged command |
| `STAFF-10` | Case severity and priority triage | Advanced | Queue sort/paging reproducible with same results |
| `STAFF-11` | Player teleport/spectate restrictions | Advanced | Every moderation travel has scope and reason |
| `STAFF-12` | Staff impersonation/report prevention | Advanced | Server-issued verification prevents fake badge claims |

## Discord application and community operations

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `BOT-01` | Activity displays real humans online | Foundation | Authoritative production heartbeat excludes testbots |
| `BOT-02` | Slash help and new-user guide | Foundation | Command categories and privacy scope clearly explained |
| `BOT-03` | Real production status slash command | Foundation | Stale production data shown as unavailable |
| `BOT-04` | Room catalog search slash command | Foundation | Filtered public results never reveal private rooms |
| `BOT-05` | Event announcement and scheduling | Expansion | Rate limited updates and opt-in subscriber scope |
| `BOT-06` | Discord account linking portal | Expansion | OAuth with revocable consent and short-lived code |
| `BOT-07` | Moderator report queue access | Expansion | Scoped role check in Metallic backend, never Discord role alone |
| `BOT-08` | Bot command receipts and rate limits | Expansion | Every privileged request audited with idempotent request ID |
| `BOT-09` | Creator documentation and event command | Advanced | Published resources verified as accessible to members |
| `BOT-10` | Staff online incident alert subscription | Advanced | Only meaningful incidents posted with no sensitive case data |
| `BOT-11` | Discord user Rich Presence privacy controls | Advanced | Hidden room/private status never leaks game or join token |
| `BOT-12` | Maintenance status and alert fallback | Advanced | Bot gracefully degrades during backend or Discord outage |

## AI bot playtesting and stress engineering

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `LAB-01` | Local synthetic room fixture | Foundation | Seeded mock clients join/move/leave and report latency |
| `LAB-02` | Protocol-realistic headless players | Foundation | Bots use the actual game wire protocol after it exists |
| `LAB-03` | Deterministic scripted quest bots | Foundation | Failure replays from captured seed and server build |
| `LAB-04` | Optional trained Unity ML-Agents explorers | Foundation | Policy discovers bugs and records reproducible traces |
| `LAB-05` | Locust login+room browse benchmark | Expansion | Concurrent scoped users and p95 error measurements |
| `LAB-06` | k6 WebSocket rooms benchmark | Expansion | Checks connection recovery and durable event delivery |
| `LAB-07` | Staging-only load orchestrator | Expansion | Public servers cannot be selected without explicit access |
| `LAB-08` | Chat and proximity voice media load test | Expansion | Synthetic audio packets stress SFU independently |
| `LAB-09` | Creator CV3 graph stress scenes | Advanced | Quotas stop runaway graph nodes without server crash |
| `LAB-10` | Regional fleet scale and join saturation tests | Advanced | Session capacity and autoscaler evidence published |
| `LAB-11` | Failure injection and recoverability sweeps | Advanced | Cache/network/database outage budgets measured |
| `LAB-12` | Quest/PC rendered 16-64 avatar benchmark | Advanced | GPU/CPU frame time, memory and LOD result recorded |

## Original visual design and real UX testing

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `LOOK-01` | Obsidian Alloy token system | Foundation | Color/typography/spacing tokens and usage implemented consistently |
| `LOOK-02` | Handcrafted launcher visual review | Foundation | No stock mock assets or fake download CTA |
| `LOOK-03` | VR room-browser legibility audit | Foundation | Text and controls pass headset distance check |
| `LOOK-04` | Desktop admin case triage workspace | Foundation | Data table, command confirmation and keyboard focus tested |
| `LOOK-05` | Branded GitHub live progress SVG | Expansion | SVG must parse and mirror exact GitHub issue closure count |
| `LOOK-06` | Live status website connected to GitHub | Expansion | Website displays sanitized JSON, no private issue content |
| `LOOK-07` | Visual regression multi-browser suite | Expansion | Screenshots catch overlap, truncation and layout regression |
| `LOOK-08` | Animated UI reduced-motion support | Expansion | All decorative movement fully respects comfort option |
| `LOOK-09` | Responsive mobile viewport stress | Advanced | 320/375/768/1440px plus long labels/200% zoom |
| `LOOK-10` | Original brand motion studies | Advanced | Purposeful mark reveal without loop/glow/slop |
| `LOOK-11` | Screen-level quality reviews before merge | Advanced | Reviewer validates reference, hierarchy and hit targets |
| `LOOK-12` | Real download progress and repair animations | Advanced | Displayed percent based on bytes, never invented telemetry |

## High-end VFX, avatars, animation and graphics

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `VFX2-01` | Particle pooling and emission budgets | Foundation | GPU traces show bounded emitters and no huge frame spikes |
| `VFX2-02` | Metal material and reflection consistency | Foundation | Materials verified across desktop and Quest tiers |
| `VFX2-03` | Quest VFX tiering and foveated options | Foundation | Maintains required headset refresh under representative scene |
| `VFX2-04` | Avatar head/hand IK stress under 32 avatars | Foundation | Replication and CPU animation cost measured |
| `VFX2-05` | Emote wheel and cancellation blending | Expansion | No stuck rigs, hand overlap or jitter after emote cancel |
| `VFX2-06` | Surface-specific footsteps and haptic events | Expansion | Audio/haptic variants tested across target surfaces |
| `VFX2-07` | Creator effect node authoring and previews | Expansion | Node-limit and volume safeguards during maker editing |
| `VFX2-08` | Quest/boss fight telegraphing with comfort | Expansion | Effects remain readable and photosensitivity-aware |
| `VFX2-09` | Celebration VFX reward reveal | Advanced | Pool remains responsive under simultaneous reward events |
| `VFX2-10` | Physics hit reactions and props | Advanced | Fixed step preserves authoritative contacts and interactions |
| `VFX2-11` | Spatial audio occlusion and mix snapshots | Advanced | Client and media streams remain audible and configurable |
| `VFX2-12` | Frame-safe shadows, fog and scene transitions | Advanced | Profiling shows acceptable frame-time on low tier |

## Store publishing, platform distribution and build quality

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `META-01` | Meta Horizon developer application setup | Foundation | App ID and team verified via owner dashboard |
| `META-02` | Signed ARM64 IL2CPP APK pipeline | Foundation | Manifest and signature validated on actual Quest |
| `META-03` | VRC automated packaging checks | Foundation | Missing required manifest fields fail build CI |
| `META-04` | Release channels alpha and beta | Foundation | Only approved testers receive install access |
| `META-05` | Quest gameplay comfort and input checks | Expansion | Device matrix passes controller disconnect and teleport modes |
| `META-06` | Meta technical review evidence bundle | Expansion | Real build video and measured frame time attached |
| `META-07` | Windows launcher signed release channel | Expansion | Manifest hash and interrupted update recovery proven |
| `META-08` | Linux native package proof | Expansion | Input, renderer and dependency test matrix passes |
| `META-09` | Android/iOS UI and performance preview | Advanced | Touch safe areas and thermal soak measured |
| `META-10` | Build asset delta patcher | Advanced | Bandwidth saving quantified vs full download and rollback safe |
| `META-11` | Website downloads only for released artifacts | Advanced | No download button unless artifact URL and signatures valid |
| `META-12` | Meta store approval and rollout confirmation | Advanced | Verified public listing before announcing publication |

## Cloud and admin operations lab

| ID | Proposed feature | Priority | Proof required |
| --- | --- | --- | --- |
| `OPS2-01` | In-game room health overlay for owner | Foundation | Shows tick/packet and capacity from authorized metrics |
| `OPS2-02` | Web operations protected identity | Foundation | Admin MFA/passkeys and no privileged JS secret |
| `OPS2-03` | Server role-based resource controls | Foundation | Operators can drain test rooms with audit and rollback |
| `OPS2-04` | Private staff escalation pipeline | Foundation | Critical incident receipt per web/game/Discord initiation |
| `OPS2-05` | Room live statistics via authoritative session | Expansion | Public counts dedupe session/user and exclude test accounts |
| `OPS2-06` | Per-region CPU/memory/egress dashboards | Expansion | Charts have timestamp, source, units and staleness |
| `OPS2-07` | Dedicated UDP mitigation provider review | Expansion | Upstream protections for real-time ports validated |
| `OPS2-08` | Load testing cost ceilings and kill switch | Expansion | Job auto-terminates at time, bandwidth or error budget |
| `OPS2-09` | Database backup and restore drill | Advanced | Full staging restoration within measured recovery target |
| `OPS2-10` | Alert routing and owner on-call dashboard | Advanced | False alerts minimized with correlation and clear owners |
| `OPS2-11` | Published status history with incident notes | Advanced | Only sanitized approved metrics visible publicly |
| `OPS2-12` | Fleet soak+autoscaling reliability gates | Advanced | Capacity and recovery passes before beta announcement |

## Rules for promoting a catalog idea into development

1. Link the exact user scenario and success metric; specify blockers and target devices.
2. Create a scoped GitHub issue with prefix `[P0]`–`[P6]`, named owner, dependencies, and repeatable tests.
3. Identify privacy/safety, networking and content-creator performance budgets as applicable.
4. Build a reproducible test/demo. Close only with passing CI and approved acceptance evidence.
5. Release-gate the feature separately. Show unsupported/unknown claims explicitly in public feature matrix.

[Roadmap](ROADMAP.md) · [Progress](PROGRESS.md) · [Research](RESEARCH.md) · [Architecture](ARCHITECTURE.md)

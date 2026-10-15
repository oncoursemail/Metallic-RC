# Metallic Studios — Expanded Capability Catalog

**Status:** proposals, not implemented. **Research baseline:** October 15, 2026.

The catalog describes 156 candidate features across 13 areas. It does not inflate [live GitHub issue completion](PROGRESS.md): only features converted into acceptance-tested `[P0]–[P6]` issues become tracked scope. Groups are ordered for readability, not release sequence.

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

## Rules for promoting a catalog idea into development

1. Link the exact user scenario and success metric; specify blockers and target devices.
2. Create a scoped GitHub issue with prefix `[P0]`–`[P6]`, named owner, dependencies, and repeatable tests.
3. Identify privacy/safety, networking and content-creator performance budgets as applicable.
4. Build a reproducible test/demo. Close only with passing CI and approved acceptance evidence.
5. Release-gate the feature separately. Show unsupported/unknown claims explicitly in public feature matrix.

[Roadmap](ROADMAP.md) · [Progress](PROGRESS.md) · [Research](RESEARCH.md) · [Architecture](ARCHITECTURE.md)

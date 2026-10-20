> **Direction update — 20 October 2026:** use the owner's actual compiled client as the starting point. The [compiled-client rework](COMPILED_CLIENT_REWORK.md) supersedes earlier replacement-engine/hub proposals below. These plans do not establish shipped gameplay.

# Metallic-RC — World, Learning, Creator Economy, AI & Engine Research
**Status:** Proposed architecture and research, not shipped functionality. **Research date:** 2026-10-18. **Owner:** Metallic.
**Scope:** Outdoor hub, cinematic transition, community teachers, rigorous courses, personalized AI tutors, menus/watch, marketplace, NPCs/pets, support, and engine strategy.

## 1. Product north star
Metallic is a social multiplayer game and creator platform. Its distinguishing user journey: **join → explore → learn → create → test → share → play with friends → improve**. A player should experience the same creation system whether learning, publishing a prefab, building a quest, or programming an AI NPC.

**Non-goals for first release:** building a general-purpose engine from scratch; unsupervised humanlike AI with unlimited tools; launching a financial marketplace before moderation/rights are ready; promising college accreditation; reproducing copyrighted Rec Room level geometry or branding.

## 2. New recreation center: The Metallic Grounds
**Direction:** A warm, stylized, outdoor rural social campus with an iconic central barn; independent and recognizable, *not* a copy of Rec Room.
- **Arrival plaza:** spawn, social board, room portal grove, daily events and quest board; accessible default routes and clear sightlines.
- **Central barn:** primary social landmark with welcome desk, fireplace, creator exhibits, classroom loft, stage, makerspace, and game portals.
- **Golf zone:** separate putting green, mini-golf and target challenges; physics-authoritative balls; optional larger course later. “Golf court” should be treated as a flexible practice/target range until level design clarifies play rules.
- **Treehouse:** hangout, observation deck, small cooperative exploration activities, accessible alternate route/teleport.
- **Farm/gardens:** planting/building demos, ambient animals, seasonal decorations, player-driven teaching activities; scalable without simulating a full farm.
- **Workshop courtyard:** scheduled classes and demo booths.
- **Navigation:** loops between zones, distinct silhouettes, world map and watch quick-travel.
**Cinematic concept, subject to IP review:** “Goodbye to an Era”: original stylized indoor recreation room (not copied assets) gracefully transforms/dissolves into particles and architectural fragments; a door opens onto sunlight and the new Grounds. Include skip/replay, non-flashing/accessibility cut, subtitle/caption track, VR comfort camera (no forced camera acceleration), reduced-motion mode, and seamless spawn fallback.
**Streaming/LOD:** origin cell + surrounding streaming cells, HLOD/impostors, occlusion, baked/static lighting where sensible, cheap ambient interactions, pool VFX, deterministic load order. Verify PC and standalone VR independently.

**First-world vertical slice acceptance:** two players can spawn, walk barn→treehouse→practice green, interact with a golf ball and barn door, see consistent world state, and reconnect; cinematic is skippable and has reduced-motion option; profile CPU/GPU/network capture is attached to issue.

## 3. Creator Team / teachers / workshops
Roles are **Creator, Verified Instructor, Workshop Host, Moderator**; roles are additive but permissions must never be inferred from visible badges. Instructor verification includes identity/portfolio review, safeguarding training appropriate to age audience, moderation policy acceptance, and revocable privileges.
- Event proposal → staff review → scheduled listing → RSVP/capacity → event session → moderated chat/voice → attendance and feedback → archived resources.
- In-world classroom supports whiteboard, screen/media permission sharing, collaborative objects, lesson versioning, and safe pause/mute/report controls.
- Creator cohort: featured projects, peer review, office hours, bounties/commissions only when legal/payments constraints resolved.
- Quality gates: teacher rubric, transparent learner feedback, appeals, attendance integrity, child-safety policy and no student-teacher off-platform contact by default.

## 4. Metallic Academy: serious learning
Tracks: C# / game logic, GDScript or chosen engine scripting, visual logic/circuits, mathematics for 3D, physics, digital art, animation, VFX, 3D modeling, sound design, UI/UX, networking, cybersecurity foundations, AI systems, scientific simulation and entrepreneurship.
- Each course has prerequisites, explicit competencies, source/reference pack, worked examples, practice spaces, capstone, graded rubric, revision, and optional instructor assessment.
- Provide **college-level challenge** without claims of accreditation or equivalence to university credit.
- AI tutor uses grounded course materials, cites lesson sections, adapts hints, asks students to explain reasoning, and switches to direct solutions for accessibility or user-requested support. Never invent credentials, grades or references.
- Learning model: diagnostic → demonstration → guided practice → retrieval checks → independent build → feedback → spaced reinforcement. Research suggests retrieval practice improves learning while points/progress UI alone does not demonstrate recall gains.
- Automated assessment: deterministic coding tests, graph/circuit simulator tests, scene validation and plagiarism-aware human review. For creative assignments, human rubric governs awards; AI is advisory.
- Persistent learner portfolio: versions, evidence, reflection, skill map; export data; privacy defaults favor minimal collection.
**Academy MVP:** one 45–90 minute course “Build a Golf Scoring Circuit” with 3 exercises, automated checks, 1 human-review rubric, no paid credential claims.

## 5. Custom menus and creator watch
UI surfaces: pause/home, profile, parties/friends, room explorer, discoveries, Academy, inventory, creator toolbox, events, support and settings. Watch has radial actions + optional pinned shortcuts, user-custom order, context-aware commands, notifications with quiet mode, and personal visual map pins/buttons.
- Single design token system for 2D launcher, desktop in-game, VR wrist screen and website; distinctive barn/grounds visual identity but legibility first.
- Input parity: controllers, keyboard/mouse, gamepad, VR pointers, one-handed alternatives; accessible contrast, font sizing, captions, reduced motion and comfort settings.
- Visual map APIs: creator-defined actions must be declarative and permission-scoped; URL/action allowlists, no arbitrary scripts invoked from shared maps, safe teleport destinations validated by server.
- State persistence: versioned preferences per account/device; rollback and migration tests.
**Watch MVP:** open, navigate 5 main functions, pin room portal, trigger map marker action, return to gameplay within 2 interactions after opening.

## 6. Invention store → Creator Exchange
Canonical marketplace taxonomy with multiple tags, not a single mutually exclusive folder:
- Buildings & architectural kits; environment & foliage; props/furniture; characters/avatars/pets; animation/emotes; VFX/particles/shaders; audio/music; circuits/logic; UI & interactive menus; gameplay templates; courses/educational kits; accessibility tools; materials/textures.
- Metadata schema: immutable asset ID + version, creator ID, license, dependencies, compatible runtime/engine versions, format, file size and poly/texture budgets, device capability tier, tags, description, screenshots/video, localization, content/age rating, change log, moderation status.
- UX: faceted filtering, compatibility flags, verified creator trust signals, asset previews in sandbox, collections, featured editorial curation, saved searches, version pinning, dependency resolver, rollback and reporting.
- Publishing stages: draft → lint/asset scanner → malware/script/rights checks → human review for high-risk items → published → version update / takedown / appeal. All user generated content is untrusted.
- Search ranking balances relevance, quality, real adoption and freshness; avoid pay-to-win rankings and review manipulation. Explain sponsored listings if ever introduced.
- Revenue/payments and creator payouts remain separate, gated workstreams; start with **free sharing**.
**Exchange MVP:** free circuit and props upload, tags, search, preview, install/uninstall, report, versioned re-publish, safe moderation and license declaration.

## 7. Humanlike AI characters and “Rommie”
Separate **agent simulation** from **language generation**. Characters do not actually need real feelings to behave convincingly: model observable internal **fictional emotion state** (e.g. fear/trust/curiosity), memory scopes, needs, goals, perception and animations; clearly signal they are AI.
- NPC runtime: authored personality + quest/game rules → perception/event feed → bounded goal planner/behavior tree or utility AI → action permission checks → authoritative game action → dialogue/voice/animation.
- Game-specific NPC behavior can respond to danger: fear rises from proximity and threat events, character hides, seeks allies, calls for help; cooldowns prevent thrashing.
- LLM dialogue is optional and asynchronous: content filters + context retrieval + bounded response length + fallback canned line + cancellation on scene change. Never put network LLM calls in frame-critical simulation.
- Rommie = named companion/assistant archetype with expressive movement, custom voices licensed/consented for use, configurable personality, tutorial guidance, mini-games and configurable memory. Emotion is simulation, not a claim of consciousness.
- **Containment**: game creators may install a restricted NPC prefab/policy profile into their room; sandbox exposes narrow action schema (move, speak, emote, interact with permitted objects), resource quotas and admin kill switch. No arbitrary shell/network/secrets access.
- AI pets: feeding, play, follow, sit, emotes, customizable accessories, cosmetic genetics/colors, limited persistent states. No exploitative mechanics or manipulative dependence.
- Safety: age-aware chat and moderation, refusal for disallowed requests, voice consent, user blocks, player-specific memory opt-out/deletion, disclosure of generated dialogue, scoped logs, and abuse detection.
- Performance: near-tier agents use detailed pathfinding, medium-tier use cheaper schedules, far-tier sleep; cap active speech and model inference per room.
**AI MVP:** 1 deterministic pet + 1 authored NPC that responds to game events + optional safe tutor chatbot; prove no AI agent can modify player inventory/economy without validated server action.

## 8. Engine strategy: build Metallic's proprietary layer in stages
**Recommendation:** develop a **Metallic runtime, creator editor, component model, network protocol and tools** on top of a measured existing renderer/physics platform before considering a full engine fork or clean-room engine. “Better” should mean benchmarked experiences, not automatically more lines of engine code.
- **Godot candidate:** source available and customizable; GDExtension for native modules; high-level networking options; OpenXR path needs real Quest/controller performance proof.
- **Unity candidate:** mature XR interaction toolchain and content pipeline; licensing/deployment needs business and technical review. Compare equal scenes, CPU/GPU frame time, iteration time, memory, device compatibility and server cost.
- Optional Rust authoritative services separate from client engine: identity, rooms, inventories, circuit execution and publication permission checks.
- ECS/component-based scene runtime with prefab/version system, asset registry and serialized schemas, hot reload in trusted authoring mode, streaming & LOD, VR input abstraction, deterministic/validated circuits, instrumentation.
- Multiplayer must be **server-authoritative** for critical state, rate-limited and secured against malicious clients.
**Decision gate:** run same scene with two connected players on PC and Quest for two engine candidates, measure frame time P50/P95/P99, memory, network bytes, cold start, production build stability and developer iteration latency; record an ADR. No absolute FPS promises without hardware matrix.
**Never** silently switch the current repository's planned engine until decision recorded.

## 9. Achievements and motivation
Types: exploration, engineering mastery, co-op, workshop completion, creator publication, contributions, seasonal challenges.
Each achievement has server-verified event predicates, immutable ID, rarity metadata, privacy setting, localization and anti-farming rate controls. Avoid rewarding unhealthy playtime. Course badges require assessment evidence and remain separate from game achievements.

## 10. Website support and operations
Unified support center across website, launcher and in-game watch:
- Self-help knowledge base, searchable troubleshooting, platform status, build/version diagnostics, crash report opt-in, ticket creation, ticket status, in-game bug reporting with reproduction steps and attachments.
- Forms adapt to billing, account access, moderation appeal, safety incident, accessibility issue, creator marketplace and technical support.
- Support bot retrieves **approved** docs, cites articles, asks permission before any account action, escalates on ambiguity/urgent safety, and never claims a ticket was made unless backend confirms.
- Staff dashboard: ticket queue, SLA, RBAC, audit trail, PII minimization, evidence retention and redaction; no broad database privileges for AI tools.
**Support MVP:** authenticated request creation/status + 10 troubleshooting guides + staff response flow and anti-spam safeguards.

## 11. Delivery order, dependencies and acceptance evidence
| Stage | Objective | Evidence to attach |
|---|---|---|
| P0 | Engine benchmark + architecture ADR | Same-scene PC and Quest captures; networking threat model |
| P1 | Multiplayer outdoor hub vertical slice | Two-player demo, reconnect, interaction and profile traces |
| P2 | UI/watch + creative authoring minimum | Controller/VR usability tests, versioned prefabs |
| P3 | Free Creator Exchange | Scanner/moderation pipeline, permission and version tests |
| P4 | Workshops + first Academy course | Consent/safety QA, exercise tests, instructor rubric |
| P5 | Pets + event-driven NPC + Rommie | Deterministic behavior, latency, safety and deletion tests |
| P6 | Expanded catalog, creator economics and content | Operations review, abuse tests, cost accounting |

### Engineering-level test matrix
- **Cross-device:** desktop low/medium/high and target standalone VR headsets; no VR sickness inducing cinematic.
- **Network:** jitter/loss/reconnect, hostile RPC fuzz tests, session scaling and rollback.
- **Security:** uploaded object validation, executable/script sandbox escapes, authorization tests, moderation/appeal audit.
- **AI:** hallucination, tool-authorization, prompt injection, age-sensitive safety, memory privacy, voice abuse, cost/latency.
- **Learning:** pre/post skill task, delayed retention, transfer tasks; not just enjoyment or badges.
- **Accessibility:** locomotion comfort, screen readers where possible, color/contrast and input parity.
- **Product:** measure successful first-room join, first creation, first publish, first lesson completion, crash-free sessions.

## 12. Supporting research and rationale
- [Godot 4 high-level multiplayer docs](https://docs.godotengine.org/en/stable/tutorials/networking/high_level_multiplayer.html): multiplayer abstraction, authentication and server trust boundaries.
- [Godot GDExtension documentation](https://docs.godotengine.org/en/stable/engine_details/engine_api/gdextension/what_is_gdextension.html): native modular engine extension without recompiling the main engine.
- [Unity XR Interaction Toolkit](https://docs.unity3d.com/6000.0/Documentation/Manual/com.unity.xr.interaction.toolkit.html): input, locomotion and 3D UI abstractions for XR.
- [Unity XR guide](https://docs.unity3d.com/6000.1/Documentation/Manual/XR.html): XR workflows and supported capabilities.
- [Unity Asset Store content transparency](https://unity.com/legal/asset-store-content-transparency): store content moderation and reporting precedent, not a drop-in policy.
- [Retrieval practice in stepwise worked examples (2025)](https://doi.org/10.1016/j.learninstruc.2025.102196): guided recall while learning procedural steps.
- [Gamified feedback in adaptive retrieval practice (2026)](https://www.sciencedirect.com/science/article/pii/S0747563225003097): subjective motivation improved, but no corresponding delayed recall improvement in the reported study.
- [Game-based learning apps in engineering education](https://research.utwente.nl/en/publications/game-based-learning-apps-in-engineering-education-requirements-de/): learner desire for authentic transfer and autonomy.

## 13. Open questions tracked, not blockers to start
- Target first-release engine and minimum Quest hardware?
- Named geography/style of original barn and farm; IP/licensing signoff for cinematic?
- Public vs private teacher program, permitted ages and workshop moderation?
- AI inference budget, local vs cloud voice pipeline, privacy/consent requirements?
- Supported asset executable code formats, marketplace payouts and legal eligibility?

**Reporting rule:** feature documents describe intent only. Mark features complete **only after** verified code, automated tests, hardware captures and accepted builds. Keep all proprietary source, player data, API keys and moderation records private.

> **Direction update — 20 October 2026:** use the owner's actual compiled client as the starting point. The [compiled-client rework](COMPILED_CLIENT_REWORK.md) supersedes earlier replacement-engine/hub proposals below. These plans do not establish shipped gameplay.

# Metallic-RC Creator, Teachers, Learning, Roomie, Watch, Achievements and Support

**Research and system design revision:** 2026-10-18. All below are **plans**, except the explicitly identified dependency-free *contract prototypes* under [game/](../game/README.md). None is integrated into a running game/backend.

## 1. Creator Team, teachers, workshop hosts

Separate **creator**, **verified mentor/teacher**, **workshop host**, **community moderator**, **developer** and **staff administrator**. Teaching accreditation is *not* inferred from an account badge. A verified teacher has a reviewed identity/experience and role scope, competency areas, program record, and safeguarding training. The host can schedule and run a class but cannot automatically access payment accounts, private learner records or moderation privileges.

Suggested flow: teacher application -> portfolio/reference review -> limited host permissions -> curriculum submission -> age/content review -> scheduled classroom room instance -> human attendance moderation -> sandboxed assignment -> rubric-based feedback -> verified, revocable completion record. Add waitlists, timezone-aware classes, capacity caps, substitutes, cancellations, volunteer hours, instructor training and anonymous feedback.

**College-level learning** means advanced depth, practical capstones and formal marking rubrics, **not university accreditation**. Only advertise transferable college credit or a degree if a separately authorized institutional partnership truly exists.

### Learning structure

- **Beginner:** onboarding, world building, material/mesh fundamentals, safe scripting.
- **Intermediate:** circuit state machines, multi-user tools, interaction design, profiling, accessibility, art direction and group collaboration.
- **Advanced / college-level challenge:** deterministic multiplayer, latency/replication, computer graphics, spatial UX, HCI research, machine-learning agent design, cybersecurity and ethical AI; require executable projects, scientific reporting and reproducible code.
- **Capstone:** build an original mini-game with signed assets, a graph, multiplayer test evidence, accessibility checks, measured performance, peer review and a human mentor rubric.
- **Learning outcomes:** tied to evidence IDs and explicit versions; progress and portfolios are private by default. Export badges using a standard such as **Open Badges 3.0** only when issuer and evidence requirements are genuinely met. Do not conflate internal achievements with real credentials.

The [prototype curriculum](../game/learning/workshops.v1.json) supplies four original workshops and explicit rubrics. [Validation code](../game/learning/education.mjs) confirms schema integrity and guards first achievement issuance, but it does not enroll students or issue credentials.

### AI in-game teacher

Start with **retrieval from a reviewed, versioned curriculum library**; guided Socratic questions, examples, hint levels, self-tests, debugging prompts and accessibility/translation. Provide citations to actual teacher-authored material, detect uncertainty, route disputed answers to a human reviewer, and log lesson version/model version for quality analysis. A conversational model may coach but **must not be the sole assessor, final grader, disciplinarian, human teacher impersonator or guarantee of qualification**.

Recommended design: lesson planner -> vetted knowledge retrieval -> response safety -> learner-controlled chat/voice (opt-in) -> quiz/simulation evaluator -> human review for consequential credit. Use benchmark questions, hallucination audits, prompt-injection adversarial tests and high-risk safety escalation. Never train on children's private chats without an appropriate lawful basis/consent.

Education sources: [UNESCO AI student competency framework](https://www.unesco.org/en/articles/ai-competency-framework-students?hub=343), [UNESCO teacher framework](https://www.unesco.org/en/articles/ai-competency-framework-teachers?hub=365), [UNESCO generative AI education guidance](https://www.unesco.org/en/articles/guidance-generative-ai-education-and-research?hub=686), [1EdTech Open Badges](https://www.1edtech.org/standards/open-badges), [1EdTech CASE](https://www.imsglobal.org/spec/CASE/v1p1/).

## 2. The invention library that serious creators actually want

The store should be **a professional asset browser** with editorially coherent thumbnails, actual preview media, precise metadata, clear licenses, performance cost and revisions. Two contexts share one taxonomy: **watch / in-game quick insert** (focused, large targets, short descriptions) and **desktop Studio** (search facets, version diffs, dependency graph, inspector, publisher console).

**Primary categories implemented in the contract:** Animations, VFX, Buildings, Circuits, Props, Audio, Materials, Characters, UI, Worlds, Templates, Pets, Tools. Future facet dimensions: rigs (humanoid, four-leg, hand); graph version; materials; mesh/poly/texture/particle budget; platform support; license; free/paid; maturity; author; engine/content version; reviewed status; dependencies; and import size.

**Search interaction design:**
- Typeahead for tags and natural language, but category and facet filters remain authoritative.
- Pinned curated "verified examples" rows, genuine creator collections, recently approved, accessible, low-end-device optimized and template starter kits.
- Grid ↔ dense list; preview with 3D orbit, turntable, audio waveform and animation scrubber depending on asset type.
- Compare revisions and dependencies; preview memory/triangle/material costs *from measured import analysis*.
- Prevent duplicate uploads, malicious bundles, unexpected script capabilities and content plagiarism. Report/takedown, appeals and creator-controlled visibility.
- Author attribution, usage license and age rating are visible **before** installation. No access to unapproved/rejected items via public search or direct guessable download URLs.
- Star/favorite/download metrics must be real, abuse-resistant and privacy conscious; use signed entitlements, immutable revision IDs and auditable moderation receipts.
- Purchases/tokens wait until proper ledger, age rules, refunds, fraud prevention and secure entitlements exist; the prototype contains **zero payment code**.

The [catalog module](../game/creator/catalog.mjs) validates ID/category/tags/digest/file limits and revision-bound approval, performs category/query/tag/audience filtering, and returns **public metadata only**. It is testable without a 3D engine but **not a trusted moderation, CDN or download API**. Unity remote Addressables/AssetBundles could load approved platform-specific content once the client decision is benchmarked; remote catalogs simplify content updates, not asset sandboxing.

Further reference: [Unity Addressables remote content](https://docs.unity.cn/Packages/com.unity.addressables%403.1/manual/remote-content-intro.html), [Roblox creator asset management](https://create.roblox.com/docs/projects/assets) and [content safety design](https://create.roblox.com/docs/safety). These are **patterns to evaluate**, not compatible Metallic plugins by default.

## 3. Human-like NPC AI in Metallic creator games

Use **three layers, never one giant free-form chatbot**:

1. **Deterministic action layer:** server-authorized states, navigation/pathfinding, perception, cooldowns, collision, threat assessment, animation rig and behavior trees / hierarchical state machines. All gameplay-critical moves and quest/achievement events are reproducible.
2. **Optional adaptive decision layer:** tested planner or learned policy (Unity ML-Agents or comparable after engine choice) constrained to an explicit action set, room budget and active scene. Must support fallbacks and replay traces.
3. **Optional dialogue layer:** player opt-in text/voice model with safe, factual constraints for tutoring; only outputs *proposals* to a separate allowlisted action executor. A model cannot spawn paid goods, change ownership, read hidden player data, grant staff powers or moderate people by itself.

NPC "scared", "happy" and "curious" are **simulated affect variables** that guide movement, facial cues, breathing/idle and voice prosody; they are **not actual feelings or consciousness**. Respect players who prefer nonverbal companions.

**Creator NPC Studio controls:** inspectable persona sheet; voice preset/rights; knowledge source; editable behavior graph; navigation zone; patrol/interaction points; scene-specific memory on/off; age rating; room permissions; timeout/budget; abuse-report button; deterministic replay, safe fallback and script rollback. NPCs must not impersonate real people without authorization. Do not copy copyrighted voices.

The [Roomie prototype](../game/ai/roomie.mjs) implements an inspectable signal-to-mood FSM with limited fear, bond and tick budgets, validated voice preset labels and fixed animation effects. It is *not* generative voice, a trained model or live game AI.

Research: [Unity ML-Agents toolkit](https://github.com/Unity-Technologies/ml-agents), [LiveKit voice agent architecture](https://docs.livekit.io/agents/), [OWASP LLM prompt injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/), [Roblox safe generative AI guidance](https://create.roblox.com/docs/generative-AI).

## 4. Original Roomie companions and customizable pets

Roomie should feel like a charming **game character with a personality**, not an omniscient surveillance tool. Let players unlock or adopt original companion types (botanical, woodland, tiny robot), recolor non-infringing looks, choose an authorized voice preset, choose personality and volume, and opt into follow/help/wait/play animations. Some creatures can react to danger with a skittish animation, hide near an owner, relax when safe and celebrate quest victories.

Create an **owner-side collection**, room-local spawn permission and persistent safe customization manifest. In creator games, the host may offer optional companion slots and trigger *whitelisted* abilities (point to next quest, teach an interaction, play an emote). The pet may never carry staff privileges, leak private room info or follow a blocked player. Support muting, hiding, despawning and "do not approach" accessibility controls.

**MVP:** one original creature, four moods, follow/wait, scared/recover, two visually distinct idle styles; proper multiplayer replication later. Voice recording must be strictly opt-in, disclosed and revocable. A pet should work in silent/offline-scripted mode.

## 5. Achievements and player progress

Distinguish **game achievements** (golf, exploration, quest, making a first invention), **skill evidence** (workshop artifacts marked against a rubric), **teacher credentials** (verified host scope), and **paid entitlements**. These are not interchangeable.

The client can display achievement animations but cannot award them. Trusted game session/education/publishing services emit event IDs into an idempotent event store. Awards have versioned criteria, proof ref, anti-cheat review, revocation rules and private/public preference. A test bot never unlocks real achievements. Classroom work needs instructor review and rubric evidence, not simply AI praise.

[Implemented contract prototype](../game/learning/education.mjs): trusted-caller shape, non-synthetic proof, revision/approval gate for publication, rubric gate for first workshop, stable achievement IDs, duplicate award idempotency. **Still missing:** real service authentication, persistence, signature verification, retries/reconciliation, GDPR/COPPA workflows.

## 6. A truly custom Metallic watch, menus and spatial map

**Design principle:** the watch is an original compact spatial launcher/command surface; don't duplicate Rec Room's protected layout, icon art or interaction geometry.

Home quick slots can include Map, Social, Create, Inventions, Learning, Roomie, Pets, Achievements, Support and Settings. Users can pin 1–6 slots. **Map buttons** should come from the selected room's supported destinations. The rejected replacement hub and its fixed destination list have been removed.

On PC, show a stable 2D responsive layout with keyboard and gamepad shortcuts. In VR use a wrist anchor **only when deliberately opened**, stable comfortable distance, readable targets, ray/direct-poke parity, haptic confirmation, no forced wrist rotation and reduced-motion options. Teleport/party/room join must ask the authoritative server to verify admission; a menu button is not permission.

[Watch menu contract](../game/watch/menu.mjs) is a pure data model with allowed screens, configurable quick slots, five input modes and markers for server-protected operations. It's not a rendered VR panel or signed action.

## 7. First-class support on website and in game

Single support case record reachable from website, launcher, in-game watch and staff console, not four disconnected bots. Offer **account recovery, technical/connection, harassment/safety, creator marketplace, teacher/learning, billing (later), accessibility and bug reporting**. Each case gets an opaque ID, classification, authenticated submitter, status, time markers, assignment, internal notes and clear user updates.

Safety/abuse reports need a **separate protected moderation workflow**, with direct urgent escalation and appeals. A volunteer cannot browse private account data simply because they host a class. Add evidence attachments with malware scanning, file quotas, limited retention, private storage, access logging and safe sharing. Offer transparent expected response windows only when staffed and measured. A chatbot may suggest help articles but cannot claim a report is resolved without staff action.

For privacy: age-aware account/parental workflows, consent for voice/model processing, data deletion/export and minimum retention; never expose learner data publicly. Evaluate requirements with counsel before launch: [FTC COPPA guide](https://www.ftc.gov/business-guidance/resources/complying-coppa-frequently-asked-questions) and [OWASP LLM sensitive-data disclosure](https://genai.owasp.org/llmrisk/llm022025-sensitive-information-disclosure/).

## 8. Delivery order and quality gates

| Priority | Dependency / target | Objective proof |
| --- | --- | --- |
| **P0 (now)** | Game engine + network benchmark, original world & model contracts | reproducible prototype tests and actual engine ADR |
| **P2** | Two-player room and first playable outdoor hub blockout | verified server-side interaction, real headset walkthrough |
| **P2** | Watch map + golf + first achievement + scripted Roomie | synchronized game events, unauthorized action denial |
| **P3** | Basic approved invention catalog and Maker Pen | validated asset import/publish + revisions and client render |
| **P3** | Creator NPC editor + pet customization + workshop room | safe scripted behavior, teacher role checks, sandbox |
| **P4** | Trustworthy support, AI tutor review queue, classroom progress | user request -> case -> human resolution; rubric audit |
| **Expansion** | Voice dialogue, model-assisted NPCs, paid store and standard badges | latency/cost/safety measured, incident/recovery drills |

No "fully automated AI moderator/teacher" or "real thinking NPC" claims. Promote features only when a build with test evidence exists.

## 9. Mandatory safety, security and performance review

- **Trust boundaries:** identify authenticated services vs clients, moderation decisions vs creator submissions, AI suggestions vs privileged executor, private learner data vs public discovery.
- **Abuse:** instruction injection through user-uploaded scripts, prompt text, voice transcripts, UGC assets or support attachments; transitive dependency abuse; denial of service through NPC spawning; grooming and bullying protections; escalation to humans.
- **Data handling:** purpose-limited transcript logs, opt-in mic access, visible recording state, data expiration/deletion, age/parent controls, no user impersonation, no autonomous outreach from NPCs.
- **Performance:** bound NPC senses/navigation/ticks, per-room agent quotas, spatial LOD and sound voices, streaming bundles, GPU overdraw, memory cap and network updates. Do actual VR/mobile profiling rather than assuming a server-side LLM is free.
- **Product design:** human review of visuals and interaction, descriptive rather than generic UI, verified working buttons, accessible controls, measured load times, low-latency fallback when AI cannot answer.
- **Release readiness:** rollback of manifests and AI policies; immutable audit trail; cross-device QA; penetration test for any production model tools.

These protections must be implemented **before** exposing minors to open-ended NPC dialogue, persistent memories or public creator assets.

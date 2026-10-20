# Metallic-RC 2026: evidence register for the next expansion

**Reviewed:** 2026-10-18. Source links are reference material, not endorsement or proof of shipped Metallic functionality. New measurements, compatibility validation and legal review may change decisions.

| Claim / design question | Primary/technical source | What it supports | Limits and Metallic decision |
| --- | --- | --- | --- |
| Independent client and social multiplayer | [Unity Netcode overview](https://docs.unity.com/en-us/multiplayer/netcode/netcode) | A supported multiplayer framework to benchmark | Not a built-in global social backend; dedicated-server design still required |
| Open source client alternative | [Godot Android OpenXR 4.6 guide](https://docs.godotengine.org/en/4.6/tutorials/xr/deploying_to_android.html) | Android XR integration exists | Docs warn about mobile renderer limitations; run comparative Quest test |
| Runtime UGC distribution | [Unity Addressables 3.1 remote content](https://docs.unity.cn/Packages/com.unity.addressables%403.1/manual/remote-content-intro.html) | Remote catalog/content updates without full reinstall | Only transport/loading; approval, signatures, sandboxing and licenses are separate |
| Asset publication safety | [Roblox Asset management](https://create.roblox.com/docs/projects/assets) and [Safety](https://create.roblox.com/docs/safety) | Moderation and limited-trust creator workflows are essential | Patterns only; cannot call Roblox APIs to moderate Metallic |
| NPC reinforcement learning | [Unity ML-Agents](https://github.com/Unity-Technologies/ml-agents) | Training NPC control/testing policies and imitation learning | Does not magically provide conversationally competent, safe characters |
| Real-time AI voice stack | [LiveKit Agents](https://docs.livekit.io/agents/) | A model/provider-neutral voice orchestration option | Roomie can ship first in silent scripted mode; expensive audio inference is optional |
| AI tutor pedagogy | [UNESCO student competency framework](https://www.unesco.org/en/articles/ai-competency-framework-students?hub=343) | Human-centred, ethical and project-oriented learner competencies | Does not certify Metallic courses or make an AI qualified to grade alone |
| Teacher training governance | [UNESCO teacher framework](https://www.unesco.org/en/articles/ai-competency-framework-teachers?hub=365) | Teachers need pedagogical, ethics and AI capabilities | Vet and train actual hosts |
| Digital achievements vs real credential | [1EdTech Open Badges](https://www.1edtech.org/standards/open-badges) | Versioned verifiable assertion/evidence metadata | Internal Metallic achievements are not university credit |
| Learning standards exchanges | [CASE 1.1](https://www.imsglobal.org/spec/CASE/v1p1/) | Structured competency/rubric alignment | Implement only after real course records exist |
| AI risk controls | [OWASP 2025 prompt injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) and [sensitive data](https://genai.owasp.org/llmrisk/llm022025-sensitive-information-disclosure/) | Prompt injection and PII disclosure are concrete risks | Never trust a model instruction as a gameplay permission |
| Child-facing game privacy | [FTC COPPA FAQ](https://www.ftc.gov/business-guidance/resources/complying-coppa-frequently-asked-questions) | Under-13 personal information and voice recordings require special handling in covered services | Legal review, age-aware implementation and appropriate consent before launch |
| Original scene vs copied Rec Room hub | [U.S. Copyright Office derivative works circular](https://www.copyright.gov/circs/circ14.pdf) | Copyright in original creative expression and derivative works matters | Build original hall/cinematic/art or obtain valid permissions for specific third-party assets |

## Architectural interpretation

**Best next move:** a Metallic-specific *runtime layer* over a benchmark-selected general-purpose renderer/physics/XR engine, with original creator schemas and owned servers. That is a credible route to "our own engine" without spending years rebuilding OpenXR support, low-level graphics, physics and device certification before there is even a playable two-player room.

**Priority order:** independent multiplayer -> original social hub -> authoritative interaction/achievement/watch -> moderated inventions -> safe Roomie/pets -> human-reviewed learning -> optional live generative AI and voice. Later, profile and replace particular subsystems with custom code *only when faster/safer by measurement*. See [ADR-0002](adr/ADR-0002-metallic-runtime.md).

**What this research does not prove:** player population, AI reasoning quality, avatar quality, exact licensing entitlement, performance, production moderation or any deployed game capability. All sources are requirements research, not product demonstrations.

## Open questions for real experiments

1. Which target headsets, GPU class, tracked hands/controllers and Android version does the first release actually support?
2. Can Unity/OpenXR and Godot/OpenXR sustain p95 frame budgets for a **matched** hub greybox on selected devices under actual avatar/object load?
3. What are content memory/mesh/VFX budgets for a public invention and one creator room? Measure load and egress with test assets.
4. Can an NPC react to danger with server-deterministic poses without visible snapping at 100ms simulated network jitter?
5. How many simultaneous workshops/classes and separate human teachers can the community safely supervise? Establish a staffing and age-safety policy.
6. For voice AI, what is p50/p95 latency, per-conversation operational cost, opt-in UX and moderation failure rate under worst-case testing?
7. Does every support escalated abuse report reach a real assigned moderator with audited resolution?
8. Is the initial world interesting and recognizable to players without copying Rec Room's center geometry, logos, item models, audio or stage design?

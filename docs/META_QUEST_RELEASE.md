# Metallic Studios — Meta Quest / Horizon Store, PC Distribution & QA Roadmap

**Research baseline:** October 15, 2026 · **Status:** release process design; no game APK or Meta submission exists in this repository.

## Objective

A downloadable Metallic game build for independent desktop distribution, plus a properly signed Meta Quest build that may progress through **internal**, **alpha**, **beta**, and **production** distribution when each channel's requirements are satisfied. Public Meta approval is not automatic; a working headset-ready executable, developer team, store listing and formal review are required.

## Current Meta documentation checked

| Official source (2026) | Technical implication |
| --- | --- |
| [Create apps (Aug 27)](https://developers.meta.com/vr/resources/publish-create-app/) | Create a unique Meta Horizon Store app page and App ID in Developer Dashboard before uploading |
| [Get started Unity Platform SDK (Oct 2)](https://developers.meta.com/vr/documentation/unity/ps-get-started/) | Required platform initialization/entitlement components for Store publication |
| [Prepare Unity app (Apr 27)](https://developers.meta.com/vr/documentation/unity/unity-prepare-for-publish/) | Configure IL2CPP and ARM64 on the selected Unity Quest target |
| [Quest packaging (Mar 26)](https://developers.meta.com/vr/resources/publish-upload-overview/) | Proper controller support, manifest, signed APK, APK up to 1 GB, optional OBB up to 4 GB; validator rejects incorrect packaging |
| [VRC Quest technical review (Aug 19)](https://developers.meta.com/vr/resources/publish-quest-req/) | Frame-rate, responsiveness, runtime/input/packaging and UX requirements checked as part of review |
| [Release channels (Mar 23)](https://developers.meta.com/vr/resources/publish-release-channels/) | Controlled groups in alpha/beta/etc; even test-channel packages must satisfy release packaging rules |
| [Submit app (May 8)](https://developers.meta.com/vr/resources/publish-submit/) | Send through Meta's review after checking VRC and metadata; guidance suggests submitting at least two weeks before desired launch |
| [Organization verification (Aug 24)](https://developers.meta.com/vr/resources/publish-organization-verification/) | Developer organization admin or business verification required to publish/update |
| [VR app lifecycle (Jul 7)](https://developers.meta.com/vr/resources/publish-app-review/) | Test builds and review/publishing stages handled through Meta Horizon Developer Dashboard |
| [Meta app/deep-link SDK (Sep 4)](https://developers.meta.com/vr/documentation/android-apps/ps-platform-sdk-application/) | Some inter-app launch/deep-link mechanisms exist, but do not assume any other revival supports integration |

These are source-backed publishing requirements, not claims about Metallic's current eligibility.

## Technical checklist — build pipeline

1. **Engine and target:** select Unity/OpenXR after measurement against Godot/other candidates. Test one exact supported headset/device matrix; do not imply all Quest models supported.
2. **VR interaction:** head tracking, controller/hand input fallback, seated/standing heights, snap turn, comfort locomotion, pause/resume, loss of focus and boundary handling.
3. **Performance:** target the required headset frame-rate under worst realistic gameplay, with capture evidence. Check controller input when frame CPU and GPU under stress.
4. **Graphics:** XR-compatible shader variants, render pipeline, baked/limited dynamic lighting, object batching, LOD, texture/particle budgets, overdraw.
5. **Network:** handle room join timeout, Wi-Fi handover, app suspend/resume, privacy/voice permissions, connection loss and signed session renewal.
6. **Package:** ARM64, IL2CPP, correct \`AndroidManifest.xml\`, signing certificate safely stored outside code, supported SDK/engine version, release profile, correct App ID and package ID.
7. **Store metadata:** original logo/icon, truthful screenshots from actual build, app title, summary, content classification and support policy; never use concept renders as gameplay screenshots.
8. **Testers:** upload to a restricted test channel and add authorized test accounts; confirm correct App ID and entitlement behavior.
9. **Compliance with VRC:** automate simple manifest/build checks and perform device test plan for functional requirements and graphics/comfort.
10. **Submission:** release candidate -> final test report -> submit for review; only mark released after Meta approves and listing is actually published.

### Proposed build evidence schema

\`\`\`json
{
  "releaseId": "0.1.0-quest-alpha",
  "sourceCommit": "<40-character-commit-sha>",
  "applicationId": "<Meta-App-ID>",
  "packageName": "com.example.metallic",
  "minHeadsetSpec": "chosen-device-matrix",
  "buildType": "signed-ARM64-IL2CPP",
  "artifactSha256": "<actual-hash>",
  "testRunUrl": "<validated-ci-run-url>",
  "vrcCheckResults": {"packaging": "pass", "frame_rate": "measured", "input": "pass"},
  "releaseChannel": "alpha",
  "publiclyReleased": false
}
\`\`\`

These are **placeholders** for a future actual build and test record. Never fill a blank VRC check with invented success.

## How it relates to a proprietary-independent desktop launcher

Desktop Windows/Linux builds can be downloaded from Metallic's own signed website/launcher with its own account server and no Steam process required. **Quest Store installations** are managed through Meta's supported app distribution/update mechanisms; a PC launcher cannot be presumed to install or replace Store-managed Quest APKs. Deep-link travel to another project is a separate integration feature and works only where platform policy/API, installed app and cooperating partner permit it. The game must provide a generic fallback when a partner is unavailable.

## Cross-platform release priorities

| Channel | Stage recommendation | Primary blocker |
| --- | --- | --- |
| Windows desktop | First closed playable alpha | Running client, signing/update, networking |
| PCVR on Windows | Alongside first multiplayer slice | OpenXR input/IK and frame pacing |
| Meta Quest | Controlled alpha/beta after measured headset build | ARM64 APK/manifest, VRC, developer dashboard, organization verification |
| Linux native | After engine/graphics portability tests | Renderer/driver/input parity, packaging |
| Android phone/tablet | Later dedicated UX and thermals | Touch UI, native store build/permission set |
| iOS phone/tablet | Later dedicated UX | iOS signing/provisioning/App Store review, platform testing |
| Other VR devices | Per-device test effort | Controller layout, vendor SDK and frame budgets |

## Games and experiences usable as a Quest review test suite

- Social lobby with 2–4 real accounts across headset and desktop, voice muted/off by default until permission chosen.
- One original co-op quest showing grabbing, teleport/move, collision and animated hazards.
- One creator tool scene with object placement, snapping and world save/reload.
- Dense GPU load room with capped particles, 16+ visible avatars/animations, varied shadows and room props.
- UI accessibility scene: large/small text, long languages, left-handed controls, controller disconnect and loss of focus.
- Network degraded room: Wi-Fi packet loss/rejoin states and device sleep/resume.

Do not submit a nonfunctional shell or status dashboard as if it were the game. Store publication requires a tested game build.

## Release gate

**Ready for Meta alpha** only when a real Quest APK signed and installable on a supported target shows tracked graphics, completes one gameplay loop with a staging server, has truthful app metadata, survives controller disconnect and passes required upload validator steps.

**Ready for production** only when reviewer-accepted store build meets target frame rate and supported input/comfort/safety requirements, moderation/reporting works, downloads and links are real, and on-call operational capacity exists.

## Immediate GitHub task breakdown

- Create app listing **only after** game name/identity, supported-device scope and dev organization are settled.
- Implement Quest build profile, signing strategy (secrets outside repo), metadata generator and manifest validator.
- Set up test channel strategy and test account/device matrix.
- Build/soak/review separate from issue tracking; no false “published on Meta” status unless validated against Meta dashboard.
- Treat Meta platform integration and independent Metallic backend as separate account/session concerns.

**No Meta Store publication or uploads were performed by writing this document.**

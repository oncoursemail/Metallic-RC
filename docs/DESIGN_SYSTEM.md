# Metallic-RC — Original Visual Identity, Interface System, Motion & Anti-Generic Design QA

**Research baseline:** October 15, 2026 · **Design direction:** *Obsidian Alloy* · **Status:** proposed brand system, prototype/experimentation required.

## Product identity

Metallic needs a design language, not another dark rounded-card dashboard made by a code model in a single pass. It should feel like a crafted game studio with physicality and restraint, not a clone of Discord, Rec Room, ChatGPT, or a generic SaaS template.

**Signature:** calm architectural geometry; refined deep-charcoal/graphite base; subtly luminous titanium typography; one controlled desaturated aubergine/plum accent, taken from the user's preferred metallic-purple logo direction; thin inset separators, precise interaction targets, hard hierarchy, high-density operational information without clutter.

**Do not use:** glowing neon borders, purple-blue gradients behind every panel, random giant circles/abstract particles, emoji placeholders as final icons, fake graphs without data, over-rounded pill everything, monotonous 3-column cards, default glass blur, text atop busy 3D art, low-contrast small gray labels, animations on every hover, impossible/unshippable features advertised as live.

Design must adapt to **three genuinely different contexts**:
1. **Launcher + public website:** branded editorial identity with deliberate large typography, download/repair/account workflow, platform support and reality-based development progress.
2. **In-game menu:** ergonomic world-space panel for VR, plus keyboard/mouse/gamepad layout; role-based moderation/creator tools integrate without interfering with gameplay.
3. **Staff/creator studio:** utilitarian desktop application layout with dense tables, inspector panels, precise controls and no unnecessary graphical ornamentation.

One shared set of spacing/type/color *tokens* can bridge all three, but UI component anatomy must suit the input device.

## Visual tokens v0.1

| Token | Value (proposal) | Usage |
| --- | --- | --- |
| canvas | \`#0E0F12\` | app background |
| surface-1 | \`#17181D\` | surface above canvas |
| surface-2 | \`#222329\` | selected tool / nested surface |
| border | \`#393A43\` | subtle 1px separators |
| text-primary | \`#F0EEF1\` | heading/body |
| text-secondary | \`#BAB8C2\` | label/metadata where contrast permits |
| subdued | \`#95929D\` | noncritical quiet text only; verify contrast |
| accent | \`#AA8CC5\` | small active indicator/action; do not make giant purple panels |
| accent-strong | \`#CCB1E0\` | active text on deep-dark surfaces |
| success | \`#82BFA3\` | success semantics with text labels |
| warning | \`#DCBA74\` | warning semantics |
| danger | \`#DD8589\` | destructive-action label/icon |
| font | system UI font stack | web readability; real custom logo separately |
| radius | 2–8px mostly | machined/angular character, no pill-based card grid |
| border width | 1px | subtle boundaries; 2px focus outline |
| motion | 150–240ms ease-out | purposeful focus/press, respect reduced motion |
| spacing | 4/8/12/16/24/32/48/64px | consistent scales, avoid arbitrary offsets |

**Validate contrast** with actual text sizes and backgrounds; token values are hypotheses, not certified accessibility metrics. The VR version must separately test headset optics, distance and headset refresh.

### Typography

- Display headlines: expressive but not noisy; careful kerning and a restrained custom wordmark for the studio. Do not load 5 font families or fake ultracondensed letters by stretching.
- UI: reliable system face with dense readable numbers, title 24–36px web, base 15–17px desktop, metadata ≥13px if legible, VR spatial text according to device guidelines.
- Tabular numerals for live player counts, build IDs, frame-time readouts, token prices and progress percentages.
- Paragraph widths 60–78ch; admin data can be denser with visible table headers and meaningful vertical rhythm.
- VR: [Meta typography guidance](https://developers.meta.com/vr/design/styles_typography/) recommends no less than 14px for legibility and 18px+ for comfort in its spatial UI guidance; real-world apparent size depends on distance and rendering.

### Iconography & feedback

Use a coherent original icon family (stroke width/optical size consistent), not arbitrary emojis or mixed packs. Prefer descriptive text for irreversible staff actions. Every interaction has keyboard focus, hover, press, disabled and loading states. Show actual server response or queued state; never instantly flash “success” before backend confirmation.

### Motion direction

- Website progress: short, single bar fill on first load with no continuous animation; provide nonanimated static equivalent.
- Navigation: reveal on meaningful screen change only; 160–220ms, no bouncy easing.
- Launcher downloads: accurate throughput/progress and pause/retry. If size unknown, show indeterminate spinner plus explicit text, **never fabricated percentage**.
- In-game: preserve head comfort; do not attach rapidly animating UI to VR camera. Menus should be stable anchors, and fades should be used carefully.
- VFX and world animation can be rich but must have quality tiers and budgets: particle pooling, occlusion and LOD, no seizure-prone flashes, haptic/audio alternatives, and deterministic startup/stop states.

References: [Apple HIG motion](https://developer.apple.com/design/human-interface-guidelines/motion), [Apple design principles, 2026](https://developer.apple.com/design/human-interface-guidelines/design-principles), [Meta VR UI typography](https://developers.meta.com/vr/design/styles_typography/), [Meta Quest VRC](https://developers.meta.com/vr/resources/publish-quest-req/).

## Concrete screen structures

### A. Public website / development status prototype

\`\`\`text
METALLIC  /  [THE PLATFORM] [PROGRESS] [CREATORS] [DOCUMENTATION]           [GITHUB]
────────────────────────────────────────────────────────────────────────────────────
KICKER   AN INDEPENDENT SOCIAL WORLD / PRE-ALPHA

A more capable world, built in public view.
Original VR social spaces, powerful creator tools, performant dedicated servers.
                                                    [PROJECT BUILD STATUS]
   [SEE ENGINEERING ROADMAP]                         1 of 34 tasks verified
                                                     ▰▱▱▱▱▱▱▱▱▱   2.9%
────────────────────────────────────────────────────────────────────────────────────
CURRENT PRIORITIES                            DELIVERY PIPELINE
01    PROTOCOL & ROOM FOUNDATION               P0 Foundation   [...]
02    CREATOR SDK / CV3                        P1 Accounts     [...]
03    STAFF & COMMUNITY SAFETY                 P2 Multiplayer [...]
────────────────────────────────────────────────────────────────────────────────────
THIS IS NOT A GAME DOWNLOAD YET. No player count, unavailable-platform promises or fake art.
\`\`\`

**Look:** purposeful hero proportion; dense bottom catalog; real issue-derived progress; angled stripe motif used as one vertical branded spine, not ornamental everywhere; all numbers sourced from current repository snapshot. HTML/CSS/JS version goes under \`docs/site\` or \`docs/\` per Pages config.

### B. Desktop launcher

Top: metallic logo, local install state, account, settings. Left: *Play / Create / Explore / Updates / Downloads*. Main: current build compatibility, news, downloads, repair; bottom: region and connectivity status. Only offer enabled actions when a real signed build exists. External project launcher registry must disclose source, publisher and signed manifest state and obtain explicit click.

### C. Game menu

A VR watch-like panel can be familiar without copying original UX geometry. Left navigation with clear hit targets (Rooms, Social, Avatar, Create, Settings). Right content with title + filter/sort + meaningful cards. In VR display minimal data at comfortable distance, scrolling and focus respecting controller ray and direct hand input. On desktop, show advanced search and actual keyboard shortcuts.

### D. Staff UI / Studio

Dense left explorer / central canvas / right inspector, not a big dashboard mosaic. Example staff queue:
- case priority chip + created timestamp + location/room ID + assigned moderator + next action,
- report detail pane with allowed evidence,
- exact target ID confirmation for sanctions,
- reusable reason templates, duration entry, warning/appeal preview,
- append-only audit receipt,
- dark neutral palette with warning accent only on genuine priority.
Same server API powers VR trimmed version, desktop admin panel and Discord bot.

### E. Staff overhead labels

Compact, readable \`Volunteer Mod\` or \`Developer\` directly **above** the character nameplate if enabled. Single unobtrusive badge/color cue; never mask an avatar's eyes or overlap other nameplates; distance-fade/LOD and occlusion. No fake glowing halo or animated emoji crown. A toggled-off title affects visual display, not authorization.

## VFX & animation style guide

**Material story:** titanium, satinated steel, dark obsidian composites and restrained plum reflections. World art can be warmer, more colorful and playful than the admin/launcher interfaces; avoid making the game itself gray monochrome.

**VFX taxonomy:**
- **Ambient:** small dust motes, wind and environmental particles with pooling and per-room visual density cap.
- **Social:** tasteful invite arrival, friend join, emotes, hover highlight and outfit equip.
- **Maker Pen:** precise snapping confirmation, object pivot glow only on selected target, undo rewind cue, multiplayer edit ownership feedback.
- **Quest:** readable impact rings, boss warnings, victory burst, teleport effect, loot reveal, surface contact and directional haptics.
- **User moderation:** discreet notification/toast, never theatrical punishment effects.
- **Network state:** connection/reconnect/lag indicators accurate to real RTT and packet loss.

**Animation taxonomy:** gameplay locomotion/IK, hands and gestures, avatar idle variation, cloth/hair secondary motion, facial cues, contextual interaction, menu transitions, tool hover/focus, staff role title fade (no constant motion). Include emote cancellation and tool/weapon grip alignment; scripts must test transitions under network loss and animation LOD.

**Performance acceptance:** verify real Quest/PC target frame rates, pooling, overdraw, CPU GPU split, memory spikes, UI draw calls and packet bandwidth for replicated emotes. Dynamic resolution/foveation where supported and tested.

## Anti-generic / anti-error QA protocol

Each UI screen must pass all of the following:
1. **Real content:** no invented statistics, false download buttons or “Live 24,392” placeholders.
2. **Distinct hierarchy:** one primary objective, appropriate density, recognizable navigation, clear secondary paths.
3. **Geometry:** consistent baseline grid, precise text/border alignments, no overlap at smallest target resolution.
4. **Contrast:** check normal, disabled, focus, selection, danger and disabled states in both desktop and VR.
5. **Behavior:** every visible control actually performs something or is disabled with clear explanation.
6. **Responsive:** mobile/desktop tablet breakpoints, landscape/portrait where applicable, 200% zoom and long translated text.
7. **Input:** keyboard/gamepad/VR ray/direct-hand equivalent for essential flows.
8. **Motion:** \`prefers-reduced-motion\` and in-game comfort setting disable nonessential animation.
9. **Aesthetic critique:** review alongside hand-crafted reference products. Find generic cards, fake glass, inconsistent radii, emoji placeholders and typography mistakes *before* signoff.
10. **Visual evidence:** capture screenshots from actual builds, compare against approved image baselines and have a human review differences. [Playwright snapshot testing](https://playwright.dev/docs/test-snapshots) can automate regression but cannot judge original design quality.
11. **Performance:** load large catalog and staff queue states; scroll/input target latency measured.
12. **Polish gate:** no broken icons, placeholder copy, incorrect role labels, untested localization and mismatched logo variants.

Create three separate reviewed reference screens **before** implementing all screens: launcher/home, game room browser, and moderator case workspace. Make them excellent rather than generating 30 generic pages.

## Research sources and next experiment

- [Apple design principles](https://developer.apple.com/design/human-interface-guidelines/design-principles)
- [Apple layout / hierarchy](https://developer.apple.com/design/human-interface-guidelines/layout)
- [Apple motion](https://developer.apple.com/design/human-interface-guidelines/motion)
- [Meta spatial UI typography](https://developers.meta.com/vr/design/styles_typography/)
- [Meta headset input and text requirements](https://developers.meta.com/vr/documentation/android-apps/design-requirements/)
- [Meta Quest VRC checklist](https://developers.meta.com/vr/resources/publish-quest-req/)
- [Playwright screenshot assertions](https://playwright.dev/docs/test-snapshots)

**Next concrete design task:** implement a static, accessible, genuine data-driven **public development-status website prototype** and a dynamically generated *GitHub README progress artwork* using the existing issues-to-JSON automation. UI is development visualization only, not proof that the actual game is built.

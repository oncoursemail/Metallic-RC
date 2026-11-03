# Metallic launcher component setup

A repository-owned optional-component selector and installation planner. Node 22+ built-ins and native browser modules only; no package installation is required.

From this directory run `npm test` and `npm start`. Open http://127.0.0.1:4173. Set `METALLIC_LAUNCHER_PORT` to change the local port. The development server binds to loopback and serves a fixed asset allowlist.

## Implemented

All components start unselected: **Metallic**, **Minecraft Java**, **Metallic Host**. Any combination, including empty, can be saved. Only Minecraft adds its selected-version Java runtime. Playing either game never adds hosting.

Preferences use localStorage key `metallic.launcher.preferences.v1`. Saving records choices only; it does not download, install, launch, enroll a host or alter installed inventory. Reload restores choices. Unavailable native installers and public identity services are clearly labeled.

The inspected complete main tree at `20abe16e8137874615c4cd04fe8c3d4986d0ed6c` contained no native launcher. This new package begins the launcher under the existing services structure. It is a development UI and reusable planning code, not a shipped desktop application.

## Planner

`src/planner.mjs` exports `planInstallation`, `planRemoval` and `validateArtifact`. Installation planning accepts selected component IDs, a platform, trusted catalog, exact trusted origins, and separate downloaded/installed inventories. It returns dependency order, individual operations, blockers, download/install queues and checked byte totals.

The stable IDs are `metallic-game`, `minecraft-java`, `metallic-host` and the managed dependency `minecraft-runtime`. Empty selection is valid. Unknown IDs, duplicate inventory receipts, cyclic dependencies, hidden dependencies on optional components, unverified receipts, unsafe artifacts and overflow fail validation.

All selected requirements must pass before executable queues are returned. Unselected unavailable components do not block a selected component. Downloaded bytes still require installation. Existing verified matching installations are retained. Selecting/deselecting never uninstalls software.

Removal is a separate explicit request. It plans removal of the chosen component and unused managed dependencies, retains shared runtimes and always preserves user data. This module performs no filesystem deletion.

## Remaining native integration

The shipping catalog intentionally contains no artifact URLs. A native installer must load authenticated signed release metadata before passing artifacts to the planner. Artifacts bind component, platform, version, safe filename, HTTPS source, byte size and SHA-256. Trusted origins are explicit. This planner does not verify release signatures.

The executor must independently verify metadata and actual bytes, stage files, enforce safe archive paths and symlinks, control redirects, activate atomically and write installed receipts only after success. Never treat browser preferences or a caller's `verified` Boolean as a native attestation. Store saves, worlds, profiles, configuration and credentials outside removable packages. User-data purging is a separate explicit operation.

Minecraft needs Metallic's own registered Microsoft application, entitlement validation and an official metadata-driven installer. Java requirements vary by Minecraft version. Do not borrow another launcher's app ID or redistribute proprietary binaries without authorization.

Discord needs the public account service. The separate private Metallic MC administrator workbench is not public launcher authentication. The optional Host package needs signed releases, supported OS isolation, local supervision and explicit pairing; it must never be installed or started merely by choosing a game.

Supported target selectors are Windows, macOS and Linux on x64/ARM64. These are planning targets, not claims that released installers or device acceptance tests exist on every platform.

## Checks

`node --test tests/*.test.mjs` exercises 21 cases including all eight component combinations, runtime ordering, hidden/cyclic dependencies, unavailable releases, platform selection, distinct downloaded/installed state, receipt integrity, updates, unsafe artifacts, aggregate size overflow and data-preserving removal with shared runtimes.

Browser acceptance uses the real local server, verifies independent selection, saving/reloading, empty selection, no installed-state fabrication, desktop/mobile layout and JavaScript errors. See the validation record added with this change. A GitHub Actions workflow runs the planner tests on Node 22.

Work authored through owner-directed ChatGPT assistance; no Star AI authorship or automated deployment is claimed.

See the [Validation record](VALIDATION.md) for completed checks and release gates.

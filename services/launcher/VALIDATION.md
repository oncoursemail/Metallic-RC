# Launcher validation — 19 October 2026

The first launcher module is a local selection interface and a tested installation/removal planner. It is not yet a packaged desktop launcher or a Minecraft authentication client. Shipping artifact entries intentionally have no download URLs until verified release artifacts are available.

## Checks completed

Validation used Node 24.21.0 on Linux. The module declares Node 22 or newer and its GitHub workflow runs the planner tests on Node 22.

`node --test services/launcher/tests/*.test.mjs` passed all **21 tests**, with zero failures. Coverage includes every combination of the three optional components; Minecraft's runtime dependency; unavailable releases; dependency cycles; invalid metadata and origins; downloaded versus installed receipts; install readiness; aggregate byte overflow; and removal plans that preserve user data and shared requirements.

Playwright 1.64.1 and Chromium 156 exercised the actual loopback HTTP server at desktop (1440px) and mobile (390px) widths. The launcher:

- Starts with no components selected.
- Allows Metallic, Minecraft Java and the future host component to be selected independently.
- Requires the Java runtime only when Minecraft is selected.
- Saves and restores selected components and platform choices.
- Saves an empty selection.
- Does not write downloaded or installed inventory into saved preferences.
- Fits both tested widths without horizontal overflow.
- Produces no uncaught browser JavaScript errors in the acceptance flow.

These checks formed part of a 14-check acceptance run that also exercised the companion Architect workbench. Screenshots of both launcher widths were visually inspected. The temporary test servers were stopped afterward.

## What is still required

A native desktop shell must provide operating-system credential storage and a narrow, authenticated bridge to install operations. It must verify signed release metadata, fetch and hash bounded artifacts, stage and atomically install them, maintain protected durable receipts, and support interruption recovery and rollback. A planner receipt supplied by a caller is not a cryptographic attestation.

Minecraft also needs Metallic-owned Microsoft application registration, supported Xbox/Minecraft authentication and ownership checks. Discord linking belongs to the account service and must be connected to the native launcher's public identity flow. Neither credential flow is implemented in this module.

The host component still requires a real isolated host agent and protected outbound relay. Windows, macOS and Linux entries in the selector are planning targets; they do not claim that native installers or equivalent sandbox support exist.

This module does not download or launch the original Metallic game, Minecraft or any Rec Room/Roblox executable. Removing a choice does not uninstall software. No public hosting, in-game directory, paid service or public deployment was created by these checks.

See [README](README.md) for the runnable development surface and module layout. This work was performed by ChatGPT.

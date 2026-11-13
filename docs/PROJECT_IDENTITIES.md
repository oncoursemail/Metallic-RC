# Metallic project identities

Owner naming decision: 19 October 2026. Use these names in project selection, current links, documentation, and new work.

| Project | Scope | Canonical repository | Stable GitHub repository ID |
|---|---|---|---:|
| Metallic-RC | Independent Rec Room-style social multiplayer and VR platform | [Metallic-RC](https://github.com/oncoursemail/Metallic-RC) | 1420186760 |
| Metallic-MC | Minecraft Java and Bedrock network | [Metallic-MC](https://github.com/oncoursemail/Metallic-MC) | 1424831109 |
| Metallic-RB | Roblox Rec Room project; current work is the Paintball preservation port | [Metallic-RB](https://github.com/oncoursemail/Metallic-RB) | 1408973704 |
| Metallic-PX | Public progress presentation | [Metallic-PX](https://github.com/oncoursemail/Metallic-PX) | 1410841343 |

**Metallic** remains the family name. **Metallic.club** is the owner-designated family domain, purchased on 19 October 2026. This record does not configure DNS or establish a live application endpoint. Keep private control-plane addresses, credentials and host names out of public progress files.

Repository IDs establish identity across a rename. Refresh a repository's current name and permissions from authenticated GitHub metadata before using a cached slug. A name change does not grant access; missing authorization remains a distinct condition. The former `oncoursemail/Metallic` refers to Metallic-RC; the former `oncoursemail/recroom-paintball-roblox` refers to Metallic-RB; `oncoursemail/recroom-paintball-roblox-progress` refers to Metallic-PX.

References to Rec Room, Roblox, Minecraft, upstream game content, old research observations and source provenance retain their original meanings. Internal module names, environment variables, data schemas, workload IDs, file paths and installed service names are compatibility identifiers; a display-name correction does not migrate them. In particular the existing `metallic-world` workload identifies Metallic-RC, and `mc-lobby` and `mc-match` identify Metallic-MC. The lowercase Go module path for Metallic-MC is retained to preserve its existing import contract.

The public progress site is [Metallic-PX](https://oncoursemail.github.io/Metallic-PX/). Its generated content comes from Metallic-RB's progress builder. Update the source fields and generator together so a later build preserves these names.

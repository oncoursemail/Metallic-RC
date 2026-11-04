//! The Paintball map's game places, read from the reference install's scene (`levels`: the 2026 level files are
//! scrambled): team spawn points (`Systems/SpawnPoints/RedTeam|Team1` red, `BlueTeam|Team2` blue, `PreGame` the
//! lobby), the flags (`Flags/RedBase|BlueBase/[FlagTool]`) and their goals (`[FlagGoal]`, flag at its
//! `Flag Attach Point`), and the field (`*ActivityBounds/FieldBounds*` box colliders, when there are any: in River
//! they are two MonoBehaviour volumes, unreadable without type trees, so no field). Same names in all six maps
//! (River level13, Construction 14, Castle 15, ClearCut 16, Dam 17, DriveIn 102). `Team1` = red is INFERRED (DriveIn
//! keeps `Team1SpawnPoint`s under `RedTeam`).
//!
//! Anywhere else (no reference install, another room) [`practice`] makes a small field around the player.
use std::collections::HashMap;
use std::path::Path;

use bevy::math::{Mat4, Vec3};
use rr_net::games::{MapInfo, Spot, BLUE, RED};
use unity_assets::{classes, level, SerializedFile};

/// The room the client was started in (`--world`), which is the map a match is played on.
pub fn world_arg() -> Option<String> {
    let mut it = std::env::args();
    while let Some(a) = it.next() {
        if a == "--world" {
            return it.next();
        }
    }
    None
}

/// The level file for a room name: `levelN`, or a scene name (`paintball`, `Paintball_Castle`).
fn level_of(data: &Path, room: &str) -> Option<String> {
    if let Some(n) = room.strip_prefix("level").and_then(|n| n.parse::<usize>().ok()) {
        return Some(format!("level{n}"));
    }
    let ggm = SerializedFile::parse(std::fs::read(data.join("globalgamemanagers")).ok()?).ok()?;
    let scenes = level::build_scenes(&ggm).ok()?;
    scenes.iter().position(|s| level::scene_name(s).eq_ignore_ascii_case(room)).map(|i| format!("level{i}"))
}

struct Node {
    name: String,
    parent: Option<usize>,
    world: Mat4,
    components: Vec<(i32, i64)>,
}

fn read_nodes(sf: &SerializedFile) -> Vec<Node> {
    let mut transforms = HashMap::new();
    for o in sf.objects.iter().filter(|o| o.class_id == classes::TRANSFORM || o.class_id == classes::RECT_TRANSFORM) {
        if let Ok(t) = sf.transform(o) {
            transforms.insert(o.path_id, t);
        }
    }
    let mut stack: Vec<(i64, Option<usize>)> = transforms.iter().filter(|(_, t)| t.parent.is_null()).map(|(&id, _)| (id, None)).collect();
    let mut nodes: Vec<Node> = Vec::new();
    while let Some((tid, parent)) = stack.pop() {
        let Some(t) = transforms.get(&tid) else { continue };
        let Some(go) = sf.object(t.game_object.path_id).and_then(|o| sf.game_object(o).ok()) else { continue };
        let pw = parent.map_or(Mat4::IDENTITY, |p| nodes[p].world);
        let components = go.components.iter().filter_map(|c| sf.object(c.path_id).map(|o| (o.class_id, o.path_id))).collect();
        nodes.push(Node { name: go.name.clone(), parent, world: pw * crate::room::unity_to_bevy(t), components });
        let me = nodes.len() - 1;
        stack.extend(t.children.iter().map(|c| (c.path_id, Some(me))));
    }
    nodes
}

fn spot(world: &Mat4) -> Spot {
    let (_, rot, pos) = world.to_scale_rotation_translation();
    let d = rot * Vec3::NEG_Z;
    Spot { pos: pos.into(), yaw: (-d.x).atan2(-d.z) }
}

/// The map's places from scene `room` (`levelN` or a scene name) of the reference install `data` (its
/// `RecRoom_Data`). Errors say what is missing.
pub fn read(data: &Path, room: &str) -> Result<MapInfo, String> {
    let file = level_of(data, room).ok_or_else(|| format!("no scene {room} in the reference install"))?;
    let sf = SerializedFile::parse(std::fs::read(data.join(&file)).map_err(|e| format!("{file}: {e}"))?).map_err(|e| format!("{file}: {e}"))?;
    let nodes = read_nodes(&sf);
    let name = |i: Option<usize>| i.map_or("", |i| nodes[i].name.as_str());
    let mut m = MapInfo { name: room.to_string(), ..Default::default() };
    let (mut tools, mut attach, mut goals) = ([None, None], [None, None], [None, None]);
    let mut field: Option<(Vec3, Vec3)> = None;
    for (i, n) in nodes.iter().enumerate() {
        let (parent, grand) = (name(n.parent), name(n.parent.and_then(|p| nodes[p].parent)));
        if grand == "SpawnPoints" {
            match parent {
                "RedTeam" | "Team1" => m.spawns[RED as usize].push(spot(&n.world)),
                "BlueTeam" | "Team2" => m.spawns[BLUE as usize].push(spot(&n.world)),
                "PreGame" => m.lobby.push(spot(&n.world)),
                _ => {}
            }
        }
        let team = match parent {
            "RedBase" => Some(RED as usize),
            "BlueBase" => Some(BLUE as usize),
            _ => None,
        };
        if let Some(t) = team {
            if n.name == "[FlagTool]" {
                tools[t] = Some(n.world.w_axis.truncate());
            } else if n.name.starts_with("[FlagGoal]") {
                // the flag sits on the goal's attach point
                goals[t] = Some(n.world.w_axis.truncate());
                attach[t] = nodes.iter().find(|c| c.parent == Some(i) && c.name == "Flag Attach Point").map(|c| c.world.w_axis.truncate());
            }
        }
        if n.name.starts_with("FieldBounds") && parent.ends_with("ActivityBounds") {
            for &(_, id) in n.components.iter().filter(|c| c.0 == classes::BOX_COLLIDER) {
                let Ok(c) = level::box_collider(&sf, sf.object(id).unwrap()) else { continue };
                for k in 0..8 {
                    let corner = Vec3::new(
                        c.center[0] + c.size[0] * if k & 1 == 0 { -0.5 } else { 0.5 },
                        c.center[1] + c.size[1] * if k & 2 == 0 { -0.5 } else { 0.5 },
                        -(c.center[2] + c.size[2] * if k & 4 == 0 { -0.5 } else { 0.5 }),
                    );
                    let p = n.world.transform_point3(corner);
                    field = Some(field.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
                }
            }
        }
    }
    if m.spawns.iter().any(Vec::is_empty) {
        return Err(format!("{room}: no team spawn points (red {}, blue {})", m.spawns[0].len(), m.spawns[1].len()));
    }
    for t in 0..2 {
        let centre = m.spawns[t].iter().fold(Vec3::ZERO, |a, s| a + Vec3::from(s.pos)) / m.spawns[t].len() as f32;
        m.goals[t] = goals[t].unwrap_or(centre).into();
        m.flags[t] = attach[t].or(tools[t]).unwrap_or(centre + Vec3::Y).into();
    }
    if m.lobby.is_empty() {
        m.lobby = m.spawns[0].clone();
    }
    m.bounds = field.map(|(lo, hi)| (lo.into(), hi.into()));
    Ok(m)
}

/// A small field of our own around `centre` (feet) facing `yaw`: teams 24 m apart, flags 2 m behind them.
pub fn practice(centre: Vec3, yaw: f32) -> MapInfo {
    let rot = bevy::math::Quat::from_rotation_y(yaw);
    let at = |x: f32, z: f32| (centre + rot * Vec3::new(x, 0.0, z)).to_array();
    let row = |z: f32, face: f32| (-2..=2).map(|i| Spot { pos: at(i as f32 * 1.5, z), yaw: yaw + face }).collect::<Vec<_>>();
    MapInfo {
        name: "practice".into(),
        lobby: vec![Spot { pos: centre.into(), yaw }],
        spawns: [row(12.0, 0.0), row(-12.0, std::f32::consts::PI)],
        flags: [(Vec3::from(at(0.0, 14.0)) + Vec3::Y).into(), (Vec3::from(at(0.0, -14.0)) + Vec3::Y).into()],
        goals: [at(0.0, 14.0), at(0.0, -14.0)],
        bounds: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practice_field_faces_each_other() {
        let m = practice(Vec3::new(1.0, 0.0, 2.0), 0.0);
        assert_eq!(m.spawns[0].len(), 5);
        assert!(m.spawns[0][0].pos[2] > 2.0 && m.spawns[1][0].pos[2] < 2.0);
        assert!((m.flags[0][1] - 1.0).abs() < 1e-5);
    }

    /// Reads River (level13) from the reference install when this PC has one.
    #[test]
    fn river_from_the_reference_install() {
        let Some(data) = crate::levels::ref_install() else { return };
        let m = read(data, "level13").unwrap();
        assert!(m.spawns.iter().all(|s| s.len() >= 4), "{:?}", m.spawns.iter().map(Vec::len).collect::<Vec<_>>());
        // the bases are about 50 m either side of the middle (Unity z -49.7 is red's: Bevy z +49.7)
        assert!((m.goals[RED as usize][2] - 49.7).abs() < 1.0, "{:?}", m.goals);
        assert!((m.goals[BLUE as usize][2] + 49.9).abs() < 1.0, "{:?}", m.goals);
        assert!(m.lobby.len() >= 8);
    }
}

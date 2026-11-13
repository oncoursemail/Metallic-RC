//! Validate MonoFields against the real gameplay gadgets, using the decompile lane's schemas:
//!   DiscGolfGoal : RRNetworkBehavior  -> hole:int, par:int, beamRenderer:PPtr, OnGoalHitEvent:UnityEvent
//!   PaddleballGoal: RRNetworkBehavior -> team:int, ScoreEvent:UnityEvent
//! (RRNetworkBehavior adds no serialized fields, so these are the first scalars after the MB header.)
//! Scans every level file in a decrypted-corpus dir and prints the values it reads, with a sanity verdict.
//! usage: rr-gadget-fields <corpus-dir>
use std::collections::HashMap;
use unity_assets::*;

struct Files {
    dir: std::path::PathBuf,
    open: HashMap<String, Option<SerializedFile>>,
}
impl Files {
    fn get(&mut self, name: &str) -> Option<&SerializedFile> {
        self.open.entry(name.to_string()).or_insert_with(|| std::fs::read(self.dir.join(name)).ok().and_then(|d| SerializedFile::parse(d).ok())).as_ref()
    }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("corpus dir"));
    let mut files = Files { dir: dir.clone(), open: HashMap::new() };
    let (mut disc, mut disc_ok, mut pad, mut pad_ok, mut soccer, mut soccer_ok) = (0, 0, 0, 0, 0, 0);
    for i in 0..=200 {
        let name = format!("level{i}");
        if !dir.join(&name).exists() {
            continue;
        }
        let Some(data) = std::fs::read(dir.join(&name)).ok() else { continue };
        let Ok(sf) = SerializedFile::parse(data) else { continue };
        // class name of a MonoBehaviour, resolving m_Script across the scene's sibling files
        let externals: Vec<String> = sf.externals.iter().map(|e| e.path.rsplit('/').next().unwrap_or(&e.path).to_string()).collect();
        for o in sf.objects.iter().filter(|o| o.class_id == classes::MONO_BEHAVIOUR) {
            let Ok(p) = level::mono_script_ref(&sf, o) else { continue };
            let class = if p.file_id == 0 {
                sf.object(p.path_id).and_then(|so| level::mono_script_name(&sf, so).ok()).map(|(c, _)| c)
            } else if let Some(ext) = externals.get(p.file_id as usize - 1) {
                files.get(ext).and_then(|f| f.object(p.path_id).and_then(|so| level::mono_script_name(f, so).ok())).map(|(c, _)| c)
            } else {
                None
            };
            match class.as_deref() {
                Some("DiscGolfGoal") => {
                    disc += 1;
                    if let Ok(mut mf) = level::MonoFields::open(&sf, o) {
                        let (hole, par) = (mf.i32().unwrap_or(-999), mf.i32().unwrap_or(-999));
                        let sane = (-1..=17).contains(&hole) && (1..=12).contains(&par);
                        if sane {
                            disc_ok += 1;
                        }
                        if disc <= 10 {
                            println!("{name}: DiscGolfGoal hole={hole} par={par}  {}", if sane { "OK" } else { "??" });
                        }
                    }
                }
                Some("PaddleballGoal") => {
                    pad += 1;
                    if let Ok(mut mf) = level::MonoFields::open(&sf, o) {
                        let team = mf.i32().unwrap_or(-999);
                        let sane = (0..=16).contains(&team);
                        if sane {
                            pad_ok += 1;
                        }
                        if pad <= 10 {
                            println!("{name}: PaddleballGoal team={team}  {}", if sane { "OK" } else { "??" });
                        }
                    }
                }
                Some("SoccerGoal") => {
                    // SoccerGoal { goalCollider: PPtr(BoxCollider), team: int } — the AABB is the referenced collider
                    soccer += 1;
                    if let Ok(mut mf) = level::MonoFields::open(&sf, o) {
                        let gc = mf.pptr().ok();
                        let team = mf.i32().unwrap_or(-999);
                        let bc = gc.filter(|p| p.file_id == 0).and_then(|p| sf.object(p.path_id)).and_then(|co| level::box_collider(&sf, co).ok());
                        let sane = (0..=16).contains(&team) && bc.is_some();
                        if sane {
                            soccer_ok += 1;
                        }
                        if soccer <= 10 {
                            let aabb = bc.map(|c| format!("goalCollider size={:?} center={:?} trigger={}", c.size, c.center, c.trigger)).unwrap_or_else(|| "goalCollider UNRESOLVED".into());
                            println!("{name}: SoccerGoal team={team} {aabb}  {}", if sane { "OK" } else { "??" });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    println!("\nDiscGolfGoal: {disc} found, {disc_ok} with sane hole/par");
    println!("PaddleballGoal: {pad} found, {pad_ok} with sane team");
    println!("SoccerGoal: {soccer} found, {soccer_ok} with sane team + resolved goalCollider");
}

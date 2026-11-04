//! Rooms from the owner's install (skipped when there is none).
use metallic_rc_client::{room::{self, Library}, world::{self, Addresses}};

fn bundle_dir() -> Option<std::path::PathBuf> {
    let game = room::GAME_DIRS.iter().map(std::path::PathBuf::from).find(|p| p.exists());
    if game.is_none() {
        eprintln!("no install: skipped");
    }
    game.map(|g| room::bundle_dir(&g))
}

#[test]
fn a_group_and_a_single_address_load() {
    let Some(dir) = bundle_dir() else { return };
    let addresses = Addresses::open(&dir).unwrap();
    assert!(addresses.by_key.get("(Sandbox_Floor)").is_some_and(|l| l.resource_type == "UnityEngine.GameObject"));
    let mut lib = Library::new(&dir).unwrap();

    let r = world::load(&mut lib, &dir, Some("sandbox")).unwrap();
    assert_eq!(r.roots.len(), 6, "{:?}", r.roots);
    assert!(!r.parts.is_empty() && !r.colliders.is_empty());

    let r = world::load(&mut lib, &dir, Some("(Sandbox_GameTable)")).unwrap();
    assert_eq!(r.roots, ["(Sandbox_GameTable)"]);
    // recentred: the table's middle is over the origin
    let xs: Vec<f32> = r.parts.iter().flat_map(|p| p.positions.iter().map(|&v| p.world.transform_point3(v.into()).x)).collect();
    let mid = (xs.iter().cloned().fold(f32::MAX, f32::min) + xs.iter().cloned().fold(f32::MIN, f32::max)) / 2.0;
    assert!(mid.abs() < 0.01, "middle at {mid}");

    assert!(world::load(&mut lib, &dir, Some("no such address")).is_err());
}

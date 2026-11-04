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

#[test]
fn room_props_lights_and_materials() {
    let Some(dir) = bundle_dir() else { return };
    let mut lib = Library::new(&dir).unwrap();
    let r = world::load(&mut lib, &dir, Some("dorm")).unwrap();
    // the desk lamp carries RigidbodyPickup; walls carry only KinematicRigidbodyPickup
    let props: Vec<&str> = r.roots.iter().zip(&r.props).filter(|(_, p)| **p).map(|(n, _)| n.as_str()).collect();
    assert!(props.contains(&"(Dorm_Desk_Lamp)"), "{props:?}");
    assert!(!props.contains(&"(Dorm_Wall_1)"));
    assert_eq!(r.places.len(), r.roots.len());
    assert_eq!(r.collider_roots.len(), r.colliders.len());
    // the bunk bed's recolour mask is baked: brown wood, not the white texture
    let bed = r.materials.iter().find(|m| m.name.contains("BunkBed")).expect("bunk bed material");
    let px = &bed.texture.as_ref().unwrap().2;
    let mean = |c: usize| px.chunks_exact(4).map(|p| p[c] as u32).sum::<u32>() / (px.len() as u32 / 4);
    assert!(mean(0) > mean(2) + 20, "not brown: {} {} {}", mean(0), mean(1), mean(2));

    // the Maker room ceiling brings its own realtime lights (hemisphere point lights)
    let r = world::load(&mut lib, &dir, Some("sandbox")).unwrap();
    assert_eq!(r.lights.len(), 9);
    assert!(r.lights.iter().all(|l| l.kind == 2 && l.intensity > 0.0 && l.range > 0.0));
}

#[test]
fn prop_ids_are_stable() {
    assert_eq!(world::stable_id(&["dorm", "3", "(Dorm_Desk_Lamp)"]), world::stable_id(&["dorm", "3", "(Dorm_Desk_Lamp)"]));
    assert_ne!(world::stable_id(&["dorm", "3", "a"]), world::stable_id(&["dorm", "4", "a"]));
    assert_ne!(world::stable_id(&["ab", "c"]), world::stable_id(&["a", "bc"]));
}

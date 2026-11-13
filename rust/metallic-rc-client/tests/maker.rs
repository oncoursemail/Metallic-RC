//! The Maker Pen's saves against a real metallic-rc-server: the dorm's owner builds, saves and loads; others may
//! not until let in; a published room shows in the rooms list and matchmaking takes players there.
use bevy::prelude::Transform;
use metallic_rc_client::maker::{edit::ObjState, save};
use metallic_rc_client::net::{connect, Login};

fn start_server() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("mrc-maker-test-{}-{nanos}", std::process::id()));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let app = metallic_rc_server::App::open(&dir, &url).unwrap();
            tx.send(url).unwrap();
            axum::serve(listener, metallic_rc_server::router(app)).await.unwrap();
        })
    });
    rx.recv().unwrap()
}

fn link(s: &metallic_rc_client::net::Session, room: &str) -> save::Link {
    save::Link { api: s.service("API").unwrap().to_string(), token: s.access_token.clone(), room: room.into(), me: s.account.display_name.clone() }
}

fn cube(x: f32, uid: u32) -> ObjState {
    let mut at = Transform::from_xyz(x, 0.25, -2.0).with_scale(bevy::math::Vec3::splat(0.5));
    at.rotation = bevy::math::Quat::from_rotation_y(0.5);
    ObjState { address: format!("maker/cube/1/9#m{uid:x}"), at }
}

#[test]
fn saves_load_and_only_builders_save() {
    let url = start_server();
    let ada = connect(&url, &Login::Name("Ada".into())).unwrap();
    let mut bo = connect(&url, &Login::Name("Bo".into())).unwrap();
    let dorm = ada.room.photon_room.clone();
    let la = link(&ada, &dorm);

    let info = save::fetch(&la).unwrap();
    assert!(info.can_edit && info.objects.is_empty() && info.version == 0);
    assert_eq!(info.owner.as_deref(), Some("Ada"));
    let objects = vec![cube(1.0, 0xa1), ObjState { address: "(Hay_Bale_Sandbox)Variant#m7".into(), at: Transform::from_xyz(0.0, 0.0, -3.0) }];
    assert_eq!(save::save(&la, Some(0), &objects), Ok(1));
    let back = save::fetch(&la).unwrap();
    assert_eq!((back.version, back.objects.len()), (1, 2));
    assert_eq!(back.objects[0].address, objects[0].address);
    assert!(back.objects[0].at.translation.distance(objects[0].at.translation) < 1e-5);
    assert!(back.objects[0].at.rotation.angle_between(objects[0].at.rotation) < 1e-4);
    // a stale version is refused with the saved one in the message
    assert!(save::save(&la, Some(0), &objects).unwrap_err().ends_with("version 1"));

    // Bo sees Ada's dorm as view only and cannot save there until she lets him
    let lb = link(&bo, &dorm);
    let seen = save::fetch(&lb).unwrap();
    assert!(!seen.can_edit && seen.objects.len() == 2);
    assert!(save::save(&lb, None, &[]).is_err());
    let ids = save::builders(&la, &["Bo".into()]).unwrap();
    assert_eq!(ids, vec![bo.account.id]);
    assert!(save::fetch(&lb).unwrap().can_edit);
    assert_eq!(save::save(&lb, None, &[cube(2.0, 3)]), Ok(3));
    assert!(save::builders(&lb, &["Bo".into()]).is_err());

    // a published room is Ada's, listed, and matchmaking goes there
    assert_eq!(save::publish(&la, "Ada's Fort", &objects).unwrap(), "Ada's Fort");
    assert!(save::publish(&lb, "Ada's Fort", &[]).is_err());
    let r = bo.goto("Ada's Fort").unwrap().clone();
    assert_eq!(r.photon_room, "made-ada-s-fort");
    let fort = save::fetch(&link(&bo, &r.photon_room)).unwrap();
    assert!(!fort.can_edit && fort.objects.len() == 2 && fort.owner.as_deref() == Some("Ada"));
    let rooms: serde_json::Value = ureq::get(&format!("{}/rooms/hot", ada.service("Rooms").unwrap())).call().unwrap().body_mut().read_json().unwrap();
    assert!(rooms["Results"].as_array().unwrap().iter().any(|r| r["Name"] == "Ada's Fort"));
}

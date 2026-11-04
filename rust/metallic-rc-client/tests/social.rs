//! The social client against a real metallic-rc-server on a free local port: two accounts friend each other, party
//! up, join each other, the server restarts on the same data folder and everything saved is still there; then the
//! Bevy plugin fills its resources and follows a party.
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bevy::prelude::*;
use metallic_rc_client::{
    avatar::{Accessory, LocalLook, Look},
    net::{connect, Login},
    social::{self, Client, FollowRoom, MyParty, MyProfile, NoticeKind, Notifications, Pushes, ProfileUpdate, Relation, SocialNotice},
};

/// A server on `dir` in its own thread; dropping the sender stops it.
fn start_server(dir: &Path) -> (String, std::sync::mpsc::Sender<()>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let app = metallic_rc_server::App::open(&dir, &url).unwrap();
            tx.send(url).unwrap();
            tokio::spawn(async move { axum::serve(listener, metallic_rc_server::router(app)).await.unwrap() });
        });
        let _ = stop_rx.recv();
        rt.shutdown_background();
    });
    (rx.recv().unwrap(), stop_tx)
}

fn temp_dir() -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("mrc-client-social-{}-{n}", std::process::id()))
}

/// Waits up to 5 s for a `MessageReceived` push of this `Kind` (others are skipped); its `Msg`.
fn wait_message(p: &mut Pushes, kind: &str) -> serde_json::Value {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Some((i, m)) = p.next().unwrap() {
            if i == "MessageReceived" && m["Kind"] == kind {
                return m;
            }
        }
    }
    panic!("no {kind} message within 5 s")
}

#[derive(Resource, Default)]
struct Seen {
    notices: Vec<social::Notification>,
    follows: Vec<metallic_rc_client::net::Room>,
}

fn collect(mut seen: ResMut<Seen>, mut n: MessageReader<SocialNotice>, mut f: MessageReader<FollowRoom>) {
    seen.notices.extend(n.read().map(|x| x.0.clone()));
    seen.follows.extend(f.read().map(|x| x.0.clone()));
}

/// Runs the app until `done` holds (at most 10 s).
fn run_until(app: &mut App, what: &str, done: impl Fn(&mut App) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        app.update();
        if done(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn two_players_friend_party_join_and_survive_a_restart() {
    let dir = temp_dir();
    let (url, stop) = start_server(&dir);
    let mut ann = connect(&url, &Login::Name("Ann".into())).unwrap();
    let mut bob = connect(&url, &Login::Name("Bob".into())).unwrap();
    let (ca, cb) = (Client::new(&ann).unwrap(), Client::new(&bob).unwrap());
    let mut pb = cb.pushes().unwrap();

    // friends
    ca.relationship("sendfriendrequest", bob.account.id).unwrap();
    assert_eq!(wait_message(&mut pb, "friendRequest")["FromPlayerId"], ann.account.id);
    assert_eq!(cb.friends().unwrap()[0].relation, Relation::RequestReceived);
    assert_eq!(cb.notifications().unwrap()[0].kind, NoticeKind::FriendRequest);
    cb.relationship("acceptfriendrequest", ann.account.id).unwrap();
    let f = ca.friends().unwrap();
    assert_eq!((f[0].relation, f[0].online), (Relation::Friend, true));
    let bob_room = f[0].room.clone().expect("Bob is in his dorm");
    assert_eq!((bob_room.name.as_str(), bob_room.private), ("DormRoom", true));
    // a private dorm cannot be joined by a friend outside the party
    assert_eq!(ca.join_friend(bob.account.id).unwrap_err().0, "RoomInstanceIsPrivate");

    // profile and look
    let halo = Look { accessory: Accessory::Halo, ..Look::for_name("Ann") };
    let p = ca.update_profile(&ProfileUpdate { bio: Some("hi from Ann".into()), look: Some(halo), ..Default::default() }).unwrap();
    assert_eq!((p.bio.as_str(), p.look), ("hi from Ann", Some(halo)));
    assert_eq!(cb.bio(ann.account.id).unwrap(), "hi from Ann");
    assert!(ca.update_profile(&ProfileUpdate { display_name: Some(String::new()), ..Default::default() }).is_err());
    assert_eq!(ca.inventory().unwrap().len(), 3);

    // party: Ann invites, Bob joins, Ann goes to the Rec Center and Bob follows into the same instance
    let party = ca.invite_to_party(bob.account.id).unwrap().unwrap();
    let invite = cb.notifications().unwrap().into_iter().find(|n| matches!(n.kind, NoticeKind::PartyInvite { .. })).unwrap();
    assert_eq!(invite.kind, NoticeKind::PartyInvite { party_id: party.id });
    let joined = cb.join_party(party.id).unwrap().unwrap();
    assert_eq!(joined.members, [ann.account.id, bob.account.id]);
    let rc = ann.goto("RecCenter").unwrap().clone();
    let follow = wait_message(&mut pb, "partyGoto");
    assert_eq!(follow["FromPlayerId"], ann.account.id);
    let there = cb.join_friend(ann.account.id).unwrap();
    assert_eq!(there.photon_room, rc.photon_room);

    // join each other: Ann goes to her dorm, Bob (party mate) can join her there; then Ann joins Bob
    let dorm = ann.goto("dormroom").unwrap().clone();
    assert_eq!(cb.join_friend(ann.account.id).unwrap().photon_room, dorm.photon_room);
    let rc = bob.goto("RecCenter").unwrap().clone();
    assert_eq!(ca.join_friend(bob.account.id).unwrap().photon_room, rc.photon_room);
    drop(pb);

    // restart the server on the same folder
    drop(stop);
    std::thread::sleep(Duration::from_millis(300));
    let (url, _stop) = start_server(&dir);
    let ann = connect(&url, &Login::Name("Ann".into())).unwrap();
    let bob = connect(&url, &Login::Name("Bob".into())).unwrap();
    let (ca, cb) = (Client::new(&ann).unwrap(), Client::new(&bob).unwrap());
    let f = ca.friends().unwrap();
    assert_eq!((f[0].account_id, f[0].relation), (bob.account.id, Relation::Friend));
    let p = cb.profile(Some(ann.account.id)).unwrap();
    assert_eq!((p.bio.as_str(), p.look), ("hi from Ann", Some(halo)), "the look is kept on the server");
    assert_eq!(ca.inventory().unwrap().len(), 3);
    assert!(cb.notifications().unwrap().iter().any(|n| n.kind == NoticeKind::FriendRequest));
    assert!(ca.party().unwrap().is_none(), "parties end with a restart");

    // the Bevy side: Ann's game logs in with the default look; the server's look replaces it
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(social::SocialPlugin)
        .insert_resource(LocalLook(Look::default()))
        .init_resource::<Seen>()
        .add_systems(Update, collect);
    social::attach(&ann);
    run_until(&mut app, "the profile", |a| a.world().resource::<MyProfile>().0.is_some());
    run_until(&mut app, "the saved look", |a| a.world().resource::<LocalLook>().0 == halo);
    assert_eq!(app.world().resource::<social::Friends>().0[0].relation, Relation::Friend);
    assert_eq!(app.world().resource::<social::Inventory>().0.len(), 3);

    // Bob invites Ann: a notice arrives; Ann joins through the plugin and follows Bob into his private dorm
    let party = cb.invite_to_party(ann.account.id).unwrap().unwrap();
    run_until(&mut app, "the party invite", |a| a.world().resource::<Seen>().notices.iter().any(|n| n.kind == NoticeKind::PartyInvite { party_id: party.id }));
    assert!(app.world().resource::<Notifications>().0.iter().any(|n| matches!(n.kind, NoticeKind::PartyInvite { .. })));
    app.world().resource::<social::Social>().join_party(party.id);
    run_until(&mut app, "following Bob", |a| !a.world().resource::<Seen>().follows.is_empty());
    assert_eq!(app.world().resource::<Seen>().follows[0].photon_room, bob.room.photon_room);
    run_until(&mut app, "the party", |a| a.world().resource::<MyParty>().0.as_ref().is_some_and(|p| p.members.len() == 2));

    // a look changed in the game is saved on the server
    let blue = Look { accessory: Accessory::Headphones, ..halo };
    app.world_mut().resource_mut::<LocalLook>().0 = blue;
    run_until(&mut app, "the saved look", |_| cb.profile(Some(ann.account.id)).unwrap().look == Some(blue));
    social::detach();
}

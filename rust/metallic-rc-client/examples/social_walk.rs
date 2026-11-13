//! Two headless players against a running metallic-rc-server, for checking the social backend by hand:
//!
//!   social_walk --server http://127.0.0.1:4300 setup   friend, profile + look, party, follow the leader, join each other
//!   (restart the server on the same --data-dir)
//!   social_walk --server http://127.0.0.1:4300 check   everything saved is still there; party up and join again
//!
//! `--names A,B` picks the two account names (default WalkAnn,WalkBob). Exits 1 at the first thing that is wrong.
use metallic_rc_client::{
    avatar::{Accessory, Look},
    net::{connect, Login, Session},
    social::{Client, NoticeKind, ProfileUpdate, Relation},
};

fn check(ok: bool, what: &str) {
    if ok {
        println!("ok    {what}");
    } else {
        println!("FAIL  {what}");
        std::process::exit(1);
    }
}

fn login(server: &str, name: &str) -> (Session, Client) {
    let s = connect(server, &Login::Name(name.into())).unwrap_or_else(|e| panic!("login {name}: {e}"));
    let c = Client::new(&s).unwrap();
    println!("      {name}: account {} in {} ({})", s.account.id, s.room.name, s.room.photon_room);
    (s, c)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut server, mut names, mut phase) = ("http://127.0.0.1:4300".to_string(), "WalkAnn,WalkBob".to_string(), String::new());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--server" => server = args.next().unwrap_or_default(),
            "--names" => names = args.next().unwrap_or_default(),
            p => phase = p.to_string(),
        }
    }
    let (an, bn) = names.split_once(',').expect("--names A,B");
    let look = Look { accessory: Accessory::Halo, ..Look::for_name(an) };
    let (mut ann, ca) = login(&server, an);
    let (mut bob, cb) = login(&server, bn);
    let (ida, idb) = (ann.account.id, bob.account.id);

    match phase.as_str() {
        "setup" => {
            ca.relationship("sendfriendrequest", idb).unwrap();
            check(cb.notifications().unwrap().iter().any(|n| n.kind == NoticeKind::FriendRequest && n.from == ida), "Bob has Ann's friend request");
            cb.relationship("acceptfriendrequest", ida).unwrap();
            let f = ca.friends().unwrap();
            check(f.iter().any(|f| f.account_id == idb && f.relation == Relation::Friend && f.online), "Ann sees Bob as an online friend");
            let p = ca.update_profile(&ProfileUpdate { bio: Some("walk-through bio".into()), look: Some(look), ..Default::default() }).unwrap();
            check(p.look == Some(look), "Ann's look (halo) is saved");
            let party = ca.invite_to_party(idb).unwrap().unwrap();
            let joined = cb.join_party(party.id).unwrap().unwrap();
            check(joined.members == [ida, idb], "Bob joined Ann's party");
            let rc = ann.goto("RecCenter").unwrap().clone();
            let there = cb.join_friend(ida).unwrap();
            check(there.photon_room == rc.photon_room, &format!("Bob followed Ann into {}", rc.photon_room));
            let dorm = bob.goto("dormroom").unwrap().clone();
            check(ca.join_friend(idb).unwrap().photon_room == dorm.photon_room, &format!("Ann joined Bob in his private dorm {} (party mates)", dorm.photon_room));
        }
        "check" => {
            let f = ca.friends().unwrap();
            check(f.iter().any(|f| f.account_id == idb && f.relation == Relation::Friend), "after the restart Ann and Bob are still friends");
            let p = cb.profile(Some(ida)).unwrap();
            check(p.look == Some(look) && p.bio == "walk-through bio", "Ann's look and bio are still saved");
            check(ca.inventory().unwrap().len() == 3, "Ann's inventory is still there");
            check(ca.party().unwrap().is_none(), "the party ended with the restart");
            check(ca.join_friend(idb).is_err(), "Bob's dorm is private to a friend outside his party");
            let party = cb.invite_to_party(ida).unwrap().unwrap();
            ca.join_party(party.id).unwrap();
            check(ca.join_friend(idb).unwrap().photon_room == bob.room.photon_room, "in a party again, Ann joins Bob's dorm");
            let rc = ann.goto("RecCenter").unwrap().clone();
            check(cb.join_friend(ida).unwrap().photon_room == rc.photon_room, "Bob joins Ann in the Rec Center");
            cb.leave_party().unwrap();
        }
        _ => {
            eprintln!("social_walk [--server URL] [--names A,B] setup|check");
            std::process::exit(2)
        }
    }
    println!("done: {phase}");
}

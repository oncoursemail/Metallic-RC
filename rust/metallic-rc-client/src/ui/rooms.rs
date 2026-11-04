//! The rooms browser's list: the server's rooms (`GET {Rooms}/rooms/hot`, RecNet's paged `{Results, TotalResults}`),
//! found through the name server as the login finds its services, with the login's token; and how many players are in
//! each (`GET /realtime/rooms` beside the name server, the room server's open rooms).
use std::time::Duration;

use serde_json::Value;

/// A room as the browser shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomInfo {
    pub name: String,
    pub description: String,
    pub room_id: i64,
    /// the scene the client loads for it (the entry's `scene`, else its first sub room's `UnitySceneId`); empty when the
    /// server names none
    pub scene: String,
    /// players in its public instance now; None when not known (no count from the room server, or a private room)
    pub players: Option<usize>,
}

/// The rooms of a `{Results: [room...]}` answer.
pub fn parse(v: &Value) -> Vec<RoomInfo> {
    let rooms = v.get("Results").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let text = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or("").to_string();
    rooms.iter().filter_map(|r| {
        let name = r.get("Name")?.as_str()?.to_string();
        let scene = [r.get("scene"), r.get("Scene"), r.pointer("/SubRooms/0/UnitySceneId")].into_iter().map(text).find(|s| !s.is_empty()).unwrap_or_default();
        Some(RoomInfo { name, description: text(r.get("Description")), room_id: r.get("RoomId").and_then(Value::as_i64).unwrap_or(0), scene, players: None })
    }).collect()
}

/// Puts the room server's counts (`[{room, players}]`) on the rooms. A public room's players meet in the room server's
/// `room-<RoomId>` (the server's matchmaking names it so); a private one (the dorm: one per player) gets no count.
/// INFERRED from metallic-rc-server's naming; a party in a private instance is not counted.
pub fn count(rooms: &mut [RoomInfo], live: &Value) {
    let live = live.as_array().map(Vec::as_slice).unwrap_or_default();
    for r in rooms.iter_mut() {
        let key = format!("room-{}", r.room_id);
        let n = live.iter().filter(|l| l.get("room").and_then(Value::as_str) == Some(&key)).filter_map(|l| l.get("players")?.as_u64()).sum::<u64>();
        r.players = (!r.name.eq_ignore_ascii_case("DormRoom")).then_some(n as usize);
    }
}

/// Asks the server behind `name_server` for its rooms, as the account holding `token` (empty: not signed in yet, then
/// the list goes without one and without counts).
pub fn fetch(name_server: &str, token: &str) -> Result<Vec<RoomInfo>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(5))).build().into();
    let get = |url: &str| -> Result<Value, String> {
        let mut req = agent.get(url);
        if !token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {token}"));
        }
        req.call().map_err(|e| format!("{url}: {e}"))?.body_mut().read_json::<Value>().map_err(|e| format!("{url}: {e}"))
    };
    let base = name_server.trim_end_matches('/');
    let services = get(&format!("{base}/?buildId=23191908"))?;
    let rooms = services.get("Rooms").and_then(Value::as_str).ok_or("the name server has no Rooms service")?;
    let mut list = parse(&get(&format!("{rooms}/rooms/hot?take=100"))?);
    if !token.is_empty() {
        match get(&format!("{base}/realtime/rooms")) {
            Ok(live) => count(&mut list, &live),
            Err(e) => eprintln!("rooms: player counts: {e}"),
        }
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_list_answer_gives_names_and_descriptions() {
        let v = serde_json::json!({ "Results": [
            { "RoomId": 1, "Name": "DormRoom", "Description": "Yours.", "SubRooms": [{ "UnitySceneId": "" }] },
            { "RoomId": 2, "Name": "RecCenter", "scene": "rec-center" },
            { "RoomId": 3 }], "TotalResults": 3 });
        let mut rooms = parse(&v);
        assert_eq!(rooms, [
            RoomInfo { name: "DormRoom".into(), description: "Yours.".into(), room_id: 1, ..Default::default() },
            RoomInfo { name: "RecCenter".into(), room_id: 2, scene: "rec-center".into(), ..Default::default() },
        ]);
        assert!(parse(&serde_json::json!({})).is_empty());
        // counts from the room server: RecCenter's room-2 has 3, the dorms are private (no count)
        count(&mut rooms, &serde_json::json!([{ "room": "room-2", "players": 3 }, { "room": "dorm-7", "players": 1 }]));
        assert_eq!((rooms[0].players, rooms[1].players), (None, Some(3)));
        count(&mut rooms, &serde_json::json!([]));
        assert_eq!(rooms[1].players, Some(0));
    }
}

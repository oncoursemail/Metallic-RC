//! The rooms browser's list: the server's rooms (`GET {Rooms}/rooms/hot`, RecNet's paged `{Results, TotalResults}`),
//! found through the name server as the login finds its services.
use std::time::Duration;

use serde_json::Value;

/// A room as the browser shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomInfo {
    pub name: String,
    pub description: String,
}

/// The rooms of a `{Results: [room...]}` answer.
pub fn parse(v: &Value) -> Vec<RoomInfo> {
    let rooms = v.get("Results").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    rooms.iter().filter_map(|r| {
        let name = r.get("Name")?.as_str()?.to_string();
        Some(RoomInfo { name, description: r.get("Description").and_then(Value::as_str).unwrap_or("").to_string() })
    }).collect()
}

/// Asks the server behind `name_server` for its rooms. No sign-in token: the menu does not hold the login's session,
/// and this server's list does not need one (RecNet's does; INFERRED gap for a real server).
pub fn fetch(name_server: &str) -> Result<Vec<RoomInfo>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(5))).build().into();
    let get = |url: &str| -> Result<Value, String> {
        agent.get(url).call().map_err(|e| format!("{url}: {e}"))?.body_mut().read_json::<Value>().map_err(|e| format!("{url}: {e}"))
    };
    let services = get(&format!("{}/?buildId=23191908", name_server.trim_end_matches('/')))?;
    let rooms = services.get("Rooms").and_then(Value::as_str).ok_or("the name server has no Rooms service")?;
    Ok(parse(&get(&format!("{rooms}/rooms/hot?take=100"))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_list_answer_gives_names_and_descriptions() {
        let v = serde_json::json!({ "Results": [{ "Name": "DormRoom", "Description": "Yours." }, { "Name": "RecCenter" }, { "RoomId": 3 }], "TotalResults": 3 });
        assert_eq!(parse(&v), [
            RoomInfo { name: "DormRoom".into(), description: "Yours.".into() },
            RoomInfo { name: "RecCenter".into(), description: String::new() },
        ]);
        assert!(parse(&serde_json::json!({})).is_empty());
    }
}

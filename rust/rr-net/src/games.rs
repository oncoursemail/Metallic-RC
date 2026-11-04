//! Built-in game modes (Paintball first): the messages between the room server, which runs the match (teams,
//! phases, timer, scores, flags, hits, bots), and the players' clients. NamedMessage 71 up and 72 down, carried on
//! the room connection like voice. The layout is ours (little-endian, the `events` writer): the stock game runs its
//! modes on the master client through game-rule circuits, which we do not have.
use crate::events::{In, Malformed, Out};

pub const TO_SERVER: u16 = 71;
pub const TO_CLIENT: u16 = 72;

pub const RED: u8 = 0;
pub const BLUE: u8 = 1;
/// `State::winner` while nobody has won (and for a draw)
pub const NOBODY: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    TeamBattle,
    CaptureTheFlag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Lobby,
    Countdown,
    Playing,
    RoundEnd,
    Results,
}

/// A place to stand: feet position and facing (radians about +Y, Bevy axes).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Spot {
    pub pos: [f32; 3],
    pub yaw: f32,
}

/// What the server needs to know of the map, read from the scene by the client that starts the match.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MapInfo {
    pub name: String,
    /// where players wait before and after a match
    pub lobby: Vec<Spot>,
    /// red's and blue's spawn points
    pub spawns: [Vec<Spot>; 2],
    /// each team's flag at home
    pub flags: [[f32; 3]; 2],
    /// where each team brings the other team's flag
    pub goals: [[f32; 3]; 2],
    /// the playing field (min, max), if the scene has one
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

/// Client to server.
#[derive(Clone, Debug, PartialEq)]
pub enum Up {
    /// start a match (any player in the room may, in the lobby): mode, score to win, round length
    Start { mode: Mode, target: u16, seconds: u16, map: MapInfo },
    /// back to the lobby
    Stop,
    /// add this many bots, shared out over the teams (0 removes them all)
    Bots { count: u8 },
    /// join a team (lobby only)
    Team { team: u8 },
    /// a paintball fired from `origin`; `life` is how long it flies before it meets the world (the client's view of
    /// the map: the server has no geometry)
    Fire { origin: [f32; 3], velocity: [f32; 3], life: f32 },
}

/// A bot as the server moves it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BotPose {
    pub name: String,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub vel: [f32; 3],
}

/// One player or bot of the match. Players are their actor number, bots negative ids.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Member {
    pub id: i32,
    pub team: u8,
    pub out: bool,
    pub kills: u16,
    pub outs: u16,
    pub caps: u16,
    pub bot: Option<BotPose>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Flag {
    pub at: [f32; 3],
    /// who carries it, 0 for nobody
    pub carrier: i32,
    pub home: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    pub mode: Mode,
    pub phase: Phase,
    /// seconds left in this phase (the round's time while playing)
    pub time_left: f32,
    pub scores: [u16; 2],
    pub target: u16,
    pub winner: u8,
    pub members: Vec<Member>,
    pub flags: [Flag; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    FlagTaken,
    FlagDropped,
    FlagReturned,
    Captured,
}

/// Server to client.
#[derive(Clone, Debug, PartialEq)]
pub enum Down {
    State(State),
    Map(MapInfo),
    /// someone fired (sent to the others, for drawing)
    Shot { shooter: i32, origin: [f32; 3], velocity: [f32; 3], life: f32 },
    /// `victim` is out, hit by `shooter` at `at`
    Hit { shooter: i32, victim: i32, at: [f32; 3] },
    /// `id` goes to `spot` (players move themselves there)
    Respawn { id: i32, spot: Spot },
    /// flag news: what happened, who did it, whose flag
    Note { kind: NoteKind, actor: i32, team: u8 },
}

fn v3(o: &mut Out, v: [f32; 3]) {
    v.iter().for_each(|x| o.f32(*x));
}

fn rv3(i: &mut In) -> Result<[f32; 3], Malformed> {
    Ok([i.f32()?, i.f32()?, i.f32()?])
}

fn spot(o: &mut Out, s: &Spot) {
    v3(o, s.pos);
    o.f32(s.yaw);
}

fn rspot(i: &mut In) -> Result<Spot, Malformed> {
    Ok(Spot { pos: rv3(i)?, yaw: i.f32()? })
}

fn spots(o: &mut Out, s: &[Spot]) {
    o.i32(s.len() as i32);
    s.iter().for_each(|s| spot(o, s));
}

fn rspots(i: &mut In) -> Result<Vec<Spot>, Malformed> {
    let n = i.count(16)?;
    (0..n).map(|_| rspot(i)).collect()
}

fn mode(m: Mode) -> u8 {
    m as u8
}

fn rmode(i: &mut In) -> Result<Mode, Malformed> {
    match i.u8()? {
        0 => Ok(Mode::TeamBattle),
        1 => Ok(Mode::CaptureTheFlag),
        _ => Err(Malformed),
    }
}

fn rteam(i: &mut In) -> Result<u8, Malformed> {
    match i.u8()? {
        t @ 0..=2 => Ok(t),
        _ => Err(Malformed),
    }
}

impl MapInfo {
    fn write(&self, o: &mut Out) {
        o.string(&self.name);
        spots(o, &self.lobby);
        spots(o, &self.spawns[0]);
        spots(o, &self.spawns[1]);
        self.flags.iter().chain(&self.goals).for_each(|f| v3(o, *f));
        o.bool(self.bounds.is_some());
        if let Some((lo, hi)) = self.bounds {
            v3(o, lo);
            v3(o, hi);
        }
    }

    fn read(i: &mut In) -> Result<MapInfo, Malformed> {
        let name = i.string()?;
        let lobby = rspots(i)?;
        let spawns = [rspots(i)?, rspots(i)?];
        let flags = [rv3(i)?, rv3(i)?];
        let goals = [rv3(i)?, rv3(i)?];
        let bounds = if i.bool()? { Some((rv3(i)?, rv3(i)?)) } else { None };
        Ok(MapInfo { name, lobby, spawns, flags, goals, bounds })
    }
}

impl Up {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Up::Start { mode: m, target, seconds, map } => {
                o.u8(0);
                o.u8(mode(*m));
                o.i16(*target as i16);
                o.i16(*seconds as i16);
                map.write(&mut o);
            }
            Up::Stop => o.u8(1),
            Up::Bots { count } => {
                o.u8(2);
                o.u8(*count);
            }
            Up::Team { team } => {
                o.u8(3);
                o.u8(*team);
            }
            Up::Fire { origin, velocity, life } => {
                o.u8(4);
                v3(&mut o, *origin);
                v3(&mut o, *velocity);
                o.f32(*life);
            }
        }
        o.0
    }

    pub fn decode(b: &[u8]) -> Result<Up, Malformed> {
        let mut i = In::new(b);
        Ok(match i.u8()? {
            0 => Up::Start { mode: rmode(&mut i)?, target: i.i16()? as u16, seconds: i.i16()? as u16, map: MapInfo::read(&mut i)? },
            1 => Up::Stop,
            2 => Up::Bots { count: i.u8()? },
            3 => Up::Team { team: rteam(&mut i)? },
            4 => Up::Fire { origin: rv3(&mut i)?, velocity: rv3(&mut i)?, life: i.f32()? },
            _ => return Err(Malformed),
        })
    }
}

impl Down {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out::default();
        match self {
            Down::State(s) => {
                o.u8(0);
                o.u8(mode(s.mode));
                o.u8(s.phase as u8);
                o.f32(s.time_left);
                o.i16(s.scores[0] as i16);
                o.i16(s.scores[1] as i16);
                o.i16(s.target as i16);
                o.u8(s.winner);
                o.i32(s.members.len() as i32);
                for m in &s.members {
                    o.i32(m.id);
                    o.u8(m.team);
                    o.bool(m.out);
                    o.i16(m.kills as i16);
                    o.i16(m.outs as i16);
                    o.i16(m.caps as i16);
                    o.bool(m.bot.is_some());
                    if let Some(b) = &m.bot {
                        o.string(&b.name);
                        v3(&mut o, b.pos);
                        o.f32(b.yaw);
                        v3(&mut o, b.vel);
                    }
                }
                for f in &s.flags {
                    v3(&mut o, f.at);
                    o.i32(f.carrier);
                    o.bool(f.home);
                }
            }
            Down::Map(m) => {
                o.u8(1);
                m.write(&mut o);
            }
            Down::Shot { shooter, origin, velocity, life } => {
                o.u8(2);
                o.i32(*shooter);
                v3(&mut o, *origin);
                v3(&mut o, *velocity);
                o.f32(*life);
            }
            Down::Hit { shooter, victim, at } => {
                o.u8(3);
                o.i32(*shooter);
                o.i32(*victim);
                v3(&mut o, *at);
            }
            Down::Respawn { id, spot: s } => {
                o.u8(4);
                o.i32(*id);
                spot(&mut o, s);
            }
            Down::Note { kind, actor, team } => {
                o.u8(5);
                o.u8(*kind as u8);
                o.i32(*actor);
                o.u8(*team);
            }
        }
        o.0
    }

    pub fn decode(b: &[u8]) -> Result<Down, Malformed> {
        let mut i = In::new(b);
        Ok(match i.u8()? {
            0 => {
                let mode = rmode(&mut i)?;
                let phase = match i.u8()? {
                    0 => Phase::Lobby,
                    1 => Phase::Countdown,
                    2 => Phase::Playing,
                    3 => Phase::RoundEnd,
                    4 => Phase::Results,
                    _ => return Err(Malformed),
                };
                let time_left = i.f32()?;
                let scores = [i.i16()? as u16, i.i16()? as u16];
                let target = i.i16()? as u16;
                let winner = rteam(&mut i)?;
                let n = i.count(13)?;
                let mut members = Vec::with_capacity(n);
                for _ in 0..n {
                    let (id, team, out) = (i.i32()?, rteam(&mut i)?, i.bool()?);
                    let (kills, outs, caps) = (i.i16()? as u16, i.i16()? as u16, i.i16()? as u16);
                    let bot = if i.bool()? { Some(BotPose { name: i.string()?, pos: rv3(&mut i)?, yaw: i.f32()?, vel: rv3(&mut i)? }) } else { None };
                    members.push(Member { id, team, out, kills, outs, caps, bot });
                }
                let mut flags = [Flag::default(); 2];
                for f in &mut flags {
                    *f = Flag { at: rv3(&mut i)?, carrier: i.i32()?, home: i.bool()? };
                }
                Down::State(State { mode, phase, time_left, scores, target, winner, members, flags })
            }
            1 => Down::Map(MapInfo::read(&mut i)?),
            2 => Down::Shot { shooter: i.i32()?, origin: rv3(&mut i)?, velocity: rv3(&mut i)?, life: i.f32()? },
            3 => Down::Hit { shooter: i.i32()?, victim: i.i32()?, at: rv3(&mut i)? },
            4 => Down::Respawn { id: i.i32()?, spot: rspot(&mut i)? },
            5 => {
                let kind = match i.u8()? {
                    0 => NoteKind::FlagTaken,
                    1 => NoteKind::FlagDropped,
                    2 => NoteKind::FlagReturned,
                    3 => NoteKind::Captured,
                    _ => return Err(Malformed),
                };
                Down::Note { kind, actor: i.i32()?, team: rteam(&mut i)? }
            }
            _ => return Err(Malformed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> MapInfo {
        MapInfo {
            name: "level13".into(),
            lobby: vec![Spot { pos: [1.0, 0.0, 2.0], yaw: 0.5 }],
            spawns: [vec![Spot { pos: [0.0, 0.0, -50.0], yaw: 0.0 }], vec![Spot { pos: [0.0, 0.0, 50.0], yaw: 3.1 }, Spot::default()]],
            flags: [[0.0, 1.0, -49.7], [0.0, 1.0, 49.9]],
            goals: [[0.0, 0.8, -49.7], [0.0, 0.8, 49.9]],
            bounds: Some(([-40.0, -5.0, -60.0], [40.0, 30.0, 60.0])),
        }
    }

    #[test]
    fn up_round_trips() {
        for m in [
            Up::Start { mode: Mode::CaptureTheFlag, target: 3, seconds: 600, map: map() },
            Up::Stop,
            Up::Bots { count: 4 },
            Up::Team { team: BLUE },
            Up::Fire { origin: [1.0, 2.0, 3.0], velocity: [0.0, 1.0, -33.0], life: 0.7 },
        ] {
            assert_eq!(Up::decode(&m.encode()), Ok(m));
        }
    }

    #[test]
    fn down_round_trips() {
        let state = State {
            mode: Mode::TeamBattle,
            phase: Phase::Playing,
            time_left: 123.5,
            scores: [4, 7],
            target: 15,
            winner: NOBODY,
            members: vec![
                Member { id: 2, team: RED, out: true, kills: 3, outs: 1, caps: 0, bot: None },
                Member { id: -1, team: BLUE, out: false, kills: 0, outs: 2, caps: 1, bot: Some(BotPose { name: "Bot 1".into(), pos: [1.0, 0.0, 1.0], yaw: 1.0, vel: [3.0, 0.0, 0.0] }) },
            ],
            flags: [Flag { at: [0.0, 1.0, 0.0], carrier: 0, home: true }, Flag { at: [0.0, 2.0, 5.0], carrier: -1, home: false }],
        };
        for m in [
            Down::State(state),
            Down::Map(map()),
            Down::Shot { shooter: 3, origin: [0.0; 3], velocity: [1.0, 2.0, 3.0], life: 2.0 },
            Down::Hit { shooter: -2, victim: 3, at: [0.5, 1.2, 0.0] },
            Down::Respawn { id: 3, spot: Spot { pos: [1.0, 0.0, 1.0], yaw: 2.0 } },
            Down::Note { kind: NoteKind::Captured, actor: 3, team: RED },
        ] {
            assert_eq!(Down::decode(&m.encode()), Ok(m));
        }
    }

    #[test]
    fn junk_is_refused() {
        assert!(Up::decode(&[]).is_err());
        assert!(Up::decode(&[9]).is_err());
        assert!(Down::decode(&[0, 1, 2]).is_err());
        assert!(Down::decode(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0x7f]).is_err());
    }
}

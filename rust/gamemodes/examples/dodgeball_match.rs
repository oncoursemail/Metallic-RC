//! Runs a full Dodgeball match end-to-end through the ported `DodgeballManager`, printing a play-by-play — the first
//! built-in RR Original "running" at the rules layer (the visual/networked client integration — ball throw physics,
//! collision→hit, elimination visuals over rr-net — is the next step; this proves the round/match logic is 1:1).
//! `cargo run -p gamemodes --example dodgeball_match`

use gamemodes::dodgeball::{DodgeballConfig, DodgeballEffects, DodgeballManager, Phase, PlayerId, Team};

fn team_name(t: Team) -> &'static str {
    match t {
        0 => "RED",
        1 => "BLUE",
        _ => "T?",
    }
}

/// Announces the manager's effects as a match commentary.
struct Announce;
impl DodgeballEffects for Announce {
    fn round_started(&mut self, r: u32) {
        println!("  — Round {r}: FIGHT!");
    }
    fn round_over(&mut self, w: Option<Team>) {
        match w {
            Some(t) => println!("  — Round over — {} takes it.", team_name(t)),
            None => println!("  — Round over — a draw."),
        }
    }
    fn match_over(&mut self, w: Team) {
        println!("\n*** MATCH OVER — {} WINS ***", team_name(w));
    }
    fn player_out(&mut self, p: PlayerId) {
        println!("      player {p} is OUT");
    }
    fn score_changed(&mut self, t: Team, s: u32) {
        println!("      {} round-wins: {s}", team_name(t));
    }
}

/// A scripted ball event (the ball gadget would raise these from real throws/collisions).
enum Ev {
    Hit(PlayerId, PlayerId),   // (victim, thrower)
    Catch(PlayerId, PlayerId), // (catcher, thrower)
}

fn main() {
    let cfg = DodgeballConfig { score_to_win: 2, pre_round_duration: 1.0, post_round_duration: 1.0 };
    let mut m = DodgeballManager::new(cfg);
    for p in [1, 2, 3] {
        m.add_player(p, 0); // RED
    }
    for p in [4, 5, 6] {
        m.add_player(p, 1); // BLUE
    }
    let mut fx = Announce;
    println!("Dodgeball — RED (1,2,3) vs BLUE (4,5,6), first to 2 round-wins.\n");

    // one script of ball events per round (fired one per tick while the round is running)
    let scripts: Vec<Vec<Ev>> = vec![
        // round 1: BLUE catches one of RED's throws (1 is out), but RED still wipes BLUE
        vec![Ev::Hit(5, 2), Ev::Catch(4, 1), Ev::Hit(4, 3), Ev::Hit(6, 2)],
        // round 2: RED wipes BLUE again -> reaches 2 round-wins -> match
        vec![Ev::Hit(4, 1), Ev::Hit(5, 2), Ev::Hit(6, 3)],
    ];

    let dt = 0.25;
    let mut elapsed = 0.0;
    let mut cur_round = 0u32;
    let mut fired = 0usize;
    loop {
        m.tick(dt, &mut fx);
        if m.round != cur_round {
            cur_round = m.round;
            fired = 0;
        }
        if m.is_round_running() {
            let idx = (m.round - 1) as usize;
            if idx < scripts.len() && fired < scripts[idx].len() {
                match &scripts[idx][fired] {
                    Ev::Hit(v, th) => {
                        println!("    player {th} throws — hits player {v}");
                        m.on_player_hit(*v, *th, &mut fx);
                    }
                    Ev::Catch(c, th) => {
                        println!("    player {th} throws — CAUGHT by player {c}!");
                        m.on_catch(*c, *th, &mut fx);
                    }
                }
                fired += 1;
            }
        }
        if matches!(m.phase, Phase::GameOver(_)) {
            break;
        }
        elapsed += dt;
        if elapsed > 120.0 {
            println!("(timeout — match did not resolve)");
            break;
        }
    }
}

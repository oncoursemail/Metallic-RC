# Adding a built-in Rec Room Original — the recipe

Three games are ported 1:1 and playable in-client over rr-net through one reusable seam: **Dodgeball**
(team elimination), **Paddleball** (team goals to 5), **DiscGolf** (per-player stroke play). The seam has
handled three genuinely different scoring shapes with no structural change, so it is now **frozen** (see
"The frozen API" below). Every remaining RR Original is a small, well-defined delta. Follow these steps
verbatim; hold the 1:1 bar (no invented numbers — mark anything unverified `// 1:1:` and get the real value
from the decompile lane).

## The recipe (per game)

1. **Get the real bodies.** Message the decompile lane with the game's `GameRuleManager` class name (e.g.
   `PaintballManager @dump.cs:NNNN`). It returns the verified method bodies + config numbers + the
   gadget contract (what raises which `RpcMasterRequestX`) + the corpus scene name (`levelN`). Do NOT guess
   rules or numbers; wait for the bodies.

2. **Port the rules → a `GameMode`** in `gamemodes/src/<game>.rs`. The base `GameRuleManager` lifecycle
   (round state, round clock, killzone, net-singleton) is shared — your mode only implements the scoring +
   win check. Pick the closest existing mode as a template:
   - elimination / "players out" → `dodgeball.rs` (`on_player_hit` == the real `IsValidHit`)
   - team points to a target → `paddleball.rs` (`GameInput::Goal` → `AddScore` → win at N)
   - per-player scoring (strokes/holes/individual) → `discgolf.rs` (`PlayerScore`/`MatchOver`)
   Add a unit test that plays a round/match to a win. Mark every rule a clean body would pin down `// 1:1:`.
   Re-use `GameEvent::Score`/`Round` (team games) or `PlayerScore`/`MatchOver` (per-player games); add a new
   `GameInput`/`GameEvent` variant only if the game genuinely needs one (then update the other modes' `input`
   match — a catch-all `_ => {}` is fine).

3. **Wire the client plugin** in `metallic-rc-client/src/<game>.rs`, adapting `paddleball.rs` (team) or
   `discgolf.rs` (per-player). Boilerplate that is **pure reuse** (copy it): registering the fixed
   game-authority scene object for RPCs, `net_spawn` of the game's props, the `recv_rpcs` request/event
   decode, and `master_tick` (drain pending requests + the self-test bots → `mode.input` → broadcast the
   events). The only new code is: which prop addresses, which request(s) the gadget raises, and the
   gadget/trigger detection. Add `<Game>Active(bool)` + `<Game>Plugin`, and wire both into `lib.rs` +
   `main.rs` (activate on `--world <game>`).

4. **Read scene-specific config** through `levels::scene_objects(game_data, room, needle)` (in `levels.rs`):
   it returns the scene's objects whose hierarchy NAME contains `needle` (case-insensitive), each with its
   world position + box/sphere collider half-extent — enough to place goal/basket/hazard zones and spawns from
   the real scene instead of approximating. Discover a game's gadget names first with `needle = "*"` (lists all
   scripted objects) — e.g. run `METALLIC_OBJECTS=* levels_report <room> --game-dir <RecRoom>`.
   KNOWN LIMITS (maps-lane follow-ups; mark affected values `// 1:1:` until closed):
   - Matching is by NAME, not MonoBehaviour script class: the 2026 scenes reference gameplay MonoScripts
     EXTERNALLY, so `mono_script_name` resolves none from the scene file alone — resolving those external PPtrs
     is maps-lane work. (The reader is proven: `*` finds 1410 scripted objects in `discgolf`.)
   - Serialized FIELDS (a goal's `hole`/`par`/`team`) need the MonoBehaviour byte layout (no type trees) — not
     read here. Coordinate with the maps session, which parses scene byte-layout.
   - Some gadgets (e.g. the Paddleball goals) may be instantiated by the `*GameRuleWrapper` prefab at runtime
     rather than baked into the scene; if `scene_objects` doesn't find them by name, read them from the wrapper
     prefab (via `world::Addresses`/the bundle) instead.

5. **Port any non-ballistic prop physics** client-side if the game has them (DiscGolf's disc flight model:
   `airLift` + throw-assist curve + spin — see `discgolf.rs::disc_flight`). Throw speed is always the hand
   release velocity (physics-authentic) — never a scripted constant.

6. **Reconcile every `// 1:1:`** against the decompiled body the moment it lands, and correct any drift. A
   game is not "done" until its rules are verified against the real body, not just plausible.

7. **Self-test, muted, mic off.** Run `--world <game>` with `METALLIC_NO_MIC=1` (never open the mic) +
   `METALLIC_DEMO_CAM=1` (bounds-framed demo cam) + the game's `METALLIC_<X>_BOTS=1`, against a local
   `metallic-rc-server --open-rooms`. Confirm the log shows the authority scoring to a win over the net.
   Commit + push; ping the integrator when it runs a round.

## The frozen API (`gamemodes::mode`)

```
trait GameMode { name; add_player(id)->Team; remove_player(id); input(GameInput, &mut Vec<GameEvent>); tick(dt, &mut Vec<GameEvent>); }
enum GameInput  { Hit{thrower,victim}, Catch{catcher,thrower},  // Dodgeball
                  Goal{team},                                     // Paddleball / team-goal games
                  Throw{player}, Basket{player}, Hazard{player} } // DiscGolf / per-player games
enum GameEvent  { PlayerOut(p), PlayerRespawned(p), Score{team,score}, Round{round,phase,winner},  // team
                  PlayerScore{player,score:i32}, MatchOver{winner},                                 // per-player
                  Cue(String) }
```

The master client is the room authority (Rec Room's model: `GameRuleManager` is a NetworkedSingleton on the
master). Clients send **requests** (what a gadget observed) as RPCs to Master; the master runs the `GameMode`
(the only authoritative scorer) and broadcasts **events** to Others. No outcome is client-trusted. Moving
authority to a dedicated server later is a drop-in (the `GameMode` is engine-agnostic; feed it the same
requests on the server). Add a variant to these enums only when a new game truly needs it; the three shapes
above already cover elimination, team-points, and per-player scoring.

## Roster (remaining `*GameRuleManager` @dump.cs)

Charades @969 · DiscGolf ✅ @2570 · Dodgeball ✅ @5625 · Paddleball ✅ @10788 · Paintball @13813 (5 variants,
heavy) · Soccer @25313 · BattleRoyale @947469 · RecRally @949803 · Quest @968004 (per-player) · Bowling
@980135 · Arena @983309. Each is a drop-in via this recipe; give decompile the class name to start.

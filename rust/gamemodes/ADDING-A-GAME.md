# Adding a built-in Rec Room Original — the recipe

Four games are ported 1:1 and playable in-client over rr-net through one reusable seam: **Dodgeball**
(team elimination), **Paddleball** (team goals to 5), **DiscGolf** (per-player stroke play), **Soccer**
(team goals but **TIMED** — 180 s, most goals wins). The seam has handled four genuinely different scoring
shapes with no structural change (the timed win lives inside the mode's `tick`, not a new enum variant), so
it is **frozen** (see "The frozen API" below). Every remaining RR Original is a small, well-defined delta.
Follow these steps verbatim; hold the 1:1 bar (no invented numbers — mark anything unverified `// 1:1:` and
get the real value from the decompile lane).

Watch for the win CONDITION, not just the scoring: Paddleball is first-to-N, Soccer is most-goals-when-the-
clock-expires. Ask the decompile lane "goals-to-win or timed?" up front — they read very differently.

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

4. **Read scene-specific config — REAL values, no stand-ins.** Two readers in `levels.rs`:
   - `levels::scene_goals(game_data, room, class, goal_collider_first)` — resolves gadget objects by
     MonoBehaviour CLASS (`mono_script_ref` → `mono_script_name`, across sibling files) and reads their
     serialized fields with `unity_assets::level::MonoFields`, returning each goal's `team` + the world-space
     AABB of the trigger `BoxCollider` it references. This is how Soccer gets its real goal teams + AABBs
     (SoccerGoal = goalCollider PPtr then team → `goal_collider_first = true`). Give the decompile lane the
     gadget class name; it returns the serialized FIELD ORDER (base-first, Unity declaration order) so you read
     the right scalars. For a prop ADDRESS (the ball), find the real Addressables key with the `world_inventory`
     example (`cargo run --example world_inventory -- "" <filter>`) — e.g. Soccer's ball is `(SoccerBall)`.
   - `levels::scene_objects(game_data, room, needle)` — NAME match (case-insensitive; `*` = all scripted), each
     with world position + collider half-extent, for discovery / spawns. Discover gadget names with
     `METALLIC_OBJECTS=* levels_report <room> --game-dir <RecRoom>`.
   The MonoFields reader (maps lane, merged) CLOSED the old "can't read script class / serialized fields"
   limit — validated on the corpus (DiscGolfGoal 18/18, PaddleballGoal 2/2, SoccerGoal 2/2). For a gadget the
   `*GameRuleWrapper` prefab instantiates at runtime rather than baking into the scene, read it from the wrapper
   prefab (via `world::Addresses`/the bundle) instead. The decompile lane's `rr-mb-dump <scene> <Class>` dumps a
   class's leading serialized words to confirm a field layout against the real bytes.

5. **Port any non-ballistic prop physics** client-side if the game has them (DiscGolf's disc flight model:
   `airLift` + throw-assist curve + spin — see `discgolf.rs::disc_flight`). Throw speed is always the hand
   release velocity (physics-authentic) — never a scripted constant.

6. **Reconcile every `// 1:1:`** against the decompiled body the moment it lands, and correct any drift. A
   game is not "done" until its rules are verified against the real body, not just plausible.

7. **Self-test, muted, mic off, no window.** Run `--world <game>` with `METALLIC_NO_MIC=1` (never open the
   mic — owner's hard constraint) + `METALLIC_HIDDEN=1` (create the window but don't show it — the owner is
   often on a call; the ECS/net/rules run the same, you just read the log) + the game's `METALLIC_<X>_BOTS=1`,
   against a local `metallic-rc-server --open-rooms`. Mute audio output too: point `LOCALAPPDATA` at a temp dir
   holding `Metallic-RC/settings.json` = `{"settings":{"volume_master":0}}`. The scene loads from the corpus
   (first load ~20 s, then cached), so give the run ~45 s (the self-test match clock starts only once the arena
   is built and `join_game` fires). Confirm the log shows: the real gadget config read (e.g. "read 2 real
   SoccerGoal(s)"), the prop spawned (no "no GameObject address" error), and the authority scoring to a win/full
   time over the net. (`METALLIC_DEMO_CAM=1` + `METALLIC_SHOT=<png>` + `METALLIC_SHOT_AT=<s>` instead captures a
   framed screenshot and exits, if you want a visual — that one does open a window.) Commit + push to `unit`;
   ping the integrator when it runs a round.

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

Dodgeball ✅ · Paddleball ✅ · DiscGolf ✅ · Soccer ✅ (TIMED team goals). Remaining: Bowling
(BowlingManager, level91 — per-player pins/frames, ~DiscGolf shape; **do next**, lean) · Paintball
(PaintballManager, level14+4 maps, 2 modes CTF/TeamBattle) · Charades (CharadesManager, level8, timed
word-guess, 2 modes) · RecRally (racing) + StuntRunner (time-trial, natural pair) · Arena (horde+shop, heavy)
· BattleRoyale (level78 HUGE 172k objs, shrinking killzone — isolate) · Quest (co-op campaign, many scenes —
isolate). **LaserTag is SPECIAL** — no `*Manager` subclass, only `LaserTagGameRuleWrapper`; it's
gadget/circuit-based, so route it to the CircuitsV2/gadget lane, NOT this manager-body recipe. Each manager
game is a drop-in via this recipe; give decompile the class name to start.

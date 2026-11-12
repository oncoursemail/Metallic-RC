# Adding a built-in Rec Room Original — the recipe

Six games are ported 1:1 over one reusable seam: **Dodgeball** (team elimination), **Paddleball** (team goals
to 5), **DiscGolf** (per-player stroke play), **Soccer** (team goals, TIMED — 180 s, most goals wins), **Bowling**
(per-player REAL 10-pin frame scoring), **Charades** (draw-and-guess, CoOp 480 s shared-score + TeamLightning
4-round team-score). The seam has handled six scoring shapes with only two new `GameInput` variants (`Bowl`,
`Guess`); the timers/roles/win-conditions live inside each mode, so it is **frozen** (see
"The frozen API" below). Every remaining RR Original is a small, well-defined delta. Follow these steps verbatim;
hold the 1:1 bar — NO invented numbers. Mark anything unverified `// 1:1:`, get the real value from the decompile
lane (or the memory dump for runtime-only values), and make missing real config a HARD-FAIL (refuse to run with a
clear error), never a silent approximation. Watch the win CONDITION, not just scoring: first-to-N (Paddleball),
most-goals-on-the-clock (Soccer), highest-frame-total (Bowling) read very differently — ask decompile up front.

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
   - `levels::scene_class_positions(game_data, room, class)` — the world positions of every scene object of a
     MonoBehaviour CLASS (same class resolution). This is how Bowling reads its real pin layout
     (`scene_class_positions(_, "bowlingalley", "BowlingPin")` → the 33 real `BowlingPin` placements in level91, the
     4-3-2-1 ten-pin triangle per lane). Verify with `METALLIC_CLASS=<Class> levels_report <room>`.
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
   (first load ~20 s for a small scene, ~45 s for a big one like level91, then cached) and `join_game` may re-read
   it for a class lookup, so give a big-scene run ~120 s+ (the self-test starts only once the arena is built and
   `join_game` fires). Confirm the log shows: the real gadget config read (e.g. "read 2 real SoccerGoal(s)" /
   "read 33 real BowlingPins"), the prop spawned (no "no GameObject address" error), and the authority scoring to
   a win/full time over the net. (`METALLIC_DEMO_CAM=1` + `METALLIC_SHOT=<png>` + `METALLIC_SHOT_AT=<s>` instead captures a
   framed screenshot and exits, if you want a visual — that one does open a window.) Commit + push to `unit`;
   ping the integrator when it runs a round.

## The frozen API (`gamemodes::mode`)

```
trait GameMode { name; add_player(id)->Team; remove_player(id); input(GameInput, &mut Vec<GameEvent>); tick(dt, &mut Vec<GameEvent>); }
enum GameInput  { Hit{thrower,victim}, Catch{catcher,thrower},  // Dodgeball
                  Goal{team},                                     // Paddleball / Soccer / team-goal games
                  Throw{player}, Basket{player}, Hazard{player},  // DiscGolf / per-player games
                  Bowl{player,pins} }                             // Bowling (pins = physics pinfall, 0..=10)
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

Dodgeball ✅ · Paddleball ✅ · DiscGolf ✅ · Soccer ✅ (TIMED) · Bowling ✅ (real 10-pin + real scene config over
net; **pin-toppling physics is the one open dependency** — see below) · Charades ✅ (CoOp + TeamLightning both over
net; pen DRAWING is presentation depth, a follow-up) · StuntRunner ✅ (6-course time-trial, real formula + base
score 180000 + real course gates, winner verified over net; two world-depth follow-ups — the multi-course scene
SWAP (loader doesn't hot-swap level93–98 yet) and the real gate-crossing TIMING; `CourseMaxFinishTimeSeconds` cap
is a live-capture //1:1-pending and the SS/S/A/B/C grades are prefab percentiles //1:1-pending — all cap/grade
items are winner-independent). Paintball — RULES ✅ (CTF 3-captures + TeamBattle 50-tags, both timed 600s + score cap, flag state machine,
master-validated tags; 1:1 from the real wrapper numbers; client vertical needs the paintball-projectile + flag
gadgets, a dynamic-physics item). Remaining: RecRally (RecRallyManager — KART racing, needs the dynamic-physics foundation) · Arena
(horde+shop+AI, heavy) · BattleRoyale (level78 HUGE 172k objs, shrinking killzone — isolate) · Quest (co-op
campaign + AI, many scenes — isolate). **LaserTag is SPECIAL** — no `*Manager` subclass, only
`LaserTagGameRuleWrapper`; gadget/circuit-based, route it to the CircuitsV2/gadget lane, NOT this manager-body
recipe. The remaining games converge on the dynamic-physics foundation (Bowling toppling + RecRally karts +
Paintball projectiles) — that reusable `bevy_rapier` layer is the next big enabler. Give the decompile session
(local_9442e322) the manager class name to start each.

**Pin-toppling physics dependency.** Bowling's real pinfall is rigidbody TOPPLING (ball knocks pins, pin-to-pin
chains, settle; a pin is standing while `AngleFromUpright <= 25.0°`). The rebuild's custom physics (interact.rs
`Body`) only bounces box props — it doesn't topple. The planned home is a REUSABLE dynamic-prop foundation
(evaluate `bevy_rapier` for PhysX-matching rigidbody dynamics) that BattleRoyale / RecRally karts will also use,
with the 25° IsUpright check (`bowling::pin_is_standing`) + `pinfall_from_physics` on top. Until it lands, the
Bowling self-test injects bowls (`METALLIC_BW_BOTS=1`); the scoring + scene config are real and verified.

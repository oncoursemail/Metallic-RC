extends SceneTree
## Run with the official Godot editor binary, --headless --path game/native --script res://tests/test_simulation.gd.

const SIM = preload("res://scripts/simulation.gd")
const WORLD = preload("res://scripts/world_data.gd")
const SESSION_SCRIPT = preload("res://scripts/session.gd")
var _failures: Array[String] = []
var _checks: int = 0

func _initialize() -> void:
	_test_names_and_capacity()
	_test_input_authority()
	_test_movement_and_collision()
	_test_resume()
	_test_shared_round()
	_test_persistence_validation()
	_test_wire_snapshot()
	print("METALLIC_SIMULATION_TESTS " + JSON.stringify({"checks": _checks, "failures": _failures, "ok": _failures.is_empty()}))
	quit(0 if _failures.is_empty() else 1)

func _check(condition: bool, label: String) -> void:
	_checks += 1
	if not condition:
		_failures.append(label)
		push_error(label)

func _test_names_and_capacity() -> void:
	_check(SIM.valid_name("Alex Fisher"), "ordinary player name accepted")
	for bad: String in ["A", " bad", "bad ", "bad\nname", "<script>", "abcdefghijklmnopqrstu", "_start"]:
		_check(not SIM.valid_name(bad), "invalid player name rejected: " + bad.replace("\n", "newline"))
	var sim: MeadowSimulation = SIM.new()
	for index: int in SIM.CAPACITY:
		_check(not sim.add_player("Player%02d" % index).is_empty(), "capacity accepts slot %d" % index)
	_check(sim.add_player("Overflow").is_empty(), "capacity refuses 33rd player")

func _test_input_authority() -> void:
	var sim: MeadowSimulation = SIM.new()
	var id: String = sim.add_player("Player")
	_check(not sim.submit_input("spoof", 1, Vector2.RIGHT, false), "unknown membership refused")
	_check(sim.submit_input(id, 1, Vector2.RIGHT, false), "bounded input accepted")
	_check(not sim.submit_input(id, 1, Vector2.LEFT, false), "replayed input refused")
	_check(not sim.submit_input(id, 0, Vector2.LEFT, false), "older input refused")
	_check(not sim.submit_input(id, 10000, Vector2.ZERO, false), "sequence leap refused")
	_check(not sim.submit_input(id, 2, Vector2(100.0, 0.0), false), "oversized input refused")
	_check(not sim.submit_input(id, 2, Vector2(INF, 0.0), false), "infinite input refused")
	_check(not sim.submit_input(id, 2, Vector2(NAN, 0.0), false), "NaN input refused")
	_check(not sim.submit_input(id, 2, Vector2.ONE, false), "unnormalized diagonal refused")
	_check(sim.submit_input(id, 2, Vector2.ONE.normalized(), false), "normalized diagonal accepted")
	for sequence: int in range(3, 80):
		sim.submit_input(id, sequence, Vector2.ZERO, false)
	_check(sim.accepted_inputs <= SIM.MAX_INPUTS_PER_SECOND, "per-peer input budget enforced")
	_check(sim.rejected_inputs > 0, "rejected inputs counted")
	_check(sim.players[id].axis == Vector2.ZERO, "rejected payload never sets arbitrary velocity")

func _drive(sim: MeadowSimulation, id: String, axis: Vector2, steps: int, jump: bool = false) -> void:
	for frame: int in steps:
		sim.submit_input(id, int(sim.players[id].last_seq) + 1, axis, jump and frame == 0)
		sim.step()

func _test_movement_and_collision() -> void:
	var sim: MeadowSimulation = SIM.new()
	var id: String = sim.add_player("Mover")
	var start: Vector3 = sim.players[id].position
	_drive(sim, id, Vector2.RIGHT, SIM.TICK_RATE)
	var distance: float = start.distance_to(sim.players[id].position)
	_check(distance > 5.0 and distance <= SIM.SPEED + 0.001, "real fixed steps move at bounded speed")
	_drive(sim, id, Vector2.ONE.normalized(), SIM.TICK_RATE)
	_drive(sim, id, Vector2.RIGHT, 1)
	var turn_velocity: Vector3 = sim.players[id].velocity
	_check(Vector2(turn_velocity.x, turn_velocity.z).length() <= SIM.SPEED + 0.0001, "abrupt diagonal turns cannot exceed horizontal speed limit")
	for frame: int in 60:
		sim.step()
	_check(Vector3(sim.players[id].velocity).length() == 0.0, "stale input expires and motion stops")
	sim.players[id].position = Vector3(-33.0, 0.0, -20.0)
	sim.players[id].velocity = Vector3.ZERO
	_drive(sim, id, Vector2.RIGHT, 60)
	_check(float(sim.players[id].position.x) <= -31.0 - WORLD.PLAYER_RADIUS + 0.001, "barn west wall stops crossing")
	sim.players[id].position = Vector3(-22.0, 0.0, -8.0)
	sim.players[id].velocity = Vector3.ZERO
	_drive(sim, id, Vector2(0.0, -1.0), 45)
	_check(float(sim.players[id].position.z) < -12.0, "authored barn doorway admits player")
	sim.players[id].position = Vector3(41.0, 0.0, 0.0)
	sim.players[id].velocity = Vector3.ZERO
	_drive(sim, id, Vector2.RIGHT, 45)
	_check(float(sim.players[id].position.x) <= WORLD.MAX_X - WORLD.PLAYER_RADIUS + 0.0001, "world edge bounds enforced")
	sim.players[id].position = Vector3.ZERO
	sim.players[id].velocity = Vector3.ZERO
	_drive(sim, id, Vector2.ZERO, 8, true)
	_check(float(sim.players[id].position.y) > 0.5 and float(sim.players[id].position.y) < 1.3, "server-owned jump rises within physical bound")
	_drive(sim, id, Vector2.ZERO, 60)
	_check(float(sim.players[id].position.y) == 0.0, "jump lands on shared floor")

func _test_resume() -> void:
	var sim: MeadowSimulation = SIM.new()
	var id: String = sim.add_player("Resume")
	_drive(sim, id, Vector2.RIGHT, 30)
	var position: Vector3 = sim.players[id].position
	sim.disconnect_player(id)
	_check(not sim.submit_input(id, 31, Vector2.RIGHT, true), "disconnected membership cannot move")
	for frame: int in 20:
		sim.step()
	_check(sim.players[id].position == position, "disconnected player freezes")
	_check(sim.resume_player(id), "unexpired reservation resumes")
	_check(sim.players[id].position == position, "resume preserves position")
	_check(sim.submit_input(id, 1, Vector2.LEFT, false), "resumed connection has fresh input sequence")
	_check(not sim.resume_player(id), "live session cannot be resumed twice")
	sim.disconnect_player(id)
	for frame: int in SIM.RESUME_TICKS:
		sim.step()
	_check(not sim.players.has(id), "expired reservation is reclaimed")
	_check(not sim.resume_player(id), "expired player cannot resume")

func _test_shared_round() -> void:
	var sim: MeadowSimulation = SIM.new()
	var one: String = sim.add_player("One")
	var two: String = sim.add_player("Two")
	_check(sim.interact(one, 1).code == "distance", "remote landmark activation refused")
	sim.players[two].position = WORLD.BELL_POSITION
	_check(sim.interact(two, 1).code == "missing_landmarks", "bell cannot complete unfinished round")
	var sequence: int = 1
	for id: String in WORLD.LANDMARKS:
		for frame: int in SIM.ACTION_COOLDOWN_TICKS:
			sim.step()
		sequence += 1
		sim.players[one].position = WORLD.LANDMARKS[id].position
		_check(sim.interact(one, sequence).ok, "nearby landmark lit: " + id)
		_check(sim.interact(one, sequence).code == "sequence", "replayed interaction rejected: " + id)
		_check(sim.interact(one, sequence + 1).code == "cooldown", "per-peer cooldown enforced: " + id)
		sequence += 1
	_check(sim.lit.size() == 3, "three shared landmarks recorded")
	_check(sim.interact(two, 2).ok, "another player can ring shared completion bell")
	_check(sim.phase == "complete" and sim.completed_by == "Two", "round result records completing player")
	var state: Dictionary = sim.snapshot()
	_check(state.players.size() == 2 and state.round.lit.size() == 3, "snapshot includes both players and shared progress")
	_check(not JSON.stringify(state).contains("token") and not state.players[0].has("axis") and not state.players[0].has("last_seq"), "snapshot excludes credentials and input internals")
	for frame: int in SIM.ROUND_RESTART_TICKS:
		sim.step()
	_check(sim.round_id == 2 and sim.phase == "collecting" and sim.lit.is_empty(), "server timer starts next round")
	sim.reset_round()
	_check(sim.round_id == 3, "explicit authority reset advances round")

func _test_persistence_validation() -> void:
	var sim: MeadowSimulation = SIM.new()
	var valid: Dictionary = {"version": 1, "round_id": 8, "phase": "complete", "lit": ["west", "east", "grove"], "completed_by": "Alex Fisher"}
	_check(sim.restore_progress(valid), "supported progress restores")
	var saved: Dictionary = sim.export_progress()
	_check(saved.round_id == 8 and saved.phase == "complete" and saved.lit.size() == 3, "restore-export round trip")
	_check(sim.players.is_empty(), "save restore never recreates unauthenticated player sessions")
	var candidates: Array[Dictionary] = []
	var extra: Dictionary = valid.duplicate(true)
	extra["token"] = "secret"
	candidates.append(extra)
	var fractional: Dictionary = valid.duplicate(true)
	fractional.round_id = 8.5
	candidates.append(fractional)
	var unknown: Dictionary = valid.duplicate(true)
	unknown.lit = ["west", "east", "admin"]
	candidates.append(unknown)
	var duplicate: Dictionary = valid.duplicate(true)
	duplicate.lit = ["west", "west", "grove"]
	candidates.append(duplicate)
	var bad_name: Dictionary = valid.duplicate(true)
	bad_name.completed_by = "Bad\nName"
	candidates.append(bad_name)
	var incomplete: Dictionary = valid.duplicate(true)
	incomplete.lit = ["west"]
	candidates.append(incomplete)
	for index: int in candidates.size():
		_check(not sim.restore_progress(candidates[index]), "malformed persistence refused %d" % index)
		_check(sim.export_progress() == saved, "rejected restore leaves live progress intact %d" % index)
	var json_roundtrip: Variant = JSON.parse_string(JSON.stringify(valid))
	_check(sim.restore_progress(json_roundtrip), "JSON numeric decoding handled without accepting fractional values")

func _test_wire_snapshot() -> void:
	var sim: MeadowSimulation = SIM.new()
	var name: String = "abcdefghijklmnopqrst"
	for index: int in SIM.CAPACITY:
		sim.add_player(name)
	sim.restore_progress({"version": 1, "round_id": 4, "phase": "complete", "lit": ["west", "east", "grove"], "completed_by": name})
	var codec = SESSION_SCRIPT.new()
	var original: Dictionary = sim.snapshot()
	var wire: PackedByteArray = codec._encode_snapshot(original)
	_check(wire.size() == 1277, "maximum-name 32-player wire snapshot stays below ENet MTU")
	var decoded: Dictionary = codec._decode_snapshot(wire)
	_check(codec._valid_snapshot(decoded), "packed snapshot decodes to validated public schema")
	_check(decoded.round == original.round and decoded.players.size() == SIM.CAPACITY, "packed wire retains shared round and membership")
	for index: int in SIM.CAPACITY:
		_check(WORLD.array_vector(decoded.players[index].position).distance_to(WORLD.array_vector(original.players[index].position)) < 0.002, "wire position quantization within two millimetres %d" % index)
	for length: int in range(0, 42):
		_check(codec._decode_snapshot(wire.slice(0, length)).is_empty(), "truncated wire rejected %d" % length)
	var truncated: PackedByteArray = wire.slice(0, wire.size() - 1)
	_check(codec._decode_snapshot(truncated).is_empty(), "truncated final player rejected")
	var oversized: PackedByteArray = wire.duplicate()
	oversized.append(0)
	_check(codec._decode_snapshot(oversized).is_empty(), "oversized snapshot rejected")
	var invalid_phase: PackedByteArray = wire.duplicate()
	invalid_phase[13] = 255
	_check(codec._decode_snapshot(invalid_phase).is_empty(), "invalid packed phase rejected")
	var duplicate_id: Dictionary = original.duplicate(true)
	duplicate_id.players[1].id = duplicate_id.players[0].id
	_check(not codec._valid_snapshot(duplicate_id), "duplicate player identity rejected before rendering")
	var bad_velocity: Dictionary = original.duplicate(true)
	bad_velocity.players[0].velocity[0] = NAN
	_check(not codec._valid_snapshot(bad_velocity), "non-finite replicated velocity rejected before rendering")
	var bad_round: Dictionary = original.duplicate(true)
	bad_round.round.lit = ["fake"]
	_check(not codec._valid_snapshot(bad_round), "unknown replicated landmark rejected before rendering")
	codec.free()

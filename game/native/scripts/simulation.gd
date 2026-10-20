class_name MeadowSimulation
extends RefCounted
## Pure, server-owned fixed-step simulation. No networking or scene dependencies.

const WORLD = preload("res://scripts/world_data.gd")
const PROTOCOL: int = 1
const TICK_RATE: int = 30
const STEP: float = 1.0 / float(TICK_RATE)
const CAPACITY: int = 32
const SPEED: float = 6.0
const ACCELERATION: float = 32.0
const GRAVITY: float = 22.0
const JUMP_SPEED: float = 7.2
const INPUT_TIMEOUT: int = 12
const RESUME_TICKS: int = 120 * TICK_RATE
const ROUND_RESTART_TICKS: int = 30 * TICK_RATE
const ACTION_COOLDOWN_TICKS: int = 10
const MAX_INPUTS_PER_SECOND: int = 60
const MAX_SEQUENCE_GAP: int = 512
const MAX_SEQUENCE: int = 2147483646

var tick: int = 0
var players: Dictionary = {}
var round_id: int = 1
var phase: String = "collecting"
var lit: Dictionary = {}
var completed_by: String = ""
var completed_at_tick: int = -1
var revision: int = 0
var accepted_inputs: int = 0
var rejected_inputs: int = 0
var accepted_actions: int = 0
var rejected_actions: int = 0
var _next_player: int = 1

static func valid_name(value: String) -> bool:
	if value.length() < 2 or value.length() > 20 or value != value.strip_edges():
		return false
	for index: int in value.length():
		var point: int = value.unicode_at(index)
		var letter: bool = (point >= 65 and point <= 90) or (point >= 97 and point <= 122)
		var digit: bool = point >= 48 and point <= 57
		if not letter and not digit and (index == 0 or (point != 32 and point != 45 and point != 95)):
			return false
	return true

func add_player(display_name: String) -> String:
	if not valid_name(display_name) or players.size() >= CAPACITY:
		return ""
	var id: String = "p%04d" % _next_player
	_next_player += 1
	players[id] = {
		"id": id, "name": display_name,
		"position": WORLD.spawn_position(players.size()), "velocity": Vector3.ZERO,
		"axis": Vector2.ZERO, "jump_queued": false, "connected": true,
		"last_seq": -1, "last_action_seq": -1, "last_input_tick": tick,
		"last_action_tick": -ACTION_COOLDOWN_TICKS, "expires_tick": -1,
		"rate_window": tick / TICK_RATE, "rate_count": 0,
	}
	return id

func disconnect_player(id: String) -> void:
	if not players.has(id):
		return
	var player: Dictionary = players[id]
	player.connected = false
	player.axis = Vector2.ZERO
	player.velocity = Vector3.ZERO
	player.jump_queued = false
	player.expires_tick = tick + RESUME_TICKS

func resume_player(id: String) -> bool:
	if not players.has(id):
		return false
	var player: Dictionary = players[id]
	if player.connected or int(player.expires_tick) <= tick:
		return false
	player.connected = true
	player.expires_tick = -1
	player.last_seq = -1
	player.last_action_seq = -1
	player.last_input_tick = tick
	player.rate_count = 0
	player.rate_window = tick / TICK_RATE
	return true

func submit_input(id: String, sequence: int, axis: Vector2, jump: bool) -> bool:
	if not players.has(id):
		rejected_inputs += 1
		return false
	var player: Dictionary = players[id]
	var window: int = tick / TICK_RATE
	if int(player.rate_window) != window:
		player.rate_window = window
		player.rate_count = 0
	player.rate_count = int(player.rate_count) + 1
	if not player.connected or int(player.rate_count) > MAX_INPUTS_PER_SECOND:
		rejected_inputs += 1
		return false
	if sequence < 0 or sequence > MAX_SEQUENCE or sequence <= int(player.last_seq) or sequence - int(player.last_seq) > MAX_SEQUENCE_GAP:
		rejected_inputs += 1
		return false
	if not axis.is_finite() or axis.length_squared() > 1.002 or absf(axis.x) > 1.0 or absf(axis.y) > 1.0:
		rejected_inputs += 1
		return false
	player.last_seq = sequence
	player.last_input_tick = tick
	player.axis = axis.limit_length(1.0)
	player.jump_queued = bool(player.jump_queued) or jump
	accepted_inputs += 1
	return true

func reject_malformed_input() -> void:
	rejected_inputs += 1

func interact(id: String, sequence: int) -> Dictionary:
	if not players.has(id):
		return _reject_action("membership", "Join the meadow first.")
	var player: Dictionary = players[id]
	if not player.connected:
		return _reject_action("membership", "Your session is disconnected.")
	if sequence < 0 or sequence > MAX_SEQUENCE or sequence <= int(player.last_action_seq) or sequence - int(player.last_action_seq) > MAX_SEQUENCE_GAP:
		return _reject_action("sequence", "That interaction has already been received.")
	player.last_action_seq = sequence
	if tick - int(player.last_action_tick) < ACTION_COOLDOWN_TICKS:
		return _reject_action("cooldown", "One moment between interactions.")
	player.last_action_tick = tick
	var position: Vector3 = player.position
	for landmark_id: String in WORLD.LANDMARKS:
		var landmark: Dictionary = WORLD.LANDMARKS[landmark_id]
		if position.distance_to(landmark.position) <= float(landmark.radius):
			if phase == "complete":
				return _reject_action("round_complete", "The meadow is already glowing. A new round starts soon.")
			if lit.has(landmark_id):
				return _reject_action("already_lit", "This lantern is already lit.")
			lit[landmark_id] = true
			revision += 1
			accepted_actions += 1
			return {"ok": true, "code": "lit", "message": "%s lit the %s." % [player.name, landmark.label]}
	if position.distance_to(WORLD.BELL_POSITION) <= WORLD.INTERACTION_RADIUS:
		if phase == "complete":
			return _reject_action("round_complete", "The bell has already rung. Enjoy the celebration!")
		if lit.size() != WORLD.LANDMARKS.size():
			return _reject_action("missing_landmarks", "Light all three meadow lanterns before ringing the bell.")
		phase = "complete"
		completed_by = str(player.name)
		completed_at_tick = tick
		revision += 1
		accepted_actions += 1
		return {"ok": true, "code": "complete", "message": "%s rang the community bell. Meadow restored!" % player.name}
	return _reject_action("distance", "Move closer to a lantern or the barn bell, then press E.")

func _reject_action(code: String, message: String) -> Dictionary:
	rejected_actions += 1
	return {"ok": false, "code": code, "message": message}

func reset_round() -> void:
	round_id += 1
	phase = "collecting"
	lit.clear()
	completed_by = ""
	completed_at_tick = -1
	revision += 1

func step() -> void:
	tick += 1
	var expired: Array[String] = []
	for id: String in players:
		var player: Dictionary = players[id]
		if not player.connected:
			if tick >= int(player.expires_tick):
				expired.append(id)
			continue
		var axis: Vector2 = player.axis
		if tick - int(player.last_input_tick) > INPUT_TIMEOUT:
			axis = Vector2.ZERO
			player.jump_queued = false
		var velocity: Vector3 = player.velocity
		var position: Vector3 = player.position
		# Accelerate the vector as a whole so abrupt diagonal turns cannot exceed SPEED.
		var horizontal: Vector2 = Vector2(velocity.x, velocity.z).move_toward(axis * SPEED, ACCELERATION * STEP)
		velocity.x = horizontal.x
		velocity.z = horizontal.y
		if bool(player.jump_queued) and position.y <= 0.001:
			velocity.y = JUMP_SPEED
		player.jump_queued = false
		velocity.y -= GRAVITY * STEP
		position = _move_horizontal(position, Vector2(velocity.x, velocity.z) * STEP)
		position.y = maxf(0.0, position.y + velocity.y * STEP)
		if position.y == 0.0:
			velocity.y = 0.0
		player.position = position
		player.velocity = velocity
	for id: String in expired:
		players.erase(id)
	if phase == "complete" and tick - completed_at_tick >= ROUND_RESTART_TICKS:
		reset_round()

func _move_horizontal(position: Vector3, motion: Vector2) -> Vector3:
	var radius: float = WORLD.PLAYER_RADIUS
	var next_x: float = clampf(position.x + motion.x, WORLD.MIN_X + radius, WORLD.MAX_X - radius)
	for collider: Rect2 in WORLD.COLLIDERS:
		var obstacle: Rect2 = collider.grow(radius)
		if position.z > obstacle.position.y and position.z < obstacle.end.y:
			if position.x <= obstacle.position.x and next_x > obstacle.position.x:
				next_x = obstacle.position.x
			elif position.x >= obstacle.end.x and next_x < obstacle.end.x:
				next_x = obstacle.end.x
	position.x = next_x
	var next_z: float = clampf(position.z + motion.y, WORLD.MIN_Z + radius, WORLD.MAX_Z - radius)
	for collider: Rect2 in WORLD.COLLIDERS:
		var obstacle: Rect2 = collider.grow(radius)
		if position.x > obstacle.position.x and position.x < obstacle.end.x:
			if position.z <= obstacle.position.y and next_z > obstacle.position.y:
				next_z = obstacle.position.y
			elif position.z >= obstacle.end.y and next_z < obstacle.end.y:
				next_z = obstacle.end.y
	position.z = next_z
	return position

func snapshot() -> Dictionary:
	var public_players: Array = []
	var connected_count: int = 0
	for id: String in players:
		var player: Dictionary = players[id]
		if player.connected:
			connected_count += 1
		public_players.append({
			"id": id, "name": player.name,
			"position": WORLD.vector_array(player.position),
			"velocity": WORLD.vector_array(player.velocity), "connected": player.connected,
		})
	var landmarks: Array = []
	for id: String in WORLD.LANDMARKS:
		var landmark: Dictionary = WORLD.LANDMARKS[id]
		landmarks.append({"id": id, "label": landmark.label, "position": WORLD.vector_array(landmark.position), "radius": landmark.radius, "lit": lit.has(id)})
	var lit_ids: Array = lit.keys()
	lit_ids.sort()
	return {
		"protocol": PROTOCOL, "tick": tick,
		"players": public_players, "landmarks": landmarks,
		"bell": {"position": WORLD.vector_array(WORLD.BELL_POSITION), "radius": WORLD.INTERACTION_RADIUS},
		"round": {"id": round_id, "phase": phase, "lit": lit_ids, "completed_by": completed_by, "completed_at_tick": completed_at_tick},
		"server": {"capacity": CAPACITY, "tick_rate": TICK_RATE, "connected_count": connected_count, "accepted_inputs": accepted_inputs, "rejected_inputs": rejected_inputs, "accepted_actions": accepted_actions, "rejected_actions": rejected_actions},
	}

func export_progress() -> Dictionary:
	return {"version": 1, "round_id": round_id, "phase": phase, "lit": lit.keys(), "completed_by": completed_by}

func restore_progress(data: Dictionary) -> bool:
	## Validate the entire candidate before changing any live state. No player credentials are persisted.
	if data.size() != 5 or not _json_integer(data.get("version")) or int(data.version) != 1:
		return false
	if not _json_integer(data.get("round_id")) or int(data.round_id) < 1 or int(data.round_id) > 1000000000:
		return false
	if typeof(data.get("phase")) != TYPE_STRING or not ["collecting", "complete"].has(data.phase):
		return false
	if typeof(data.get("lit")) != TYPE_ARRAY or data.lit.size() > WORLD.LANDMARKS.size():
		return false
	var candidate_lit: Dictionary = {}
	for id: Variant in data.lit:
		if typeof(id) != TYPE_STRING or not WORLD.LANDMARKS.has(id) or candidate_lit.has(id):
			return false
		candidate_lit[id] = true
	if typeof(data.get("completed_by")) != TYPE_STRING:
		return false
	if data.phase == "complete" and (candidate_lit.size() != WORLD.LANDMARKS.size() or not valid_name(data.completed_by)):
		return false
	if data.phase == "collecting" and data.completed_by != "":
		return false
	round_id = int(data.round_id)
	phase = str(data.phase)
	lit = candidate_lit
	completed_by = str(data.completed_by)
	completed_at_tick = tick if phase == "complete" else -1
	revision += 1
	return true

static func _json_integer(value: Variant) -> bool:
	return (typeof(value) == TYPE_INT or typeof(value) == TYPE_FLOAT) and is_finite(float(value)) and float(value) == floorf(float(value))

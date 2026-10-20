extends Node
## Real ENet integration peer. Uses the production Session autoload and never teleports authority state.

const WORLD = preload("res://scripts/world_data.gd")
const SIM = preload("res://scripts/simulation.gd")
var _args: Dictionary = {}
var _role: String = "bot"
var _mode: String = "observer"
var _name: String = "TestPlayer"
var _result_file: String = ""
var _telemetry_file: String = ""
var _stop_file: String = ""
var _duration: float = 65.0
var _elapsed: float = 0.0
var _finished: bool = false
var _last_write: float = -1.0
var _joined_count: int = 0
var _initial_id: String = ""
var _peak_players: int = 0
var _snapshots: int = 0
var _last_tick: int = -1
var _last_position: Vector3 = Vector3.ZERO
var _moved_distance: float = 0.0
var _max_speed: float = 0.0
var _speed_valid: bool = true
var _got_distance_rejection: bool = false
var _malformed_sent: bool = false
var _client_reset_attempted: bool = false
var _probe_cases: Array[Dictionary] = []
var _probe_index: int = 0
var _probe_sent: bool = false
var _probe_sent_at: float = -1.0
var _probe_rejections_before: int = 0
var _probe_acceptances_before: int = 0
var _probe_confirmed: Array[String] = []
var _saw_rejected_inputs: bool = false
var _protocol_denied: bool = false
var _capacity_denied: bool = false
var _completed_round: Dictionary = {}
var _complete_seen_at: float = -1.0
var _route_index: int = 0
var _last_interaction: float = -10.0
var _reconnect_requested: bool = false
var _reconnect_at: float = -1.0
var _resume_before: Vector3 = Vector3.ZERO
var _resume_before_lit: Array = []
var _resumed_same_player: bool = false
var _resume_distance: float = 1000.0
var _resume_progress: bool = false
var _restored_after_crash: bool = false
var _recovery_rounds: Array = []
var _route: Array[Dictionary] = []

func _ready() -> void:
	_args = _parse_args()
	_role = str(_args.get("role", "bot"))
	_mode = str(_args.get("mode", "observer"))
	_name = str(_args.get("name", "TestPlayer"))
	_result_file = str(_args.get("result-file", ""))
	_telemetry_file = str(_args.get("telemetry-file", ""))
	_stop_file = str(_args.get("stop-file", ""))
	_duration = float(_args.get("duration", "65"))
	Session.state_changed.connect(_snapshot_changed)
	Session.joined.connect(_joined)
	Session.notice.connect(_notice)
	var port: int = int(_args.get("port", "27841"))
	if _role == "server":
		var config_error: Error = Session.configure_state_file(str(_args.get("state-file", "")))
		if config_error != OK:
			_finish(false, "state_configuration_error")
			return
		var server_error: Error = Session.start_server(port)
		if server_error != OK:
			_finish(false, "server_start_error_%d" % server_error)
		return
	_route = [
		{"id": "west", "position": WORLD.LANDMARKS.west.position},
		{"id": "grove", "position": WORLD.LANDMARKS.grove.position},
		{"id": "waypoint", "position": Vector3(18.0, 0.0, 5.0)},
		{"id": "east", "position": WORLD.LANDMARKS.east.position},
		{"id": "waypoint", "position": Vector3(-22.0, 0.0, -7.0)},
		{"id": "bell", "position": WORLD.BELL_POSITION},
	]
	_probe_cases = [
		{"case": "valid_baseline", "accept": true, "payload": {"seq": 1, "axis": Vector2.ZERO, "jump": false}},
		{"case": "replayed_sequence", "accept": false, "payload": {"seq": 1, "axis": Vector2.ZERO, "jump": false}},
		{"case": "older_sequence", "accept": false, "payload": {"seq": 0, "axis": Vector2.ZERO, "jump": false}},
		{"case": "oversized_axis", "accept": false, "payload": {"seq": 2, "axis": Vector2(100.0, 0.0), "jump": false}},
		{"case": "wrong_axis_type", "accept": false, "payload": {"seq": 2, "axis": "not_a_vector", "jump": false}},
		{"case": "spoofed_member", "accept": false, "payload": {"seq": 2, "axis": Vector2.ZERO, "jump": false, "player_id": "spoof"}},
		{"case": "sequence_leap", "accept": false, "payload": {"seq": 1000000, "axis": Vector2.ZERO, "jump": false}},
		{"case": "non_finite_axis", "accept": false, "payload": {"seq": 2, "axis": Vector2(NAN, 0.0), "jump": false}},
	]
	if _mode == "bad_protocol":
		var peer: ENetMultiplayerPeer = ENetMultiplayerPeer.new()
		var error: Error = peer.create_client("127.0.0.1", port, 3)
		if error != OK:
			_finish(false, "probe_connection_error")
			return
		multiplayer.connected_to_server.connect(_send_bad_hello)
		multiplayer.multiplayer_peer = peer
	elif Session.join_server(str(_args.get("address", "127.0.0.1")), port, _name) != OK:
		_finish(false, "client_start_error")

func _parse_args() -> Dictionary:
	var values: Dictionary = {}
	var arguments: PackedStringArray = OS.get_cmdline_user_args()
	var index: int = 0
	while index < arguments.size():
		if arguments[index].begins_with("--"):
			var key: String = arguments[index].substr(2)
			if index + 1 < arguments.size() and not arguments[index + 1].begins_with("--"):
				values[key] = arguments[index + 1]
				index += 1
			else:
				values[key] = "true"
		index += 1
	return values

func _send_bad_hello() -> void:
	Session._receive_hello.rpc_id(1, {"protocol": 999, "name": "BadProtocol", "resume": ""})
	## A second invalid hello must not extend the initial handshake or get a second allocation.
	Session._receive_hello.rpc_id(1, {"protocol": 999, "name": "BadProtocol", "resume": ""})

func _joined(id: String) -> void:
	_joined_count += 1
	_last_tick = -1
	if _joined_count == 1:
		_initial_id = id
		if _mode == "actor":
			Session.interact()
		elif _mode == "input_probe":
			# Stop this test client's ordinary input pump while probing the production RPC.
			# A newer normal packet must not supersede the deliberately invalid packet.
			Session.set_physics_process(false)
			_probe_rejections_before = int(Session.snapshot.server.rejected_inputs)
			_probe_acceptances_before = int(Session.snapshot.server.accepted_inputs)
	elif _mode == "actor" and _reconnect_requested:
		_resumed_same_player = id == _initial_id
		var player: Dictionary = Session.get_local_player()
		if not player.is_empty():
			_resume_distance = WORLD.array_vector(player.position).distance_to(_resume_before)
		_resume_progress = true
		for landmark: Variant in _resume_before_lit:
			if not Session.snapshot.round.lit.has(landmark):
				_resume_progress = false
	elif _mode == "recovery":
		_restored_after_crash = Session.snapshot.round.phase == "complete" and Session.snapshot.round.lit.size() == 3
	if _mode == "recovery":
		_recovery_rounds.append(Session.snapshot.round.duplicate(true))
	print("METALLIC_BOT_JOINED " + JSON.stringify({"name": _name, "id": id, "join_count": _joined_count}))

func _notice(message: String) -> void:
	if message.begins_with("Move closer"):
		_got_distance_rejection = true
	if _mode == "bad_protocol" and message.contains("same Metallic-RC"):
		_protocol_denied = true
		_finish(true, "protocol_mismatch_refused")
	if _mode == "expect_full" and message.contains("full"):
		_capacity_denied = true
		_finish(true, "capacity_refused")

func _snapshot_changed(state: Dictionary) -> void:
	if state.is_empty() or _role == "server":
		return
	_snapshots += 1
	_peak_players = maxi(_peak_players, int(state.server.connected_count))
	_saw_rejected_inputs = _saw_rejected_inputs or int(state.server.rejected_inputs) >= 4
	var player: Dictionary = Session.get_local_player()
	if not player.is_empty():
		var position: Vector3 = WORLD.array_vector(player.position)
		var next_tick: int = int(state.tick)
		if _last_tick >= 0 and next_tick > _last_tick:
			var travel: float = Vector2(position.x - _last_position.x, position.z - _last_position.z).length()
			var time_delta: float = float(next_tick - _last_tick) / float(SIM.TICK_RATE)
			_moved_distance += travel
			_max_speed = maxf(_max_speed, travel / time_delta)
			if travel > SIM.SPEED * time_delta + 0.008:
				_speed_valid = false
		_last_position = position
		_last_tick = next_tick
	if state.round.phase == "complete":
		_completed_round = state.round.duplicate(true)
		if _complete_seen_at < 0.0:
			_complete_seen_at = _elapsed

func _process(delta: float) -> void:
	if _finished:
		return
	_elapsed += delta
	if _role == "server":
		if _elapsed - _last_write >= 1.0:
			_last_write = _elapsed
			_write_json(_telemetry_file, {"kind": "server", "ok": true, "elapsed_seconds": _elapsed, "metrics": Session.metrics(), "snapshot": Session.snapshot})
		if _elapsed >= _duration or (not _stop_file.is_empty() and FileAccess.file_exists(_stop_file)):
			_finish(true, "server_stopped")
		return
	if _elapsed >= _duration:
		if _mode == "load":
			_finish(_joined_count > 0 and _peak_players >= int(_args.get("expected-players", "2")) and _speed_valid and _moved_distance > 0.5, "load_duration")
		else:
			_finish(false, "timeout_waiting_for_scenario")
		return
	if not _stop_file.is_empty() and FileAccess.file_exists(_stop_file):
		_finish(_joined_count > 0 and _speed_valid and (_mode != "load" or _peak_players >= int(_args.get("expected-players", "2"))), "controlled_stop")
		return
	if _reconnect_at > 0.0 and _elapsed >= _reconnect_at:
		_reconnect_at = -1.0
		Session.join_server(str(_args.get("address", "127.0.0.1")), int(_args.get("port", "27841")), _name)
	if not Session.connected:
		return
	if _mode == "actor":
		_actor_step()
	elif _mode == "input_probe":
		_input_probe_step()
	elif _mode == "load":
		var offset: float = float(_args.get("index", "0")) * 0.43
		Session.send_input(Vector2(cos(_elapsed * 0.7 + offset), sin(_elapsed * 0.7 + offset)), int(_elapsed * 30.0) % 180 == 0)
	elif _mode == "recovery" and _joined_count >= int(_args.get("expected-joins", "2")) and _restored_after_crash:
		_finish(_speed_valid, "crash_restart_restored_shared_round")
	elif _mode == "restore" and not _completed_round.is_empty():
		_finish(_completed_round.lit.size() == 3, "persisted_round_restored")
	elif _mode == "observer" and _complete_seen_at >= 0.0 and _elapsed - _complete_seen_at > 1.0:
		_finish(_peak_players >= 2 and _completed_round.lit.size() == 3, "shared_round_observed")

func _actor_step() -> void:
	if not _client_reset_attempted:
		_client_reset_attempted = true
		Session.reset_round()
	if _args.has("reconnect") and not _reconnect_requested and _route_index >= 1:
		_reconnect_requested = true
		_resume_before = WORLD.array_vector(Session.get_local_player().position)
		_resume_before_lit = Session.snapshot.round.lit.duplicate()
		Session.leave()
		_reconnect_at = _elapsed + 0.75
		return
	if _complete_seen_at >= 0.0:
		Session.send_input(Vector2.ZERO)
		if _elapsed - _complete_seen_at > 1.5:
			var resume_ok: bool = not _args.has("reconnect") or (_resumed_same_player and _resume_distance < 1.0 and _resume_progress)
			_finish(_speed_valid and _moved_distance > 80.0 and _got_distance_rejection and _saw_rejected_inputs and resume_ok and _completed_round.lit.size() == 3, "authoritative_activity_complete")
		return
	if _route_index >= _route.size():
		Session.send_input(Vector2.ZERO)
		return
	var player: Dictionary = Session.get_local_player()
	if player.is_empty():
		return
	var position: Vector3 = WORLD.array_vector(player.position)
	var destination: Dictionary = _route[_route_index]
	var difference: Vector3 = Vector3(destination.position) - position
	var axis: Vector2 = Vector2(difference.x, difference.z)
	if axis.length() > 1.0:
		Session.send_input(axis.normalized())
		return
	Session.send_input(Vector2.ZERO)
	if destination.id == "waypoint":
		_route_index += 1
	elif destination.id != "bell" and Session.snapshot.round.lit.has(destination.id):
		_route_index += 1
	elif _elapsed - _last_interaction > 0.45:
		_last_interaction = _elapsed
		Session.interact()

func _input_probe_step() -> void:
	if _probe_index >= _probe_cases.size():
		_finish(_probe_confirmed.size() == _probe_cases.size() and _moved_distance < 0.01, "each_input_guard_acknowledged_by_authority")
		return
	var current: Dictionary = _probe_cases[_probe_index]
	var rejections: int = int(Session.snapshot.server.rejected_inputs)
	var acceptances: int = int(Session.snapshot.server.accepted_inputs)
	var acknowledged: bool = acceptances > _probe_acceptances_before if current.accept else rejections > _probe_rejections_before
	if _probe_sent and acknowledged:
		_probe_confirmed.append(str(current.case))
		_probe_index += 1
		_probe_sent = false
		_probe_rejections_before = rejections
		_probe_acceptances_before = acceptances
		return
	if not _probe_sent or _elapsed - _probe_sent_at > 0.5:
		_probe_sent = true
		_probe_sent_at = _elapsed
		_malformed_sent = _malformed_sent or not bool(current.accept)
		Session._receive_input.rpc_id(1, current.payload)

func _finish(ok: bool, reason: String) -> void:
	if _finished:
		return
	_finished = true
	var report: Dictionary = {
		"kind": _role, "mode": _mode, "name": _name, "ok": ok, "reason": reason,
		"elapsed_seconds": snappedf(_elapsed, 0.001), "join_count": _joined_count,
		"player_id": Session.player_id, "initial_player_id": _initial_id,
		"peak_players": _peak_players, "snapshots": _snapshots,
		"moved_metres": snappedf(_moved_distance, 0.001), "observed_max_speed_mps": snappedf(_max_speed, 0.001),
		"speed_bound_passed": _speed_valid, "distance_rejection_observed": _got_distance_rejection,
		"malformed_inputs_sent": _malformed_sent, "server_rejections_observed": _saw_rejected_inputs,
		"input_guard_cases_acknowledged": _probe_confirmed, "client_reset_api_attempted": _client_reset_attempted,
		"resumed_same_player": _resumed_same_player, "resume_distance_metres": snappedf(_resume_distance, 0.001), "resume_progress_retained": _resume_progress,
		"protocol_mismatch_denied": _protocol_denied, "capacity_denied": _capacity_denied,
		"crash_recovery_round_retained": _restored_after_crash, "round": _completed_round,
		"recovery_rounds": _recovery_rounds, "crash_recovery_count": maxi(0, _joined_count - 1) if _mode == "recovery" else 0,
		"metrics": Session.metrics() if _role == "server" else {},
	}
	if _role == "server":
		report.snapshot = Session.snapshot
	_write_json(_result_file, report)
	print("METALLIC_NETWORK_RESULT " + JSON.stringify(report))
	Session.leave()
	get_tree().quit(0 if ok else 1)

func _write_json(path: String, data: Dictionary) -> void:
	if path.is_empty():
		return
	DirAccess.make_dir_recursive_absolute(path.get_base_dir())
	var file: FileAccess = FileAccess.open(path + ".tmp", FileAccess.WRITE)
	if file == null:
		push_error("Could not write test evidence.")
		return
	file.store_string(JSON.stringify(data, "  ") + "\n")
	file.close()
	DirAccess.rename_absolute(path + ".tmp", path)

extends Node
## Private-playtest ENet session. All gameplay mutations happen in the authority simulation.
## A resume token is a short-lived bearer credential, not an account or external authentication.

signal state_changed(snapshot: Dictionary)
signal connection_changed(status: String)
signal notice(message: String)
signal joined(player_id: String)

const SIM = preload("res://scripts/simulation.gd")
const WORLD = preload("res://scripts/world_data.gd")
const SNAPSHOT_INTERVAL: int = 2
const HANDSHAKE_TICKS: int = 5 * SIM.TICK_RATE
const MAX_STATE_BYTES: int = 65536
const MAX_SNAPSHOT_BYTES: int = 1277
const LANDMARK_ORDER: Array[String] = ["west", "east", "grove"]

var snapshot: Dictionary = {}
var player_id: String = ""
var connected: bool = false
var _sim: MeadowSimulation
var _authority: bool = false
var _hosting: bool = false
var _leaving: bool = false
var _peer_players: Dictionary = {}
var _pending: Dictionary = {}
var _hello_seen: Dictionary = {}
var _resume_records: Dictionary = {}
var _peer_token_hash: Dictionary = {}
var _action_windows: Dictionary = {}
var _resume_token: String = ""
var _resume_endpoint: String = ""
var _display_name: String = ""
var _address: String = "127.0.0.1"
var _port: int = 27841
var _client_active: bool = false
var _reconnect: bool = false
var _reconnect_attempts: int = 0
var _fresh_retry_used: bool = false
var _next_reconnect_ms: int = 0
var _connect_deadline_ms: int = 0
var _input_sequence: int = 0
var _action_sequence: int = 0
var _latest_axis: Vector2 = Vector2.ZERO
var _latest_jump: bool = false
var _accumulator: float = 0.0
var _state_file: String = ""
var _saved_revision: int = -1
var _persistence_error: String = ""
var _save_retry_after_ms: int = 0
var _tick_costs: Array[int] = []
var _late_frames: int = 0
var _last_snapshot_tick: int = -1

func _ready() -> void:
	multiplayer.allow_object_decoding = false
	# This game has a star topology: peers never need direct RPC routes to other clients.
	multiplayer.server_relay = false
	multiplayer.peer_connected.connect(_peer_connected)
	multiplayer.peer_disconnected.connect(_peer_disconnected)
	multiplayer.connected_to_server.connect(_connected_to_server)
	multiplayer.connection_failed.connect(_connection_failed)
	multiplayer.server_disconnected.connect(_server_disconnected)

func configure_state_file(path: String) -> Error:
	if _authority or _client_active:
		return ERR_BUSY
	if path.length() > 4096 or path.to_utf8_buffer().has(0):
		return ERR_INVALID_PARAMETER
	_state_file = ProjectSettings.globalize_path(path).simplify_path() if not path.is_empty() else ""
	return OK

func start_host(display_name: String, port: int = 27841, bind_address: String = "127.0.0.1") -> Error:
	if not SIM.valid_name(display_name):
		notice.emit("Use 2–20 letters, numbers, spaces, hyphens, or underscores for your name.")
		return ERR_INVALID_PARAMETER
	var result: Error = _start_authority(port, bind_address)
	if result != OK:
		return result
	_hosting = true
	player_id = _sim.add_player(display_name)
	_peer_players[1] = player_id
	connected = true
	connection_changed.emit("Hosting a private meadow")
	_emit_snapshot()
	joined.emit(player_id)
	return OK

func start_server(port: int = 27841, bind_address: String = "127.0.0.1") -> Error:
	var result: Error = _start_authority(port, bind_address)
	if result == OK:
		connection_changed.emit("Dedicated meadow server running")
	return result

func _start_authority(port: int, bind_address: String) -> Error:
	if port < 1024 or port > 65535 or not bind_address.is_valid_ip_address():
		notice.emit("Choose a numeric bind address and a port between 1024 and 65535.")
		return ERR_INVALID_PARAMETER
	leave()
	_sim = SIM.new()
	var restore_result: Error = _load_progress()
	if restore_result != OK:
		notice.emit("Saved meadow state is invalid or unreadable. It has been preserved; server startup stopped.")
		_sim = null
		return restore_result
	var peer: ENetMultiplayerPeer = ENetMultiplayerPeer.new()
	peer.set_bind_ip(bind_address)
	## Eight bounded handshake slots allow a full room to return an explicit capacity response.
	var result: Error = peer.create_server(port, SIM.CAPACITY + 8, 3)
	if result != OK:
		notice.emit("Could not bind the meadow server. Check that the port is free.")
		_sim = null
		return result
	multiplayer.multiplayer_peer = peer
	_authority = true
	_port = port
	_address = bind_address
	_saved_revision = _sim.revision
	_tick_costs.clear()
	_late_frames = 0
	_persistence_error = ""
	_save_retry_after_ms = 0
	_emit_snapshot()
	print("METALLIC_READY " + JSON.stringify({"protocol": SIM.PROTOCOL, "port": port, "bind": bind_address, "server": true}))
	return OK

func join_server(address: String, port: int, display_name: String) -> Error:
	if not SIM.valid_name(display_name) or address.is_empty() or address.length() > 253 or port < 1024 or port > 65535:
		notice.emit("Enter a server address, a valid port, and a 2–20 character player name.")
		return ERR_INVALID_PARAMETER
	var endpoint: String = "%s:%d/%s" % [address.to_lower(), port, display_name]
	if endpoint != _resume_endpoint:
		_resume_token = ""
	_resume_endpoint = endpoint
	leave()
	_address = address
	_port = port
	_display_name = display_name
	_client_active = true
	_reconnect_attempts = 0
	_fresh_retry_used = false
	return _begin_client()

func _begin_client() -> Error:
	var peer: ENetMultiplayerPeer = ENetMultiplayerPeer.new()
	var result: Error = peer.create_client(_address, _port, 3)
	if result != OK:
		connection_changed.emit("Could not create connection")
		return result
	multiplayer.multiplayer_peer = peer
	_reconnect = false
	_connect_deadline_ms = Time.get_ticks_msec() + 8000
	connection_changed.emit("Connecting to meadow…")
	return OK

func leave() -> void:
	_leaving = true
	_client_active = false
	_reconnect = false
	_connect_deadline_ms = 0
	if _authority and _sim != null and _sim.revision != _saved_revision:
		_save_progress()
	if multiplayer.multiplayer_peer != null:
		multiplayer.multiplayer_peer.close()
	multiplayer.multiplayer_peer = null
	_authority = false
	_hosting = false
	connected = false
	player_id = ""
	snapshot = {}
	_peer_players.clear()
	_pending.clear()
	_hello_seen.clear()
	_resume_records.clear()
	_peer_token_hash.clear()
	_action_windows.clear()
	_input_sequence = 0
	_action_sequence = 0
	_latest_axis = Vector2.ZERO
	_latest_jump = false
	_accumulator = 0.0
	_last_snapshot_tick = -1
	_sim = null
	_leaving = false
	connection_changed.emit("Offline")
	state_changed.emit(snapshot)

func is_host() -> bool:
	return _authority and _hosting

func is_server() -> bool:
	return _authority

func send_input(axis: Vector2, jump: bool = false) -> void:
	if not connected or not axis.is_finite():
		return
	_latest_axis = axis.limit_length(1.0)
	_latest_jump = _latest_jump or jump

func interact() -> void:
	if not connected:
		return
	_action_sequence += 1
	if _authority:
		_handle_interaction(1, _action_sequence)
	else:
		_receive_interact.rpc_id(1, _action_sequence)

func reset_round() -> void:
	## Deliberately not an RPC: only the local hosting process can reset a round.
	if not is_host():
		return
	_sim.reset_round()
	_save_progress()
	_emit_snapshot()
	_broadcast_notice("The host started a new meadow round.")

func get_local_player() -> Dictionary:
	for player: Dictionary in snapshot.get("players", []):
		if str(player.get("id", "")) == player_id:
			return player
	return {}

func _physics_process(delta: float) -> void:
	if not _authority and not connected:
		return
	_accumulator += minf(delta, 0.25)
	var steps: int = 0
	while _accumulator >= SIM.STEP and steps < 8:
		_accumulator -= SIM.STEP
		steps += 1
		if connected:
			_input_sequence += 1
			if _authority:
				_sim.submit_input(player_id, _input_sequence, _latest_axis, _latest_jump)
			else:
				_receive_input.rpc_id(1, {"seq": _input_sequence, "axis": _latest_axis, "jump": _latest_jump})
			_latest_jump = false
		if _authority:
			var started: int = Time.get_ticks_usec()
			_fixed_tick()
			_tick_costs.append(Time.get_ticks_usec() - started)
			if _tick_costs.size() > 4096:
				_tick_costs.pop_front()
	if _accumulator >= SIM.STEP:
		_accumulator = fmod(_accumulator, SIM.STEP)
		_late_frames += 1

func _process(_delta: float) -> void:
	if not _client_active or connected:
		return
	var now: int = Time.get_ticks_msec()
	if _connect_deadline_ms > 0 and now >= _connect_deadline_ms:
		_connect_deadline_ms = 0
		if multiplayer.multiplayer_peer != null:
			multiplayer.multiplayer_peer.close()
		_schedule_reconnect()
	if _reconnect and now >= _next_reconnect_ms:
		_reconnect_attempts += 1
		_begin_client()

func _fixed_tick() -> void:
	_sim.step()
	for peer_id: Variant in _pending.keys():
		if _sim.tick >= int(_pending[peer_id]):
			_pending.erase(peer_id)
			(multiplayer.multiplayer_peer as ENetMultiplayerPeer).disconnect_peer(int(peer_id))
	for token_hash: Variant in _resume_records.keys():
		if not _sim.players.has(_resume_records[token_hash]):
			_resume_records.erase(token_hash)
	if _sim.revision != _saved_revision:
		_save_progress()
	if _sim.tick % SNAPSHOT_INTERVAL == 0:
		_emit_snapshot()

func _peer_connected(peer: int) -> void:
	if _authority and peer > 1:
		_pending[peer] = _sim.tick + HANDSHAKE_TICKS
		(multiplayer.multiplayer_peer as ENetMultiplayerPeer).get_peer(peer).set_timeout(8, 2000, 8000)

func _peer_disconnected(peer: int) -> void:
	if not _authority:
		return
	_pending.erase(peer)
	_hello_seen.erase(peer)
	_action_windows.erase(peer)
	_peer_token_hash.erase(peer)
	if _peer_players.has(peer):
		_sim.disconnect_player(str(_peer_players[peer]))
		_peer_players.erase(peer)
		_emit_snapshot()

func _connected_to_server() -> void:
	if _authority or not _client_active:
		return
	(multiplayer.multiplayer_peer as ENetMultiplayerPeer).get_peer(1).set_timeout(8, 2000, 8000)
	connection_changed.emit("Joining meadow…")
	_receive_hello.rpc_id(1, {"protocol": SIM.PROTOCOL, "name": _display_name, "resume": _resume_token})

func _connection_failed() -> void:
	if not _leaving and _client_active:
		_schedule_reconnect()

func _server_disconnected() -> void:
	if not _leaving and _client_active:
		connected = false
		_latest_axis = Vector2.ZERO
		_latest_jump = false
		_schedule_reconnect()

func _schedule_reconnect() -> void:
	connected = false
	_connect_deadline_ms = 0
	if _reconnect_attempts >= 5:
		_reconnect = false
		_client_active = false
		connection_changed.emit("Connection lost. Rejoin to try again.")
		return
	_reconnect = true
	_next_reconnect_ms = Time.get_ticks_msec() + int(500.0 * pow(2.0, float(_reconnect_attempts)))
	connection_changed.emit("Connection lost — reconnecting…")

@rpc("any_peer", "call_remote", "reliable", 0)
func _receive_hello(payload: Dictionary) -> void:
	if not _authority:
		return
	var peer: int = multiplayer.get_remote_sender_id()
	if peer <= 1 or not _pending.has(peer) or _peer_players.has(peer) or _hello_seen.has(peer):
		return
	_hello_seen[peer] = true
	_pending[peer] = mini(int(_pending[peer]), _sim.tick + SIM.TICK_RATE)
	if payload.size() != 3 or typeof(payload.get("protocol")) != TYPE_INT or int(payload.protocol) != SIM.PROTOCOL:
		_reject_join.rpc_id(peer, "protocol", "This server requires the same Metallic-RC playtest version.")
		return
	if typeof(payload.get("name")) != TYPE_STRING or not SIM.valid_name(str(payload.name)) or typeof(payload.get("resume")) != TYPE_STRING:
		_reject_join.rpc_id(peer, "name", "Player names must use 2–20 simple letters, numbers, spaces, hyphens, or underscores.")
		return
	var supplied_token: String = str(payload.resume)
	if supplied_token.length() > 64:
		_reject_join.rpc_id(peer, "resume_expired", "The resume session is invalid. Join again for a new session.")
		return
	var id: String = ""
	if not supplied_token.is_empty():
		var supplied_hash: String = supplied_token.sha256_text()
		if not _resume_records.has(supplied_hash):
			_reject_join.rpc_id(peer, "resume_expired", "Your resume window ended. Join again for a new session.")
			return
		id = str(_resume_records[supplied_hash])
		if not _sim.players.has(id) or str(_sim.players[id].name) != str(payload.name) or not _sim.resume_player(id):
			_reject_join.rpc_id(peer, "resume_expired", "That session cannot be resumed. Join again for a new session.")
			return
		_resume_records.erase(supplied_hash)
	else:
		id = _sim.add_player(str(payload.name))
		if id.is_empty():
			_reject_join.rpc_id(peer, "capacity", "This meadow is full, including short reconnect reservations.")
			return
	var token: String = Crypto.new().generate_random_bytes(32).hex_encode()
	if token.length() != 64:
		_sim.players.erase(id)
		_reject_join.rpc_id(peer, "unavailable", "Secure session creation is temporarily unavailable.")
		return
	var token_hash: String = token.sha256_text()
	_resume_records[token_hash] = id
	_peer_token_hash[peer] = token_hash
	_peer_players[peer] = id
	_pending.erase(peer)
	_accept_join.rpc_id(peer, id, token, _sim.snapshot())
	_emit_snapshot()

@rpc("authority", "call_remote", "reliable", 0)
func _accept_join(id: String, token: String, initial_snapshot: Dictionary) -> void:
	if _authority or multiplayer.get_remote_sender_id() != 1 or not _client_active or token.length() != 64 or not _valid_snapshot(initial_snapshot):
		return
	var assigned_member: bool = false
	for player: Dictionary in initial_snapshot.players:
		if player.id == id and player.connected:
			assigned_member = true
	if not assigned_member:
		return
	player_id = id
	_resume_token = token
	_input_sequence = 0
	_action_sequence = 0
	_last_snapshot_tick = -1
	_reconnect_attempts = 0
	_fresh_retry_used = false
	_reconnect = false
	_connect_deadline_ms = 0
	connected = true
	_apply_snapshot(initial_snapshot)
	connection_changed.emit("Connected to meadow")
	joined.emit(player_id)

@rpc("authority", "call_remote", "reliable", 0)
func _reject_join(code: String, message: String) -> void:
	if _authority or multiplayer.get_remote_sender_id() != 1:
		return
	if code == "resume_expired":
		_resume_token = ""
		if _client_active and not _fresh_retry_used:
			_fresh_retry_used = true
			if multiplayer.multiplayer_peer != null:
				multiplayer.multiplayer_peer.close()
			_schedule_reconnect()
			notice.emit("The previous session expired. Rejoining with a new player identity.")
			return
	leave()
	connection_changed.emit("Could not join meadow")
	notice.emit(message.left(200))

@rpc("any_peer", "call_remote", "unreliable_ordered", 2)
func _receive_input(payload: Dictionary) -> void:
	if not _authority:
		return
	var peer: int = multiplayer.get_remote_sender_id()
	if peer <= 1 or not _peer_players.has(peer):
		_sim.reject_malformed_input()
		return
	if payload.size() != 3 or typeof(payload.get("seq")) != TYPE_INT or typeof(payload.get("axis")) != TYPE_VECTOR2 or typeof(payload.get("jump")) != TYPE_BOOL:
		_sim.reject_malformed_input()
		return
	_sim.submit_input(str(_peer_players[peer]), int(payload.seq), payload.axis, bool(payload.jump))

@rpc("any_peer", "call_remote", "reliable", 0)
func _receive_interact(sequence: Variant) -> void:
	if not _authority:
		return
	var peer: int = multiplayer.get_remote_sender_id()
	if peer <= 1 or not _peer_players.has(peer) or typeof(sequence) != TYPE_INT:
		_sim.rejected_actions += 1
		return
	var window: int = _sim.tick / SIM.TICK_RATE
	var rate: Dictionary = _action_windows.get(peer, {"window": window, "count": 0})
	if int(rate.window) != window:
		rate = {"window": window, "count": 0}
	rate.count = int(rate.count) + 1
	_action_windows[peer] = rate
	if int(rate.count) > 6:
		_sim.rejected_actions += 1
		return
	_handle_interaction(peer, int(sequence))

func _handle_interaction(peer: int, sequence: int) -> void:
	if not _peer_players.has(peer):
		return
	var result: Dictionary = _sim.interact(str(_peer_players[peer]), sequence)
	if result.ok:
		_save_progress()
		_emit_snapshot()
		_broadcast_notice(str(result.message))
	elif result.code != "cooldown" and result.code != "sequence":
		if peer == 1:
			notice.emit(str(result.message))
		else:
			_server_notice.rpc_id(peer, str(result.message))

func _broadcast_notice(message: String) -> void:
	notice.emit(message)
	for peer: Variant in _peer_players:
		if int(peer) != 1 and _can_send_to(int(peer)):
			_server_notice.rpc_id(int(peer), message)

@rpc("authority", "call_remote", "reliable", 0)
func _server_notice(message: String) -> void:
	if not _authority and multiplayer.get_remote_sender_id() == 1 and connected:
		notice.emit(message.left(200))

func _emit_snapshot() -> void:
	if not _authority or _sim == null:
		return
	snapshot = _sim.snapshot()
	state_changed.emit(snapshot)
	var wire: PackedByteArray = _encode_snapshot(snapshot)
	for peer: Variant in _peer_players:
		if int(peer) != 1 and _can_send_to(int(peer)):
			_receive_snapshot.rpc_id(int(peer), wire)

func _can_send_to(peer: int) -> bool:
	if not multiplayer.get_peers().has(peer):
		return false
	var transport: ENetMultiplayerPeer = multiplayer.multiplayer_peer as ENetMultiplayerPeer
	if transport == null:
		return false
	var packet_peer: ENetPacketPeer = transport.get_peer(peer)
	return packet_peer != null and packet_peer.get_state() == ENetPacketPeer.STATE_CONNECTED

@rpc("authority", "call_remote", "unreliable_ordered", 1)
func _receive_snapshot(wire: PackedByteArray) -> void:
	if _authority or not connected or multiplayer.get_remote_sender_id() != 1:
		return
	var data: Dictionary = _decode_snapshot(wire)
	if not _valid_snapshot(data):
		return
	_apply_snapshot(data)

func _encode_snapshot(data: Dictionary) -> PackedByteArray:
	## Fixed-width centimetre fractions keep 32 maximum-length names below ENet's MTU.
	## The public/UI snapshot remains a Dictionary; only the wire representation is packed.
	var buffer: StreamPeerBuffer = StreamPeerBuffer.new()
	buffer.put_u8(SIM.PROTOCOL)
	buffer.put_u64(int(data.tick))
	buffer.put_u32(int(data.round.id))
	buffer.put_u8(1 if data.round.phase == "complete" else 0)
	var mask: int = 0
	for index: int in LANDMARK_ORDER.size():
		if data.round.lit.has(LANDMARK_ORDER[index]):
			mask |= 1 << index
	buffer.put_u8(mask)
	buffer.put_64(int(data.round.completed_at_tick))
	var completed_name: PackedByteArray = str(data.round.completed_by).to_ascii_buffer()
	buffer.put_u8(completed_name.size())
	buffer.put_data(completed_name)
	for key: String in ["accepted_inputs", "rejected_inputs", "accepted_actions", "rejected_actions"]:
		buffer.put_u32(mini(int(data.server[key]), 4294967295))
	buffer.put_u8(data.players.size())
	for player: Dictionary in data.players:
		buffer.put_u32(str(player.id).substr(1).to_int())
		var name_bytes: PackedByteArray = str(player.name).to_ascii_buffer()
		buffer.put_u8(name_bytes.size())
		buffer.put_data(name_bytes)
		buffer.put_u8(1 if player.connected else 0)
		for coordinate: Variant in player.position:
			buffer.put_16(int(roundf(float(coordinate) * 500.0)))
		for coordinate: Variant in player.velocity:
			buffer.put_16(int(roundf(float(coordinate) * 1000.0)))
	return buffer.data_array

func _decode_snapshot(wire: PackedByteArray) -> Dictionary:
	if wire.size() < 41 or wire.size() > MAX_SNAPSHOT_BYTES:
		return {}
	var buffer: StreamPeerBuffer = StreamPeerBuffer.new()
	buffer.data_array = wire
	var protocol: int = buffer.get_u8()
	var remote_tick: int = buffer.get_u64()
	var remote_round: int = buffer.get_u32()
	var phase_code: int = buffer.get_u8()
	var lit_mask: int = buffer.get_u8()
	var completed_tick: int = buffer.get_64()
	var completed_length: int = buffer.get_u8()
	if protocol != SIM.PROTOCOL or phase_code > 1 or lit_mask > 7 or completed_length > 20 or buffer.get_available_bytes() < completed_length + 17:
		return {}
	var completed_name: String = buffer.get_utf8_string(completed_length)
	var server: Dictionary = {"capacity": SIM.CAPACITY, "tick_rate": SIM.TICK_RATE, "connected_count": 0}
	for key: String in ["accepted_inputs", "rejected_inputs", "accepted_actions", "rejected_actions"]:
		server[key] = buffer.get_u32()
	var count: int = buffer.get_u8()
	if count > SIM.CAPACITY:
		return {}
	var public_players: Array = []
	for index: int in count:
		if buffer.get_available_bytes() < 18:
			return {}
		var numeric_id: int = buffer.get_u32()
		var name_length: int = buffer.get_u8()
		if numeric_id == 0 or name_length < 2 or name_length > 20 or buffer.get_available_bytes() < name_length + 13:
			return {}
		var display_name: String = buffer.get_utf8_string(name_length)
		var connected_code: int = buffer.get_u8()
		if connected_code > 1:
			return {}
		var position: Array = []
		var velocity: Array = []
		for dimension: int in 3:
			position.append(float(buffer.get_16()) / 500.0)
		for dimension: int in 3:
			velocity.append(float(buffer.get_16()) / 1000.0)
		server.connected_count = int(server.connected_count) + connected_code
		public_players.append({"id": "p%04d" % numeric_id, "name": display_name, "connected": connected_code == 1, "position": position, "velocity": velocity})
	if buffer.get_available_bytes() != 0:
		return {}
	var lit_ids: Array = []
	var landmarks: Array = []
	for index: int in LANDMARK_ORDER.size():
		var id: String = LANDMARK_ORDER[index]
		var landmark: Dictionary = WORLD.LANDMARKS[id]
		var is_lit: bool = (lit_mask & (1 << index)) != 0
		if is_lit:
			lit_ids.append(id)
		landmarks.append({"id": id, "label": landmark.label, "position": WORLD.vector_array(landmark.position), "radius": landmark.radius, "lit": is_lit})
	lit_ids.sort()
	return {
		"protocol": protocol, "tick": remote_tick, "players": public_players, "landmarks": landmarks,
		"bell": {"position": WORLD.vector_array(WORLD.BELL_POSITION), "radius": WORLD.INTERACTION_RADIUS},
		"round": {"id": remote_round, "phase": "complete" if phase_code == 1 else "collecting", "lit": lit_ids, "completed_by": completed_name, "completed_at_tick": completed_tick},
		"server": server,
	}

func _apply_snapshot(data: Dictionary) -> void:
	if int(data.tick) < _last_snapshot_tick:
		return
	_last_snapshot_tick = int(data.tick)
	snapshot = data
	state_changed.emit(snapshot)

func _valid_snapshot(data: Dictionary) -> bool:
	if data.size() != 7 or data.get("protocol") != SIM.PROTOCOL or typeof(data.get("tick")) != TYPE_INT or int(data.tick) < 0 or typeof(data.get("players")) != TYPE_ARRAY or data.players.size() > SIM.CAPACITY:
		return false
	if typeof(data.get("round")) != TYPE_DICTIONARY or typeof(data.get("landmarks")) != TYPE_ARRAY or data.landmarks.size() != 3 or typeof(data.get("bell")) != TYPE_DICTIONARY or typeof(data.get("server")) != TYPE_DICTIONARY:
		return false
	var member_ids: Dictionary = {}
	for player: Variant in data.players:
		if typeof(player) != TYPE_DICTIONARY or player.size() != 5 or typeof(player.get("id")) != TYPE_STRING or typeof(player.get("name")) != TYPE_STRING:
			return false
		var id: String = str(player.id)
		if id.length() < 2 or id.length() > 20 or not id.begins_with("p") or not id.substr(1).is_valid_int() or member_ids.has(id):
			return false
		member_ids[id] = true
		if not SIM.valid_name(str(player.name)) or typeof(player.get("connected")) != TYPE_BOOL or not _finite_triplet(player.get("position"), 100.0) or not _finite_triplet(player.get("velocity"), 100.0):
			return false
		var position: Vector3 = WORLD.array_vector(player.position)
		if position.x < WORLD.MIN_X - 0.01 or position.x > WORLD.MAX_X + 0.01 or position.z < WORLD.MIN_Z - 0.01 or position.z > WORLD.MAX_Z + 0.01 or position.y < 0.0 or position.y > 5.0:
			return false
	var round_state: Dictionary = data.round
	if round_state.size() != 5 or typeof(round_state.get("id")) != TYPE_INT or int(round_state.id) < 1 or int(round_state.id) > 1000000000 or typeof(round_state.get("phase")) != TYPE_STRING or not ["collecting", "complete"].has(round_state.phase):
		return false
	if typeof(round_state.get("lit")) != TYPE_ARRAY or round_state.lit.size() > WORLD.LANDMARKS.size() or typeof(round_state.get("completed_by")) != TYPE_STRING or typeof(round_state.get("completed_at_tick")) != TYPE_INT:
		return false
	var lit_ids: Dictionary = {}
	for id: Variant in round_state.lit:
		if typeof(id) != TYPE_STRING or not WORLD.LANDMARKS.has(id) or lit_ids.has(id):
			return false
		lit_ids[id] = true
	if round_state.phase == "collecting" and (round_state.completed_by != "" or int(round_state.completed_at_tick) != -1):
		return false
	if round_state.phase == "complete" and (lit_ids.size() != 3 or not SIM.valid_name(str(round_state.completed_by)) or int(round_state.completed_at_tick) < 0 or int(round_state.completed_at_tick) > int(data.tick)):
		return false
	var landmark_ids: Dictionary = {}
	for landmark: Variant in data.landmarks:
		if typeof(landmark) != TYPE_DICTIONARY or landmark.size() != 5 or typeof(landmark.get("id")) != TYPE_STRING or not WORLD.LANDMARKS.has(landmark.id) or landmark_ids.has(landmark.id):
			return false
		landmark_ids[landmark.id] = true
		var expected: Dictionary = WORLD.LANDMARKS[landmark.id]
		if landmark.get("label") != expected.label or typeof(landmark.get("lit")) != TYPE_BOOL or bool(landmark.lit) != lit_ids.has(landmark.id) or not _finite_triplet(landmark.get("position"), 100.0):
			return false
		if WORLD.array_vector(landmark.position).distance_to(expected.position) > 0.01 or not _numeric_radius(landmark.get("radius")):
			return false
	if data.bell.size() != 2 or not _finite_triplet(data.bell.get("position"), 100.0) or WORLD.array_vector(data.bell.position).distance_to(WORLD.BELL_POSITION) > 0.01 or not _numeric_radius(data.bell.get("radius")):
		return false
	if data.server.get("capacity") != SIM.CAPACITY or data.server.get("tick_rate") != SIM.TICK_RATE:
		return false
	for key: String in ["connected_count", "accepted_inputs", "rejected_inputs", "accepted_actions", "rejected_actions"]:
		if typeof(data.server.get(key)) != TYPE_INT or int(data.server[key]) < 0:
			return false
	if int(data.server.connected_count) > data.players.size():
		return false
	return true

func _finite_triplet(value: Variant, maximum: float) -> bool:
	if typeof(value) != TYPE_ARRAY or value.size() != 3:
		return false
	for coordinate: Variant in value:
		if (typeof(coordinate) != TYPE_FLOAT and typeof(coordinate) != TYPE_INT) or not is_finite(float(coordinate)) or absf(float(coordinate)) > maximum:
			return false
	return true

func _numeric_radius(value: Variant) -> bool:
	return (typeof(value) == TYPE_FLOAT or typeof(value) == TYPE_INT) and is_finite(float(value)) and absf(float(value) - WORLD.INTERACTION_RADIUS) < 0.001

func _load_progress() -> Error:
	if _state_file.is_empty() or not FileAccess.file_exists(_state_file):
		return OK
	var file: FileAccess = FileAccess.open(_state_file, FileAccess.READ)
	if file == null:
		return FileAccess.get_open_error()
	if file.get_length() == 0 or file.get_length() > MAX_STATE_BYTES:
		return ERR_FILE_CORRUPT
	var data: Variant = JSON.parse_string(file.get_as_text())
	file.close()
	if typeof(data) != TYPE_DICTIONARY or not _sim.restore_progress(data):
		return ERR_FILE_CORRUPT
	return OK

func _save_progress() -> void:
	if _sim == null:
		return
	if Time.get_ticks_msec() < _save_retry_after_ms:
		return
	if _state_file.is_empty():
		_saved_revision = _sim.revision
		return
	## Same-directory rename preserves the previous complete save if writing fails.
	var parent: String = _state_file.get_base_dir()
	var directory_error: Error = DirAccess.make_dir_recursive_absolute(parent)
	if directory_error != OK:
		_persistence_failed("Could not create the state directory.")
		return
	var temporary: String = _state_file + ".tmp-" + Crypto.new().generate_random_bytes(8).hex_encode()
	var file: FileAccess = FileAccess.open(temporary, FileAccess.WRITE)
	if file == null:
		_persistence_failed("Could not write meadow state.")
		return
	if OS.has_feature("linux") or OS.has_feature("macos"):
		if FileAccess.set_unix_permissions(temporary, 0x180) != OK:
			file.close()
			DirAccess.remove_absolute(temporary)
			_persistence_failed("Could not restrict permissions on the meadow save.")
			return
	file.store_string(JSON.stringify(_sim.export_progress()) + "\n")
	file.flush()
	var write_error: Error = file.get_error()
	file.close()
	if write_error != OK:
		DirAccess.remove_absolute(temporary)
		_persistence_failed("Meadow state write failed; the previous save remains intact.")
		return
	var rename_error: Error = DirAccess.rename_absolute(temporary, _state_file)
	if rename_error != OK:
		DirAccess.remove_absolute(temporary)
		_persistence_failed("Could not replace meadow state; the previous save remains intact.")
		return
	_saved_revision = _sim.revision
	_persistence_error = ""
	_save_retry_after_ms = 0

func _persistence_failed(message: String) -> void:
	if _persistence_error != message:
		notice.emit(message)
		push_warning(message)
	_persistence_error = message
	_save_retry_after_ms = Time.get_ticks_msec() + 5000

func metrics() -> Dictionary:
	var costs: Array[int] = _tick_costs.duplicate()
	costs.sort()
	var result: Dictionary = {
		"transport": "ENet", "simulation_hz": SIM.TICK_RATE,
		"tick_samples": costs.size(), "late_frames": _late_frames,
		"tick_p50_us": 0, "tick_p95_us": 0, "tick_max_us": 0,
		"persistence_error": _persistence_error,
	}
	if not costs.is_empty():
		result.tick_p50_us = costs[int(floorf(float(costs.size() - 1) * 0.5))]
		result.tick_p95_us = costs[int(ceilf(float(costs.size() - 1) * 0.95))]
		result.tick_max_us = costs[-1]
	if _sim != null:
		result.merge(_sim.snapshot().server)
		result.tick = _sim.tick
		result.reserved_players = _sim.players.size()
		result.pending_connections = _pending.size()
	return result

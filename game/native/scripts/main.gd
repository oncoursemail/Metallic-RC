extends Node3D
## Desktop orchestration: visual client and headless server share the Session authority.

const MeadowScene = preload("res://scripts/meadow.gd")
const VisitorScene = preload("res://scripts/avatar.gd")
const MeadowHUD = preload("res://scripts/hud.gd")

var meadow: Node3D
var camera: Camera3D
var hud: CanvasLayer
var visitors: Dictionary = {}
var command_line: Dictionary = {}
var dedicated := false
var host_session := false
var camera_yaw := 0.0
var camera_pitch := -0.34
var camera_distance := 8.2
var camera_target := Vector3(0, 1.4, 15)
var mouse_sensitivity := 1.0
var reduced_motion := true
var last_round_phase := ""
var last_join: Dictionary = {}
var bell_audio: AudioStreamPlayer
var snapshot_write_timer := 0.0
var quit_pending := false
var operation_notice := ""

func _ready() -> void:
	command_line = _parse_arguments(OS.get_cmdline_user_args())
	dedicated = OS.has_feature("dedicated_server") or command_line.has("server")
	Session.state_changed.connect(_on_state)
	Session.connection_changed.connect(_on_connection)
	Session.notice.connect(_on_notice)
	Session.joined.connect(_on_joined)
	if command_line.has("state-file"):
		var configured: Error = Session.configure_state_file(str(command_line["state-file"]))
		if configured != OK:
			push_error("Invalid meadow state path: " + error_string(configured))
			get_tree().quit(2)
			return
	if dedicated:
		var result: Error = Session.start_server(_argument_port(), str(command_line.get("bind", "127.0.0.1")))
		if result != OK:
			push_error("Meadow server could not start: " + error_string(result))
			get_tree().quit(2)
			return
		_schedule_quit()
		return
	_install_input_actions()
	_build_client()
	_schedule_quit()
	var player_name := str(command_line.get("name", "Traveler"))
	if command_line.has("connect"):
		_start_join(str(command_line["connect"]), _argument_port(), player_name)
	elif command_line.has("host"):
		_start_host(player_name, _argument_port(), str(command_line.get("bind", "127.0.0.1")))
	if command_line.has("screenshot"):
		get_tree().create_timer(2.2).timeout.connect(_save_screenshot)

func _parse_arguments(args: PackedStringArray) -> Dictionary:
	var result: Dictionary = {}
	var i := 0
	while i < args.size():
		var argument: String = args[i]
		if argument.begins_with("--"):
			var pieces := argument.substr(2).split("=", true, 1)
			var key: String = pieces[0]
			if pieces.size() == 2:
				result[key] = pieces[1]
			elif i + 1 < args.size() and not args[i + 1].begins_with("--"):
				i += 1
				result[key] = args[i]
			else:
				result[key] = true
		i += 1
	return result

func _argument_port() -> int:
	return int(str(command_line.get("port", "27841")))

func _schedule_quit() -> void:
	if command_line.has("quit-after"):
		var delay := maxf(0.1, float(str(command_line["quit-after"])))
		get_tree().create_timer(delay).timeout.connect(_quit_game)

func _install_input_actions() -> void:
	var actions := {
		"meadow_left": [KEY_A, KEY_LEFT],
		"meadow_right": [KEY_D, KEY_RIGHT],
		"meadow_forward": [KEY_W, KEY_UP],
		"meadow_back": [KEY_S, KEY_DOWN],
		"meadow_jump": [KEY_SPACE],
		"meadow_interact": [KEY_E],
	}
	for action in actions:
		if not InputMap.has_action(action):
			InputMap.add_action(action)
		for key in actions[action]:
			var event := InputEventKey.new()
			event.physical_keycode = key
			InputMap.action_add_event(action, event)

func _build_client() -> void:
	meadow = MeadowScene.new()
	add_child(meadow)
	camera = Camera3D.new()
	camera.fov = 61.0
	camera.near = 0.15
	camera.far = 220.0
	add_child(camera)
	camera.make_current()
	_set_overview_camera()
	bell_audio = AudioStreamPlayer.new()
	bell_audio.stream = _make_bell_sound()
	add_child(bell_audio)
	hud = MeadowHUD.new()
	hud.host_requested.connect(_start_host)
	hud.join_requested.connect(_start_join)
	hud.leave_requested.connect(_leave_meadow)
	hud.retry_requested.connect(_retry_join)
	hud.reset_requested.connect(func() -> void: Session.reset_round())
	hud.quit_requested.connect(_quit_game)
	hud.settings_changed.connect(_apply_settings)
	hud.overlay_changed.connect(_set_overlay)
	add_child(hud)
	DisplayServer.window_set_title("Metallic-RC | Metallic Meadow")

func _set_overview_camera() -> void:
	camera.position = Vector3(40, 33, 51)
	camera.look_at(Vector3(-3, 1.5, -3), Vector3.UP)

func _start_host(display_name: String, port: int, bind_address: String) -> void:
	host_session = true
	operation_notice = ""
	last_join = {"name": display_name, "port": port, "bind": bind_address, "host": true}
	var result: Error = Session.start_host(display_name, port, bind_address)
	if result != OK:
		host_session = false
		if operation_notice.is_empty():
			_on_notice("Could not host meadow: " + error_string(result))

func _start_join(address: String, port: int, display_name: String) -> void:
	host_session = false
	operation_notice = ""
	last_join = {"name": display_name, "port": port, "address": address, "host": false}
	var result: Error = Session.join_server(address, port, display_name)
	if result != OK and operation_notice.is_empty():
		_on_notice("Could not join meadow: " + error_string(result))

func _retry_join() -> void:
	if last_join.is_empty() or last_join.get("host", false):
		return
	_start_join(str(last_join["address"]), int(last_join["port"]), str(last_join["name"]))

func _on_joined(player_id: String) -> void:
	if dedicated:
		return
	hud.enter_game(player_id, host_session)
	for id in visitors:
		visitors[id].is_local = str(id) == player_id
	var player := _local_player()
	if not player.is_empty():
		camera_target = _vector(player.get("position", [0,0,15])) + Vector3(0, 1.4, 0)
		_update_camera(1.0, true)
	if command_line.has("watch"):
		hud.toggle_map()
	elif command_line.has("pause"):
		hud.show_pause()

func _on_connection(status: String) -> void:
	print("METALLIC_CONNECTION " + status)
	if hud != null:
		hud.set_connection(status, Session.connected)

func _on_notice(message: String) -> void:
	operation_notice = message
	print("METALLIC_NOTICE " + message)
	if hud != null:
		hud.toast(message)

func _on_state(value: Dictionary) -> void:
	if dedicated or meadow == null:
		return
	var present: Dictionary = {}
	for player in value.get("players", []):
		var id: String = str(player.get("id", ""))
		if not player.get("connected", true):
			continue
		present[id] = true
		if not visitors.has(id):
			var visitor := VisitorScene.new()
			add_child(visitor)
			visitor.setup(player, id == Session.player_id)
			visitor.reduced_motion = reduced_motion
			visitors[id] = visitor
		else:
			visitors[id].set_state(player)
	for id in visitors.keys():
		if not present.has(id):
			visitors[id].queue_free()
			visitors.erase(id)
	meadow.apply_snapshot(value)
	if hud != null:
		hud.update_snapshot(value, Session.player_id)
	var round_data: Dictionary = value.get("round", {})
	var phase := str(round_data.get("phase", ""))
	if phase != last_round_phase and phase in ["complete", "completed", "celebration"]:
		meadow.ring_bell()
		bell_audio.play()
	last_round_phase = phase

func _local_player() -> Dictionary:
	for player in Session.snapshot.get("players", []):
		if str(player.get("id", "")) == Session.player_id:
			return player
	return {}

func _vector(value: Array) -> Vector3:
	if value.size() != 3:
		return Vector3.ZERO
	return Vector3(float(value[0]), float(value[1]), float(value[2]))

func _input(event: InputEvent) -> void:
	if dedicated or hud == null:
		return
	if event is InputEventKey and event.pressed and not event.echo:
		if event.keycode == KEY_ESCAPE:
			if hud.overlay_open() and hud.in_game:
				hud.close_overlays()
			elif hud.pause_layer.visible:
				hud.close_overlays()
			else:
				hud.show_pause()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_TAB and hud.in_game:
			hud.toggle_map()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_F8:
			command_line["screenshot"] = "user://meadow-screenshot.png"
			_save_screenshot()
	if not hud.overlay_open() and Session.connected:
		if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
			camera_yaw -= event.relative.x * 0.0025 * mouse_sensitivity
			camera_pitch = clampf(camera_pitch - event.relative.y * 0.0025 * mouse_sensitivity, -1.02, -0.08)
		if event is InputEventMouseButton and event.pressed:
			if event.button_index == MOUSE_BUTTON_WHEEL_UP:
				camera_distance = maxf(4.0, camera_distance - 0.65)
			elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
				camera_distance = minf(13.0, camera_distance + 0.65)
		if event.is_action_pressed("meadow_interact"):
			Session.interact()

func _physics_process(_delta: float) -> void:
	if dedicated or hud == null or not Session.connected:
		return
	if hud.overlay_open():
		Session.send_input(Vector2.ZERO, false)
		return
	var axis := Input.get_vector("meadow_left", "meadow_right", "meadow_forward", "meadow_back")
	var world_axis := Vector2(axis.x * cos(camera_yaw) + axis.y * sin(camera_yaw), axis.y * cos(camera_yaw) - axis.x * sin(camera_yaw))
	Session.send_input(world_axis, Input.is_action_just_pressed("meadow_jump"))

func _process(delta: float) -> void:
	if command_line.has("snapshot-file"):
		snapshot_write_timer += delta
		if snapshot_write_timer >= 0.5:
			snapshot_write_timer = 0.0
			_write_snapshot()
	if dedicated or camera == null or hud == null:
		return
	if hud.in_game:
		_update_camera(delta)
		_update_interaction_prompt()
	else:
		_set_overview_camera()

func _update_camera(delta: float, immediate: bool = false) -> void:
	if command_line.has("overview"):
		_set_overview_camera()
		return
	var local_id: String = Session.player_id
	if visitors.has(local_id):
		camera_target = visitors[local_id].position + Vector3(0, 1.45, 0)
	var offset := Vector3(sin(camera_yaw) * cos(camera_pitch) * camera_distance, -sin(camera_pitch) * camera_distance + 0.65, cos(camera_yaw) * cos(camera_pitch) * camera_distance)
	var desired := camera_target + offset
	# Collision-aware distance stops the camera from looking through barn walls.
	# The same authoritative rectangle data is projected onto this sight segment.
	var safe_distance := 1.0
	for rect: Rect2 in MeadowWorld.COLLIDERS:
		for i in range(1, 25):
			var t := float(i) / 24.0
			var point := camera_target.lerp(desired, t)
			if point.y < 7.2 and rect.grow(0.2).has_point(Vector2(point.x, point.z)):
				safe_distance = minf(safe_distance, maxf(0.12, t - 0.07))
				break
	desired = camera_target.lerp(desired, safe_distance)
	if immediate:
		camera.position = desired
	else:
		camera.position = camera.position.lerp(desired, 1.0 - exp(-18.0 * delta))
	camera.look_at(camera_target + Vector3(0, 0.05, 0), Vector3.UP)

func _update_interaction_prompt() -> void:
	var local := _local_player()
	if local.is_empty() or not Session.connected:
		hud.set_prompt("")
		return
	var position_value := _vector(local.get("position", [0,0,0]))
	var planar := Vector2(position_value.x, position_value.z)
	var round_data: Dictionary = Session.snapshot.get("round", {})
	var lit: Array = round_data.get("lit", [])
	for landmark in Session.snapshot.get("landmarks", []):
		var p := _vector(landmark.get("position", [0,0,0]))
		if planar.distance_to(Vector2(p.x, p.z)) <= float(landmark.get("radius", 2.4)):
			hud.set_prompt("Trail discovered. Bring everyone to the barn." if landmark.get("lit", false) else "E   Light the trail marker")
			return
	var bell: Dictionary = Session.snapshot.get("bell", {})
	var bell_position := _vector(bell.get("position", [-22,0,-21]))
	if planar.distance_to(Vector2(bell_position.x, bell_position.z)) <= float(bell.get("radius", 2.4)):
		if lit.size() < 3:
			hud.set_prompt("Find all three trail markers to ring the bell.")
		elif str(round_data.get("phase", "")) in ["complete", "completed", "celebration"]:
			hud.set_prompt("Welcome complete. Enjoy a moment together.")
		else:
			hud.set_prompt("E   Ring the community bell")
		return
	hud.set_prompt("")

func _set_overlay(open: bool) -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE if open else Input.MOUSE_MODE_CAPTURED
	if open:
		Session.send_input(Vector2.ZERO)

func _apply_settings(settings: Dictionary) -> void:
	mouse_sensitivity = float(settings.get("sensitivity", 1.0))
	reduced_motion = bool(settings.get("reduced_motion", true))
	if meadow != null:
		meadow.reduced_motion = reduced_motion
	for id in visitors:
		visitors[id].reduced_motion = reduced_motion
	if bell_audio != null:
		var volume := float(settings.get("volume", 0.65))
		bell_audio.volume_db = linear_to_db(maxf(volume, 0.0001))

func _make_bell_sound() -> AudioStreamWAV:
	# An original synthesized bell, not a bundled third-party recording.
	const RATE := 24000
	const DURATION := 1.8
	var sample_count := int(RATE * DURATION)
	var bytes := PackedByteArray()
	bytes.resize(sample_count * 2)
	for i in range(sample_count):
		var t := float(i) / float(RATE)
		var attack := minf(t * 140.0, 1.0)
		var wave := sin(TAU * 523.25 * t) * exp(-t * 2.6)
		wave += sin(TAU * 1049.1 * t) * 0.42 * exp(-t * 3.9)
		wave += sin(TAU * 1437.0 * t) * 0.17 * exp(-t * 5.2)
		bytes.encode_s16(i * 2, int(clampf(wave * attack * 0.38, -1.0, 1.0) * 32767.0))
	var stream := AudioStreamWAV.new()
	stream.format = AudioStreamWAV.FORMAT_16_BITS
	stream.mix_rate = RATE
	stream.stereo = false
	stream.data = bytes
	return stream

func _save_screenshot() -> void:
	if dedicated or DisplayServer.get_name() == "headless":
		push_warning("A rendered screenshot requires a graphical display.")
		return
	await RenderingServer.frame_post_draw
	var path := str(command_line.get("screenshot", "user://meadow-screenshot.png"))
	var image := get_viewport().get_texture().get_image()
	var result := image.save_png(path)
	if result == OK:
		print("METALLIC_SCREENSHOT " + ProjectSettings.globalize_path(path))
	else:
		push_error("Screenshot save failed: " + error_string(result))

func _write_snapshot() -> void:
	var file := FileAccess.open(str(command_line["snapshot-file"]), FileAccess.WRITE)
	if file != null:
		file.store_string(JSON.stringify(Session.snapshot, "\t"))

func _leave_meadow() -> void:
	Session.leave()
	last_round_phase = ""
	if hud != null:
		hud.show_title()
		_set_overview_camera()

func _quit_game() -> void:
	if quit_pending:
		return
	quit_pending = true
	_write_snapshot() if command_line.has("snapshot-file") else null
	Session.leave()
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	get_tree().quit()

func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_quit_game()

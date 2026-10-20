extends CanvasLayer
## Small, keyboard-accessible desktop interface. Every counter uses session state.

signal host_requested(display_name: String, port: int, bind_address: String)
signal join_requested(address: String, port: int, display_name: String)
signal leave_requested
signal retry_requested
signal reset_requested
signal quit_requested
signal settings_changed(settings: Dictionary)
signal overlay_changed(open: bool)

const INK := Color("252530")
const PANEL := Color("f1eee5")
const PLUM := Color("705568")
const MUTED := Color("b9b9b4")

var root: Control
var title_layer: Control
var game_layer: Control
var pause_layer: Control
var map_layer: Control
var display_name_input: LineEdit
var address_input: LineEdit
var port_input: SpinBox
var lan_check: CheckBox
var title_status: Label
var session_status: Label
var player_count: Label
var objective_label: Label
var objective_rows: Dictionary = {}
var prompt_panel: PanelContainer
var prompt_label: Label
var toast_label: Label
var toast_panel: PanelContainer
var toast_time := 0.0
var map_view: MeadowMap
var map_players: Label
var reset_button: Button
var retry_button: Button
var pause_status: Label
var sensitivity_slider: HSlider
var volume_slider: HSlider
var motion_check: CheckBox
var snapshot: Dictionary = {}
var is_host := false
var in_game := false
var connected := false
var local_player_id := ""
var settings := {"volume": 0.65, "sensitivity": 1.0, "reduced_motion": true}

func _ready() -> void:
	_load_settings()
	root = Control.new()
	root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)
	root.theme = _make_theme()
	_build_title()
	_build_game()
	_build_pause()
	_build_map()
	show_title()
	settings_changed.emit(settings)

func _style(background: Color, border: Color = Color.TRANSPARENT, radius: int = 12) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = background
	box.border_color = border
	box.set_border_width_all(1 if border.a > 0.0 else 0)
	box.set_corner_radius_all(radius)
	box.content_margin_left = 18
	box.content_margin_right = 18
	box.content_margin_top = 12
	box.content_margin_bottom = 12
	return box

func _make_theme() -> Theme:
	var theme := Theme.new()
	theme.default_font_size = 17
	theme.set_color("font_color", "Label", Color("eeece4"))
	theme.set_stylebox("normal", "Button", _style(Color("706070"), Color("9d8898"), 8))
	theme.set_stylebox("hover", "Button", _style(Color("826d80"), Color("c0acb8"), 8))
	theme.set_stylebox("pressed", "Button", _style(Color("554354"), Color("ad94a5"), 8))
	theme.set_stylebox("focus", "Button", _style(Color(0, 0, 0, 0), Color("f3d49e"), 8))
	theme.set_stylebox("disabled", "Button", _style(Color("45414b"), Color("686370"), 8))
	theme.set_color("font_color", "Button", Color("faf4e8"))
	theme.set_color("font_disabled_color", "Button", Color("a09a9f"))
	theme.set_stylebox("normal", "LineEdit", _style(Color("292831"), Color("68616a"), 7))
	theme.set_stylebox("focus", "LineEdit", _style(Color("292831"), Color("d1b995"), 7))
	theme.set_color("font_color", "LineEdit", Color("f4eee4"))
	theme.set_color("font_placeholder_color", "LineEdit", Color("a5a09f"))
	theme.set_color("font_color", "CheckBox", Color("e7e0d8"))
	theme.set_stylebox("normal", "CheckBox", StyleBoxEmpty.new())
	theme.set_stylebox("hover", "CheckBox", StyleBoxEmpty.new())
	theme.set_stylebox("pressed", "CheckBox", StyleBoxEmpty.new())
	theme.set_constant("separation", "VBoxContainer", 10)
	theme.set_constant("separation", "HBoxContainer", 12)
	return theme

func _label(parent: Node, value: String, size: int = 17, color: Color = Color("eeeae1")) -> Label:
	var label := Label.new()
	label.text = value
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_color_override("font_color", color)
	parent.add_child(label)
	return label

func _button(parent: Node, value: String, callback: Callable, primary: bool = false) -> Button:
	var button := Button.new()
	button.text = value
	button.custom_minimum_size.y = 45
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	if primary:
		button.add_theme_stylebox_override("normal", _style(Color("eee3cc"), Color("fff1d2"), 8))
		button.add_theme_stylebox_override("hover", _style(Color("fff0d2"), Color("fff8e7"), 8))
		button.add_theme_stylebox_override("pressed", _style(Color("cfbda1"), Color("eee1c5"), 8))
		button.add_theme_color_override("font_color", INK)
		button.add_theme_color_override("font_hover_color", INK)
		button.add_theme_color_override("font_pressed_color", INK)
	button.pressed.connect(callback)
	parent.add_child(button)
	return button

func _layer() -> Control:
	var layer := Control.new()
	layer.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	layer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(layer)
	return layer

func _backdrop(layer: Control, alpha: float = 0.52) -> void:
	var shade := ColorRect.new()
	shade.color = Color(0.10, 0.11, 0.15, alpha)
	shade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	layer.add_child(shade)

func _center_panel(layer: Control, size_value: Vector2) -> VBoxContainer:
	var panel := PanelContainer.new()
	panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	panel.offset_left = -size_value.x * 0.5
	panel.offset_right = size_value.x * 0.5
	panel.offset_top = -size_value.y * 0.5
	panel.offset_bottom = size_value.y * 0.5
	panel.add_theme_stylebox_override("panel", _style(Color("302d38"), Color("706474"), 15))
	layer.add_child(panel)
	var margin := MarginContainer.new()
	for direction in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + direction, 13)
	panel.add_child(margin)
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 13)
	margin.add_child(stack)
	return stack

func _spacer(parent: Node, height: float) -> void:
	var spacer := Control.new()
	spacer.custom_minimum_size.y = height
	parent.add_child(spacer)

func _build_title() -> void:
	title_layer = _layer()
	var title_panel := PanelContainer.new()
	title_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER_LEFT)
	title_panel.offset_left = 42
	title_panel.offset_right = 467
	title_panel.offset_top = -340
	title_panel.offset_bottom = 340
	title_panel.add_theme_stylebox_override("panel", _style(Color(0.15, 0.15, 0.19, 0.96), Color("797180"), 17))
	title_layer.add_child(title_panel)
	var content := VBoxContainer.new()
	content.add_theme_constant_override("separation", 10)
	title_panel.add_child(content)
	_label(content, "METALLIC-RC", 16, Color("d4c1cf"))
	_spacer(content, 10)
	_label(content, "A little room\nfor play.", 47, Color("f7f0dc"))
	_label(content, "Welcome to Metallic Meadow", 19, Color("c9b1c5"))
	var description := _label(content, "Follow three trails. Meet in the barn.\nRing the community bell together.", 16, Color("c8c7c0"))
	description.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_spacer(content, 2)
	_label(content, "YOUR NAME", 12, Color("aaa5ab"))
	display_name_input = LineEdit.new()
	display_name_input.text = "Traveler"
	display_name_input.max_length = 20
	display_name_input.placeholder_text = "Choose your display name"
	display_name_input.custom_minimum_size.y = 44
	content.add_child(display_name_input)
	var host_row := HBoxContainer.new()
	content.add_child(host_row)
	_label(host_row, "Address", 14, Color("c2bbc2"))
	address_input = LineEdit.new()
	address_input.text = "127.0.0.1"
	address_input.placeholder_text = "Server IP or hostname"
	address_input.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	address_input.custom_minimum_size.y = 42
	host_row.add_child(address_input)
	port_input = SpinBox.new()
	port_input.min_value = 1024
	port_input.max_value = 65535
	port_input.value = 27841
	port_input.allow_greater = false
	port_input.custom_minimum_size.x = 108
	host_row.add_child(port_input)
	lan_check = CheckBox.new()
	lan_check.text = "Allow LAN players when hosting"
	lan_check.add_theme_font_size_override("font_size", 14)
	content.add_child(lan_check)
	var actions := HBoxContainer.new()
	content.add_child(actions)
	var host_button := _button(actions, "Host meadow", func() -> void:
		title_status.text = "Opening your meadow…"
		host_requested.emit(display_name_input.text.strip_edges(), int(port_input.value), "0.0.0.0" if lan_check.button_pressed else "127.0.0.1"), true)
	host_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var join_button := _button(actions, "Join address", func() -> void:
		title_status.text = "Connecting…"
		join_requested.emit(address_input.text.strip_edges(), int(port_input.value), display_name_input.text.strip_edges()))
	join_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title_status = _label(content, "Host a private session or join a known address.", 13, Color("b7b3b6"))
	title_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title_status.custom_minimum_size.y = 42
	var footer := HBoxContainer.new()
	content.add_child(footer)
	_button(footer, "Settings", func() -> void: show_pause())
	_button(footer, "Quit", func() -> void: quit_requested.emit())
	var build_label := _label(title_layer, "METALLIC MEADOW   /   DESKTOP PREVIEW 0.1", 13, Color("f1eadb"))
	build_label.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	build_label.offset_left = -440
	build_label.offset_right = -28
	build_label.offset_top = -45
	build_label.offset_bottom = -20
	build_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT

func _build_game() -> void:
	game_layer = _layer()
	var top_left := PanelContainer.new()
	top_left.position = Vector2(24, 22)
	top_left.add_theme_stylebox_override("panel", _style(Color(0.13, 0.13, 0.17, 0.94), Color("82788a"), 10))
	game_layer.add_child(top_left)
	var brand := VBoxContainer.new()
	brand.add_theme_constant_override("separation", 2)
	top_left.add_child(brand)
	_label(brand, "METALLIC-RC  /  MEADOW", 16, Color("f1e6d2"))
	session_status = _label(brand, "Connecting", 12, Color("c6bfcd"))
	var top_right := PanelContainer.new()
	top_right.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	top_right.offset_left = -218
	top_right.offset_right = -24
	top_right.offset_top = 22
	top_right.offset_bottom = 91
	top_right.add_theme_stylebox_override("panel", _style(Color(0.13, 0.13, 0.17, 0.94), Color("82788a"), 10))
	game_layer.add_child(top_right)
	var people := VBoxContainer.new()
	people.add_theme_constant_override("separation", 2)
	top_right.add_child(people)
	player_count = _label(people, "0 visitors", 18, Color("f4ebd9"))
	_label(people, "YOUR SHARED MEADOW", 10, Color("b5acbc"))
	var task_panel := PanelContainer.new()
	task_panel.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	task_panel.offset_left = 24
	task_panel.offset_right = 332
	task_panel.offset_top = -212
	task_panel.offset_bottom = -24
	task_panel.add_theme_stylebox_override("panel", _style(Color(0.13, 0.13, 0.17, 0.94), Color("82788a"), 10))
	game_layer.add_child(task_panel)
	var task_stack := VBoxContainer.new()
	task_stack.add_theme_constant_override("separation", 7)
	task_panel.add_child(task_stack)
	objective_label = _label(task_stack, "THE MEADOW WELCOME   0 / 3", 14, Color("f0d6a7"))
	for item in [["west", "Canopy trail"], ["east", "The orchard"], ["grove", "The little green"]]:
		objective_rows[item[0]] = _label(task_stack, "◇   " + item[1], 15, Color("d4d0cd"))
	_label(task_stack, "Then meet at the barn bell.", 12, Color("ada6b2"))
	prompt_panel = PanelContainer.new()
	prompt_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	prompt_panel.offset_left = -230
	prompt_panel.offset_right = 230
	prompt_panel.offset_top = -108
	prompt_panel.offset_bottom = -61
	prompt_panel.add_theme_stylebox_override("panel", _style(Color(0.13, 0.13, 0.17, 0.96), Color("dbbe8c"), 8))
	game_layer.add_child(prompt_panel)
	prompt_label = _label(prompt_panel, "", 16, Color("f5e6c8"))
	prompt_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	prompt_panel.hide()
	var controls_panel := PanelContainer.new()
	controls_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	controls_panel.offset_left = -330
	controls_panel.offset_right = 330
	controls_panel.offset_top = -54
	controls_panel.offset_bottom = -18
	controls_panel.add_theme_stylebox_override("panel", _style(Color(0.13, 0.13, 0.17, 0.9), Color("77717f"), 8))
	controls_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	game_layer.add_child(controls_panel)
	var controls := _label(controls_panel, "WASD / arrows  Move     Mouse  Look     Space  Jump     E  Interact", 13, Color("f5eedf"))
	controls.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	var action_panel := VBoxContainer.new()
	action_panel.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	action_panel.offset_left = -264
	action_panel.offset_right = -24
	action_panel.offset_top = -135
	action_panel.offset_bottom = -24
	game_layer.add_child(action_panel)
	_button(action_panel, "Tab   Meadow watch", func() -> void: toggle_map())
	_button(action_panel, "Esc   Pause", func() -> void: show_pause())
	toast_panel = PanelContainer.new()
	toast_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	toast_panel.offset_left = -360
	toast_panel.offset_right = 360
	toast_panel.offset_top = 106
	toast_panel.offset_bottom = 152
	toast_panel.add_theme_stylebox_override("panel", _style(Color(0.13, 0.13, 0.17, 0.92), Color("82788a"), 8))
	toast_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	game_layer.add_child(toast_panel)
	toast_label = _label(toast_panel, "", 16, Color("f9f0dc"))
	toast_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	toast_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	toast_label.add_theme_color_override("font_shadow_color", Color("25212c"))
	toast_label.add_theme_constant_override("shadow_offset_x", 1)
	toast_label.add_theme_constant_override("shadow_offset_y", 2)

func _build_pause() -> void:
	pause_layer = _layer()
	_backdrop(pause_layer)
	var stack := _center_panel(pause_layer, Vector2(480, 592))
	_label(stack, "Take a breather.", 30, Color("f2e8d5"))
	pause_status = _label(stack, "Settings", 14, Color("b8adbd"))
	pause_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_spacer(stack, 4)
	_label(stack, "BELL VOLUME", 12, Color("bfb6c3"))
	volume_slider = HSlider.new()
	volume_slider.min_value = 0
	volume_slider.max_value = 1
	volume_slider.step = 0.01
	volume_slider.value = settings["volume"]
	volume_slider.custom_minimum_size.y = 24
	volume_slider.value_changed.connect(func(value: float) -> void:
		settings["volume"] = value
		_save_settings())
	stack.add_child(volume_slider)
	_label(stack, "MOUSE SENSITIVITY", 12, Color("bfb6c3"))
	sensitivity_slider = HSlider.new()
	sensitivity_slider.min_value = 0.25
	sensitivity_slider.max_value = 2.5
	sensitivity_slider.step = 0.05
	sensitivity_slider.value = settings["sensitivity"]
	sensitivity_slider.custom_minimum_size.y = 24
	sensitivity_slider.value_changed.connect(func(value: float) -> void:
		settings["sensitivity"] = value
		_save_settings())
	stack.add_child(sensitivity_slider)
	motion_check = CheckBox.new()
	motion_check.text = "Reduced decorative motion"
	motion_check.button_pressed = settings["reduced_motion"]
	motion_check.toggled.connect(func(value: bool) -> void:
		settings["reduced_motion"] = value
		_save_settings())
	stack.add_child(motion_check)
	_label(stack, "Camera shake is off. Movement animation stays readable.", 12, Color("aaa0b0"))
	_button(stack, "Return", func() -> void: close_overlays(), true)
	retry_button = _button(stack, "Reconnect to meadow", func() -> void: retry_requested.emit())
	reset_button = _button(stack, "Start another welcome trail", func() -> void:
		reset_requested.emit()
		close_overlays())
	var exit_row := HBoxContainer.new()
	stack.add_child(exit_row)
	var leave_button := _button(exit_row, "Leave meadow", func() -> void: leave_requested.emit())
	leave_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var quit_button := _button(exit_row, "Quit game", func() -> void: quit_requested.emit())
	quit_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_label(stack, "Esc returns to the meadow. The shared session keeps running.", 11, Color("a79cab"))

func _build_map() -> void:
	map_layer = _layer()
	_backdrop(map_layer)
	var stack := _center_panel(map_layer, Vector2(680, 656))
	var heading := HBoxContainer.new()
	stack.add_child(heading)
	var title := _label(heading, "Meadow watch", 28, Color("f2e8d5"))
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_button(heading, "Tab   Close", func() -> void: close_overlays())
	_label(stack, "Three trails. One community bell. Your company, live.", 14, Color("c1b6c4"))
	map_view = MeadowMap.new()
	map_view.custom_minimum_size = Vector2(540, 432)
	map_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	stack.add_child(map_view)
	map_players = _label(stack, "", 14, Color("ded7db"))
	map_players.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_label(stack, "Gold = discovered    Cream = you    Plum = other visitors", 12, Color("b8afbd"))

func show_title() -> void:
	in_game = false
	title_layer.show()
	game_layer.hide()
	pause_layer.hide()
	map_layer.hide()
	overlay_changed.emit(true)

func enter_game(player_id: String, hosting: bool) -> void:
	in_game = true
	local_player_id = player_id
	is_host = hosting
	title_layer.hide()
	game_layer.show()
	pause_layer.hide()
	map_layer.hide()
	reset_button.visible = hosting
	retry_button.visible = not hosting
	overlay_changed.emit(false)
	toast("Welcome in. Find the three trail markers, then ring the barn bell.")

func show_pause() -> void:
	map_layer.hide()
	pause_layer.show()
	pause_status.text = session_status.text if in_game else "Make yourself comfortable."
	reset_button.visible = in_game and is_host
	retry_button.visible = in_game and not is_host and not connected
	overlay_changed.emit(true)

func toggle_map() -> void:
	if not in_game:
		return
	if map_layer.visible:
		close_overlays()
	else:
		pause_layer.hide()
		map_layer.show()
		map_view.queue_redraw()
		overlay_changed.emit(true)

func close_overlays() -> void:
	pause_layer.hide()
	map_layer.hide()
	overlay_changed.emit(not in_game)

func overlay_open() -> bool:
	return not in_game or pause_layer.visible or map_layer.visible

func set_connection(status: String, active: bool) -> void:
	connected = active
	session_status.text = status
	title_status.text = status
	pause_status.text = status
	retry_button.visible = in_game and not is_host and not active

func update_snapshot(value: Dictionary, player_id: String) -> void:
	snapshot = value
	local_player_id = player_id
	var players: Array = snapshot.get("players", [])
	var live_count := 0
	var player_names: PackedStringArray = []
	for player in players:
		if player.get("connected", true):
			live_count += 1
			player_names.append(str(player.get("name", "Visitor")))
	player_count.text = "%d %s" % [live_count, "visitor" if live_count == 1 else "visitors"]
	var round_data: Dictionary = snapshot.get("round", {})
	var lit: Array = round_data.get("lit", [])
	var complete := str(round_data.get("phase", "")) in ["complete", "completed", "celebration"]
	objective_label.text = "WELCOME COMPLETE  ✓" if complete else "THE MEADOW WELCOME   %d / 3" % lit.size()
	for key in objective_rows:
		var row: Label = objective_rows[key]
		var names := {"west": "Canopy trail", "east": "The orchard", "grove": "The little green"}
		row.text = ("◆   " if key in lit else "◇   ") + names[key]
		row.add_theme_color_override("font_color", Color("f2d394") if key in lit else Color("d4d0cd"))
	map_view.snapshot = snapshot
	map_view.player_id = player_id
	if map_layer.visible:
		map_view.queue_redraw()
	var visible_names := player_names.slice(0, mini(6, player_names.size()))
	var remaining := live_count - visible_names.size()
	map_players.text = "%d in this meadow: %s%s" % [live_count, ", ".join(visible_names), " · +%d more" % remaining if remaining > 0 else ""]

func set_prompt(value: String) -> void:
	prompt_label.text = value
	prompt_panel.visible = not value.is_empty() and not overlay_open()

func toast(message: String) -> void:
	toast_label.text = message
	toast_time = 5.0
	toast_panel.show()
	if not in_game:
		title_status.text = message

func _process(delta: float) -> void:
	toast_time = maxf(0.0, toast_time - delta)
	if toast_time <= 0.0:
		toast_label.text = ""
		toast_panel.hide()
	if overlay_open():
		prompt_panel.hide()

func _load_settings() -> void:
	var config := ConfigFile.new()
	if config.load("user://meadow-settings.cfg") == OK:
		settings["volume"] = clampf(float(config.get_value("accessibility", "volume", 0.65)), 0.0, 1.0)
		settings["sensitivity"] = clampf(float(config.get_value("accessibility", "sensitivity", 1.0)), 0.25, 2.5)
		settings["reduced_motion"] = bool(config.get_value("accessibility", "reduced_motion", true))

func _save_settings() -> void:
	var config := ConfigFile.new()
	for key in settings:
		config.set_value("accessibility", key, settings[key])
	config.save("user://meadow-settings.cfg")
	settings_changed.emit(settings)

class MeadowMap extends Control:
	var snapshot: Dictionary = {}
	var player_id := ""

	func _point(x: float, z: float) -> Vector2:
		return Vector2((x + 46.0) / 92.0 * size.x, (z + 46.0) / 92.0 * size.y)

	func _map_rect(x: float, z: float, width_value: float, depth: float, color: Color) -> void:
		var a := _point(x, z)
		var b := _point(x + width_value, z + depth)
		draw_rect(Rect2(a, b - a), color)

	func _draw() -> void:
		draw_style_box(_background(), Rect2(Vector2.ZERO, size))
		_map_rect(-42, -42, 84, 84, Color("83946f"))
		for path in [[Vector2(0, 36), Vector2(0, -7)], [Vector2(-22, -7), Vector2(20, -7)], [Vector2(-22, -7), Vector2(-22, -21)], [Vector2(-20, 17), Vector2(22, 17)]]:
			draw_line(_point(path[0].x, path[0].y), _point(path[1].x, path[1].y), Color("d1c397"), 12.0, true)
		_map_rect(-31, -28, 18, 18, Color("775e73"))
		_map_rect(-32, 23, 8, 7, Color("bb946d"))
		draw_circle(_point(26, 25), 28.0, Color("a8b784"))
		for tree in [Vector2(17,-20), Vector2(24,-21), Vector2(32,-21), Vector2(29,-13), Vector2(32,-3), Vector2(-28,26), Vector2(-36,16)]:
			draw_circle(_point(tree.x, tree.y), 12.0, Color("516e55"))
		var font := ThemeDB.fallback_font
		for label in [[-28.5,-31.0,"BARN"], [15.0,-28.0,"ORCHARD"], [-36.0,36.0,"CANOPY"], [16.0,37.0,"LITTLE GREEN"]]:
			draw_string(font, _point(float(label[0]), float(label[1])), str(label[2]), HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color("25352f"))
		for landmark in snapshot.get("landmarks", []):
			var p: Array = landmark.get("position", [0,0,0])
			var at := _point(float(p[0]), float(p[2]))
			var color := Color("f3d58f") if landmark.get("lit", false) else Color("d8d6cd")
			draw_circle(at, 9, Color("4f4a50"))
			draw_circle(at, 6, color)
		var bell: Dictionary = snapshot.get("bell", {})
		var bell_p: Array = bell.get("position", [-22,0,-21])
		draw_circle(_point(float(bell_p[0]), float(bell_p[2])), 7.0, Color("f1dfaa"))
		for player in snapshot.get("players", []):
			if not player.get("connected", true):
				continue
			var p: Array = player.get("position", [0,0,0])
			var at := _point(float(p[0]), float(p[2]))
			var local := str(player.get("id", "")) == player_id
			draw_circle(at, 7.0, Color("33313d"))
			draw_circle(at, 4.7, Color("fff3d1") if local else Color("cba0c5"))
			if local:
				draw_string(font, at + Vector2(10,-9), "YOU", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color("292b30"))

	func _background() -> StyleBoxFlat:
		var style := StyleBoxFlat.new()
		style.bg_color = Color("39373e")
		style.set_corner_radius_all(8)
		return style

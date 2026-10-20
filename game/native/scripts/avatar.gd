extends Node3D
## Original articulated visitor. Rendering follows positions owned by the server.

var target_position := Vector3.ZERO
var velocity := Vector3.ZERO
var is_local := false
var reduced_motion := true
var initialized := false
var time_value := 0.0
var gait := 0.0
var visual: Node3D
var left_arm: Node3D
var right_arm: Node3D
var left_leg: Node3D
var right_leg: Node3D
var name_label: Label3D
var coat_color := Color("76596e")

func setup(player_data: Dictionary, local_player: bool) -> void:
	is_local = local_player
	var palette := [Color("76596e"), Color("527d80"), Color("b17857"), Color("758252"), Color("777ca0"), Color("a57183")]
	coat_color = palette[absi(str(player_data.get("id", "")).hash()) % palette.size()]
	_build_visitor()
	set_state(player_data, true)

func _material(color: Color) -> StandardMaterial3D:
	var value := StandardMaterial3D.new()
	value.albedo_color = color
	value.roughness = 0.84
	value.metallic_specular = 0.22
	return value

func _shape(parent: Node3D, mesh: Mesh, at: Vector3, color: Color, scale_value: Vector3 = Vector3.ONE) -> MeshInstance3D:
	var instance := MeshInstance3D.new()
	instance.mesh = mesh
	instance.position = at
	instance.material_override = _material(color)
	instance.scale = scale_value
	parent.add_child(instance)
	return instance

func _ball(parent: Node3D, at: Vector3, size: Vector3, color: Color) -> MeshInstance3D:
	var mesh := SphereMesh.new()
	mesh.radius = 1.0
	mesh.height = 2.0
	mesh.radial_segments = 12
	mesh.rings = 6
	return _shape(parent, mesh, at, color, size)

func _box(parent: Node3D, at: Vector3, size: Vector3, color: Color) -> MeshInstance3D:
	var mesh := BoxMesh.new()
	mesh.size = size
	return _shape(parent, mesh, at, color)

func _capsule(parent: Node3D, at: Vector3, radius: float, height: float, color: Color) -> MeshInstance3D:
	var mesh := CapsuleMesh.new()
	mesh.radius = radius
	mesh.height = height
	mesh.radial_segments = 10
	mesh.rings = 4
	return _shape(parent, mesh, at, color)

func _limb(at: Vector3, leg: bool, color: Color) -> Node3D:
	var pivot := Node3D.new()
	pivot.position = at
	visual.add_child(pivot)
	if leg:
		_capsule(pivot, Vector3(0, -0.21, 0), 0.14, 0.58, color)
		_ball(pivot, Vector3(0, -0.47, -0.07), Vector3(0.175, 0.15, 0.24), Color("3e3c48"))
		_box(pivot, Vector3(0, -0.56, -0.07), Vector3(0.32, 0.06, 0.4), Color("d0cec0"))
	else:
		_capsule(pivot, Vector3(0, -0.16, 0), 0.14, 0.53, color)
		_ball(pivot, Vector3(0, -0.43, 0), Vector3(0.15, 0.17, 0.14), Color("d8af8a"))
	return pivot

func _build_visitor() -> void:
	visual = Node3D.new()
	add_child(visual)
	var skin := Color("d8af8a")
	# Rounded work jacket, a small crafted-metal pin and a canvas backpack.
	_capsule(visual, Vector3(0, 1.02, 0), 0.33, 0.9, coat_color)
	_box(visual, Vector3(0, 0.82, -0.29), Vector3(0.45, 0.17, 0.055), coat_color.darkened(0.13))
	_box(visual, Vector3(0, 1.18, -0.318), Vector3(0.034, 0.42, 0.04), Color("d5d7cc"))
	var pin := _box(visual, Vector3(-0.17, 1.27, -0.291), Vector3(0.095, 0.095, 0.027), Color("e2d5b0"))
	pin.rotation.z = PI / 4.0
	_ball(visual, Vector3(0, 1.04, 0.32), Vector3(0.29, 0.31, 0.14), Color("c7ab79"))
	_box(visual, Vector3(0, 0.96, 0.445), Vector3(0.31, 0.20, 0.06), Color("b79566"))
	for x in [-0.22, 0.22]:
		_box(visual, Vector3(x, 1.22, 0.09), Vector3(0.065, 0.5, 0.40), Color("bfa47c"))
	_ball(visual, Vector3(0, 1.73, -0.01), Vector3(0.39, 0.43, 0.36), skin)
	# A shaped cap of hair, with a side part, leaves the face unobstructed.
	_ball(visual, Vector3(0, 1.94, 0.03), Vector3(0.407, 0.255, 0.355), Color("514539"))
	_ball(visual, Vector3(0.29, 1.79, 0.09), Vector3(0.13, 0.23, 0.23), Color("514539"))
	_ball(visual, Vector3(-0.30, 1.86, 0.075), Vector3(0.12, 0.17, 0.23), Color("514539"))
	_ball(visual, Vector3(-0.12, 1.96, -0.25), Vector3(0.25, 0.15, 0.16), Color("514539"))
	for x in [-0.13, 0.13]:
		_ball(visual, Vector3(x, 1.75, -0.338), Vector3(0.048, 0.065, 0.025), Color("332f33"))
		_ball(visual, Vector3(x - 0.009, 1.772, -0.363), Vector3(0.012, 0.018, 0.006), Color("f5eddb"))
		_ball(visual, Vector3(x * 1.53, 1.63, -0.31), Vector3(0.058, 0.033, 0.01), Color("c58c76"))
	_ball(visual, Vector3(0, 1.66, -0.37), Vector3(0.052, 0.048, 0.045), skin.lightened(0.05))
	_box(visual, Vector3(0, 1.56, -0.323), Vector3(0.093, 0.015, 0.021), Color("875e53"))
	for x in [-0.385, 0.385]:
		_ball(visual, Vector3(x, 1.72, 0), Vector3(0.085, 0.115, 0.08), skin)
	left_arm = _limb(Vector3(-0.43, 1.31, 0), false, coat_color)
	right_arm = _limb(Vector3(0.43, 1.31, 0), false, coat_color)
	left_leg = _limb(Vector3(-0.17, 0.62, 0), true, Color("536369"))
	right_leg = _limb(Vector3(0.17, 0.62, 0), true, Color("536369"))
	name_label = Label3D.new()
	name_label.position = Vector3(0, 2.63, 0)
	name_label.font_size = 28
	name_label.pixel_size = 0.006
	name_label.modulate = Color("fff4dc")
	name_label.outline_modulate = Color("393642")
	name_label.outline_size = 5
	name_label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	name_label.no_depth_test = false
	add_child(name_label)
	# This flat, translucent contact shadow makes feet readable even at low settings.
	var shadow_mesh := CylinderMesh.new()
	shadow_mesh.top_radius = 0.45
	shadow_mesh.bottom_radius = 0.45
	shadow_mesh.height = 0.008
	shadow_mesh.radial_segments = 24
	var shadow := _shape(self, shadow_mesh, Vector3(0, 0.008, 0), Color(0.16, 0.20, 0.14, 0.16), Vector3(1, 1, 0.75))
	var shadow_material := shadow.material_override as StandardMaterial3D
	shadow_material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	shadow.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

func set_state(data: Dictionary, snap: bool = false) -> void:
	var p: Array = data.get("position", [0, 0, 0])
	var v: Array = data.get("velocity", [0, 0, 0])
	if p.size() == 3:
		target_position = Vector3(float(p[0]), float(p[1]), float(p[2]))
	if v.size() == 3:
		velocity = Vector3(float(v[0]), float(v[1]), float(v[2]))
	if snap or not initialized or position.distance_to(target_position) > 8.0:
		position = target_position
		initialized = true
	if name_label != null:
		name_label.text = str(data.get("name", "Visitor")) + ("  ·  you" if is_local else "")

func _process(delta: float) -> void:
	if visual == null:
		return
	position = position.lerp(target_position, 1.0 - exp(-20.0 * delta))
	var speed := Vector2(velocity.x, velocity.z).length()
	time_value += delta
	gait += delta * minf(speed, 6.0) * 2.4
	var walking := clampf(speed / 4.2, 0.0, 1.0)
	var swing := sin(gait) * 0.53 * walking
	left_leg.rotation.x = swing
	right_leg.rotation.x = -swing
	left_arm.rotation.x = -swing * 0.74
	right_arm.rotation.x = swing * 0.74
	var airborne := target_position.y > 0.10
	if airborne:
		left_arm.rotation.z = 0.28
		right_arm.rotation.z = -0.28
		left_leg.rotation.x *= 0.4
		right_leg.rotation.x *= 0.4
	else:
		left_arm.rotation.z = 0.04
		right_arm.rotation.z = -0.04
	visual.position.y = absf(sin(gait)) * 0.035 * walking
	if not reduced_motion:
		visual.position.y += sin(time_value * 2.1) * 0.009 * (1.0 - walking)
	if speed > 0.15:
		var yaw := atan2(-velocity.x, -velocity.z)
		visual.rotation.y = lerp_angle(visual.rotation.y, yaw, 1.0 - exp(-12.0 * delta))

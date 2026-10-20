extends Node3D
## Authored environment. All authoritative blocking walls come from MeadowWorld.
## Decorative planting sits beside the shared routes, never across them.

const GRASS := Color("86ad68")
const LEAF := Color("537d53")
const LEAF_LIGHT := Color("93b367")
const PLUM := Color("76596e")
const PLUM_DARK := Color("493e52")
const WOOD := Color("b69066")
const WOOD_LIGHT := Color("e3c79c")
const SILVER := Color("cad2ce")
const PATH := Color("d6c59c")

var materials: Dictionary = {}
var marker_lights: Dictionary = {}
var bell_mesh: Node3D
var bell_phase: float = 0.0
var reduced_motion := true

func _ready() -> void:
	_build_environment()
	_build_ground()
	_build_paths()
	_build_barn()
	_build_orchard()
	_build_treehouse()
	_build_golf()
	_build_plaza()
	_build_boundary()
	_build_markers()
	_build_small_details()

func _material(color: Color, metallic: float = 0.0, roughness: float = 0.85) -> StandardMaterial3D:
	var key := str(color) + "/" + str(metallic) + "/" + str(roughness)
	if not materials.has(key):
		var material := StandardMaterial3D.new()
		material.albedo_color = color
		material.metallic = metallic
		material.metallic_specular = 0.22 if metallic < 0.1 else 0.5
		material.roughness = roughness
		materials[key] = material
	return materials[key]

func _mesh(parent: Node3D, mesh: Mesh, at: Vector3, color: Color, scale_value: Vector3 = Vector3.ONE) -> MeshInstance3D:
	var instance := MeshInstance3D.new()
	instance.mesh = mesh
	instance.position = at
	instance.scale = scale_value
	instance.material_override = _material(color)
	parent.add_child(instance)
	return instance

func _box(parent: Node3D, at: Vector3, size: Vector3, color: Color) -> MeshInstance3D:
	var box := BoxMesh.new()
	box.size = size
	return _mesh(parent, box, at, color)

func _cylinder(parent: Node3D, at: Vector3, bottom: float, top: float, height: float, color: Color, segments: int = 10) -> MeshInstance3D:
	var cylinder := CylinderMesh.new()
	cylinder.bottom_radius = bottom
	cylinder.top_radius = top
	cylinder.height = height
	cylinder.radial_segments = segments
	return _mesh(parent, cylinder, at, color)

func _leaf(parent: Node3D, at: Vector3, size: Vector3, color: Color) -> MeshInstance3D:
	var leaf := SphereMesh.new()
	leaf.radius = 1.0
	leaf.height = 2.0
	leaf.radial_segments = 8
	leaf.rings = 4
	return _mesh(parent, leaf, at, color, size)

func _beam(parent: Node3D, from: Vector3, to: Vector3, width: float, color: Color) -> MeshInstance3D:
	var result := _box(parent, (from + to) * 0.5, Vector3(width, from.distance_to(to), width), color)
	var direction := (to - from).normalized()
	result.quaternion = Quaternion(Vector3.UP, direction)
	return result

func _label(parent: Node3D, text_value: String, at: Vector3, size: int = 44, color: Color = Color("f9eed5")) -> Label3D:
	var label := Label3D.new()
	label.text = text_value
	label.font_size = size
	label.pixel_size = 0.009
	label.position = at
	label.modulate = color
	label.outline_size = 0
	label.shaded = false
	parent.add_child(label)
	return label

func _build_environment() -> void:
	var environment := Environment.new()
	environment.background_mode = Environment.BG_SKY
	var sky := Sky.new()
	var sky_material := ProceduralSkyMaterial.new()
	sky_material.sky_top_color = Color("82b6c6")
	sky_material.sky_horizon_color = Color("cbdcd6")
	sky_material.ground_bottom_color = Color("719761")
	sky_material.ground_horizon_color = Color("c9d4b0")
	sky_material.sun_angle_max = 18.0
	sky_material.sun_curve = 0.10
	sky.sky_material = sky_material
	environment.sky = sky
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color("d7e5ed")
	environment.ambient_light_energy = 0.22
	# Compatibility's linear tonemapper is predictable on integrated GPUs and Mesa.
	environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	environment.fog_enabled = true
	environment.fog_light_color = Color("d7dfc7")
	environment.fog_density = 0.0018
	environment.fog_sky_affect = 0.15
	var world_environment := WorldEnvironment.new()
	world_environment.environment = environment
	add_child(world_environment)
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-43, -32, 0)
	sun.light_color = Color("fff0d2")
	sun.light_energy = 0.58
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = 105.0
	sun.directional_shadow_mode = DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS
	add_child(sun)
	var fill := DirectionalLight3D.new()
	fill.rotation_degrees = Vector3(-65, 145, 0)
	fill.light_color = Color("c9dcea")
	fill.light_energy = 0.1
	add_child(fill)

func _build_ground() -> void:
	_box(self, Vector3(0, -0.65, 0), Vector3(160, 1.3, 160), GRASS)
	# The surrounding foothills remain outside the playable square.
	for item in [Vector3(-68, -3, -65), Vector3(-25, -3, -76), Vector3(28, -5, -78), Vector3(69, -4, -52), Vector3(-74, -4, 6), Vector3(73, -5, 22)]:
		_leaf(self, item, Vector3(28, 15, 24), Color("8cac79"))
	for item in [Vector3(-50, 0, -54), Vector3(0, 0, -57), Vector3(44, 0, -55), Vector3(-57, 0, 28), Vector3(54, 0, 38)]:
		_leaf(self, item + Vector3(0, -2, 0), Vector3(17, 5, 13), Color("96b779"))

func _path_segment(from: Vector2, to: Vector2, width: float = 4.0) -> void:
	var a := Vector3(from.x, 0.025, from.y)
	var b := Vector3(to.x, 0.025, to.y)
	var delta := b - a
	var segment := _box(self, (a + b) * 0.5, Vector3(width, 0.08, delta.length() + 0.25), PATH)
	segment.rotation.y = atan2(delta.x, delta.z)
	# A narrow packed-earth margin gives paths a deliberate, maintained edge.
	for sign_value in [-1.0, 1.0]:
		var lateral: Vector3 = Vector3(delta.z, 0, -delta.x).normalized() * (width * 0.5 + 0.12) * float(sign_value)
		var edge := _box(self, (a + b) * 0.5 + lateral + Vector3(0, -0.012, 0), Vector3(0.22, 0.07, delta.length()), Color("bbaa83"))
		edge.rotation.y = segment.rotation.y

func _build_paths() -> void:
	_path_segment(Vector2(0, 36), Vector2(0, 4), 5.2)
	_path_segment(Vector2(0, 4), Vector2(0, -10), 5.2)
	_path_segment(Vector2(0, -7), Vector2(-22, -7), 4.5)
	_path_segment(Vector2(-22, -7), Vector2(-22, -18), 4.5)
	_path_segment(Vector2(0, -7), Vector2(20, -7), 4.0)
	_path_segment(Vector2(20, -7), Vector2(20, -13), 3.4)
	_path_segment(Vector2(0, 17), Vector2(-20, 17), 4.0)
	_path_segment(Vector2(-20, 17), Vector2(-20, 23), 3.6)
	_path_segment(Vector2(0, 17), Vector2(22, 17), 4.0)
	_path_segment(Vector2(22, 17), Vector2(22, 20), 4.0)
	for z in [-4.0, 5.0, 14.0, 23.0, 32.0]:
		for x in [-3.5, 3.5]:
			_box(self, Vector3(x, 0.025, z), Vector3(0.36, 0.11, 0.72), SILVER.darkened(0.22))

func _build_barn() -> void:
	var barn := Node3D.new()
	barn.name = "TheCommunityBarn"
	barn.position = Vector3(-22, 0, -19)
	add_child(barn)
	# Shared authority: x[-31,-13], z[-28,-10]; 6 m south door x[-25,-19].
	_box(barn, Vector3(0, 0.045, 0), Vector3(18, 0.09, 18), Color("bcb29d"))
	_box(barn, Vector3(0, 3.5, -8.7), Vector3(18, 7, 0.6), PLUM)
	_box(barn, Vector3(-8.7, 3.5, 0), Vector3(0.6, 7, 18), PLUM)
	_box(barn, Vector3(8.7, 3.5, 0), Vector3(0.6, 7, 18), PLUM)
	_box(barn, Vector3(-6, 3.5, 8.7), Vector3(6, 7, 0.6), PLUM)
	_box(barn, Vector3(6, 3.5, 8.7), Vector3(6, 7, 0.6), PLUM)
	_box(barn, Vector3(0, 6.45, 8.7), Vector3(6, 1.1, 0.6), PLUM)
	# Wooden battens define the siding without introducing holes in collision walls.
	for i in range(19):
		var x := -8.5 + float(i) * 0.944
		if absf(x) > 3.0:
			_box(barn, Vector3(x, 3.4, 9.025), Vector3(0.065, 6.7, 0.08), PLUM.lightened(0.12))
		_box(barn, Vector3(x, 3.4, -9.03), Vector3(0.065, 6.7, 0.08), PLUM.lightened(0.12))
	for i in range(18):
		var z := -8.5 + float(i)
		for x in [-9.03, 9.03]:
			_box(barn, Vector3(x, 3.4, z), Vector3(0.08, 6.7, 0.065), PLUM.lightened(0.12))
	for x in [-8.85, -3.07, 3.07, 8.85]:
		_box(barn, Vector3(x, 3.5, 9.09), Vector3(0.28, 7.05, 0.22), WOOD_LIGHT)
	for z in [-9.0, 9.0]:
		_box(barn, Vector3(0, 7.0, z), Vector3(18.5, 0.34, 0.36), WOOD_LIGHT)
		var gable := PrismMesh.new()
		gable.size = Vector3(18, 5.0, 0.32)
		_mesh(barn, gable, Vector3(0, 9.5, z), PLUM)
		_beam(barn, Vector3(-9.2, 7.0, z + 0.18), Vector3(0, 12.1, z + 0.18), 0.32, WOOD_LIGHT)
		_beam(barn, Vector3(0, 12.1, z + 0.18), Vector3(9.2, 7.0, z + 0.18), 0.32, WOOD_LIGHT)
		_box(barn, Vector3(0, 9.2, z + 0.18), Vector3(0.28, 4.2, 0.2), WOOD_LIGHT)
	var roof_angle := atan2(5.0, 9.0)
	for sign_value in [-1.0, 1.0]:
		var roof := _box(barn, Vector3(4.6 * sign_value, 9.56, 0), Vector3(10.7, 0.28, 19.8), PLUM_DARK)
		roof.rotation.z = -roof_angle * sign_value
		for z in [-9.1, -6.5, -3.5, -0.5, 2.5, 5.5, 8.6]:
			var seam := _box(barn, Vector3(4.6 * sign_value, 9.76, z), Vector3(10.7, 0.05, 0.07), Color("716c77"))
			seam.rotation.z = roof.rotation.z
	_box(barn, Vector3(0, 12.18, 0), Vector3(0.3, 0.22, 20.0), SILVER)
	# Doors are visibly parked open against the facade, leaving a real walk-through opening.
	for sign_value in [-1.0, 1.0]:
		var door_x: float = float(sign_value) * 4.55
		_box(barn, Vector3(door_x, 2.9, 9.2), Vector3(2.9, 5.8, 0.17), PLUM_DARK)
		for y in [0.15, 5.7]:
			_box(barn, Vector3(door_x, y, 9.32), Vector3(2.85, 0.2, 0.12), WOOD_LIGHT)
		_beam(barn, Vector3(door_x - 1.35, 0.25, 9.34), Vector3(door_x + 1.35, 5.55, 9.34), 0.18, WOOD_LIGHT)
		_beam(barn, Vector3(door_x + 1.35, 0.25, 9.34), Vector3(door_x - 1.35, 5.55, 9.34), 0.18, WOOD_LIGHT)
	_box(barn, Vector3(0, 6.9, 9.35), Vector3(6.7, 1.02, 0.18), Color("34323b"))
	_label(barn, "THE COMMUNITY BARN", Vector3(0, 6.92, 9.48), 37)
	# Side windows are backed with glass; no misleading unmodeled openings.
	for z in [-4.0, 3.6]:
		_box(barn, Vector3(9.04, 4.1, z), Vector3(0.08, 2.3, 2.8), Color("6d9590"))
		for offset in [-1.45, 1.45]:
			_box(barn, Vector3(9.13, 4.1, z + offset), Vector3(0.15, 2.6, 0.16), WOOD_LIGHT)
		for y in [2.87, 5.33]:
			_box(barn, Vector3(9.13, y, z), Vector3(0.15, 0.16, 3.0), WOOD_LIGHT)
		_box(barn, Vector3(9.16, 4.1, z), Vector3(0.15, 0.14, 2.9), WOOD_LIGHT)
	# Interior rafters, welcome board and suspended metal community bell.
	for z in [-5.0, 1.0, 7.0]:
		_box(barn, Vector3(0, 6.8, z), Vector3(17.5, 0.24, 0.28), WOOD)
	_box(barn, Vector3(0, 2.6, -8.31), Vector3(7.8, 2.5, 0.18), PLUM_DARK)
	_label(barn, "THREE TRAILS. ONE MEADOW.\nFind the markers, then ring together.", Vector3(0, 2.6, -8.18), 34)
	_box(barn, Vector3(-2.0, 2.0, -2.0), Vector3(0.3, 4.0, 0.3), WOOD)
	_box(barn, Vector3(2.0, 2.0, -2.0), Vector3(0.3, 4.0, 0.3), WOOD)
	_box(barn, Vector3(0, 4.0, -2.0), Vector3(4.6, 0.35, 0.42), WOOD_LIGHT)
	bell_mesh = Node3D.new()
	bell_mesh.position = Vector3(0, 3.7, -2.0)
	barn.add_child(bell_mesh)
	_cylinder(bell_mesh, Vector3(0, -0.58, 0), 0.75, 0.32, 0.9, Color("bfc6bd"), 16).material_override = _material(Color("c7bd9c"), 0.55, 0.36)
	_cylinder(bell_mesh, Vector3(0, -1.07, 0), 0.84, 0.78, 0.12, SILVER, 16)
	_cylinder(bell_mesh, Vector3(0, -1.58, 0), 0.038, 0.038, 1.2, WOOD_LIGHT, 6)
	_leaf(bell_mesh, Vector3(0, -2.15, 0), Vector3(0.11, 0.15, 0.11), WOOD)
	_label(barn, "COMMUNITY BELL", Vector3(0, 0.9, -1.8), 25)

func _tree(at: Vector3, height: float, leaf_color: Color, variant: int = 0) -> void:
	var tree := Node3D.new()
	tree.position = at
	add_child(tree)
	# Match the footprint shared with server collision; height changes only the crown.
	_cylinder(tree, Vector3(0, height * 0.3, 0), 1.0 if variant == 99 else 0.55, 0.52 if variant == 99 else 0.32, height * 0.6, Color("75634e"), 7)
	_beam(tree, Vector3(0, height * 0.28, 0), Vector3(-height * 0.23, height * 0.6, 0.06), height * 0.07, WOOD.darkened(0.24))
	_beam(tree, Vector3(0, height * 0.38, 0), Vector3(height * 0.22, height * 0.7, -0.25), height * 0.06, WOOD.darkened(0.24))
	var clusters := [Vector3(-0.23, 0.65, 0), Vector3(0.22, 0.73, -0.05), Vector3(0, 0.88, 0.03), Vector3(-0.05, 0.66, 0.24), Vector3(0.08, 0.63, -0.25)]
	if variant == 99:
		# Lift this canopy above the treehouse roof so the wooden silhouette reads.
		clusters = [Vector3(-0.23, 0.85, -0.16), Vector3(0.22, 0.98, -0.1), Vector3(0, 1.11, -0.04), Vector3(-0.05, 0.96, 0.2), Vector3(0.08, 0.90, -0.34)]
	for i in range(clusters.size()):
		var factor := 0.28 if i == 2 else 0.31
		var leaf_shape := _leaf(tree, clusters[i] * height, Vector3(height * factor, height * (factor + 0.04), height * factor), leaf_color.lightened(float(i % 3) * 0.055))
		leaf_shape.rotation.y = float(i + variant) * 0.74
	if variant % 3 == 1:
		for fruit in [Vector3(-0.3, 0.6, 0.19), Vector3(0.27, 0.68, 0.17), Vector3(0.09, 0.64, 0.37), Vector3(-0.05, 0.81, 0.24)]:
			_leaf(tree, fruit * height, Vector3.ONE * height * 0.032, Color("d58a60"))

func _pine(at: Vector3, height: float) -> void:
	_cylinder(self, at + Vector3(0, height * 0.25, 0), 0.32, 0.18, height * 0.5, WOOD.darkened(0.22), 7)
	for i in range(3):
		var radius := height * (0.25 - float(i) * 0.053)
		_cylinder(self, at + Vector3(0, height * (0.47 + float(i) * 0.18), 0), radius, 0.06, height * 0.5, Color("4d7560").lightened(float(i) * 0.045), 7)

func _build_orchard() -> void:
	for entry in [Vector3(17, 0, -20), Vector3(24, 0, -21), Vector3(32, 0, -21), Vector3(29, 0, -13), Vector3(32, 0, -3), Vector3(21, 0, 1)]:
		_tree(entry, 7.8 + fmod(absf(entry.x), 3.0), LEAF_LIGHT.darkened(0.07), 1)
	for x in [15.0, 25.0, 33.0]:
		_box(self, Vector3(x, 0.12, -25), Vector3(5, 0.25, 2.0), Color("736e4a"))
		_flowers(Vector3(x, 0.28, -25), 10, Color("e7c687"), 0.36)
	_sign(Vector3(15, 0, -3.8), "THE ORCHARD", "Take the scenic route.", -0.1)
	_bench(Vector3(27, 0, -4), PI * 0.45)
	_box(self, Vector3(25.8, 0.45, -18), Vector3(1.3, 0.9, 1.1), WOOD)
	for y in [0.25, 0.7]:
		_box(self, Vector3(25.8, y, -17.43), Vector3(1.35, 0.06, 0.04), WOOD_LIGHT)
	for item in [Vector3(25.5, 1.05, -18.2), Vector3(26.0, 1.06, -18.1), Vector3(25.8, 1.02, -17.7)]:
		_leaf(self, item, Vector3(0.21, 0.21, 0.21), Color("d58a60"))

func _build_treehouse() -> void:
	var at := Vector3(-28, 0, 26)
	_tree(at, 13.0, LEAF, 99)
	# An observation hut sits in the canopy; ground markers remain accessible.
	_box(self, at + Vector3(0, 4.8, 0), Vector3(7.3, 0.35, 6.0), WOOD)
	for x in [-3.2, 3.2]:
		for z in [-2.5, 2.5]:
			_box(self, at + Vector3(x, 2.4, z), Vector3(0.27, 4.8, 0.27), WOOD.darkened(0.18))
	_box(self, at + Vector3(0, 6.45, -2.1), Vector3(6.0, 3.2, 0.2), Color("d0a572"))
	_box(self, at + Vector3(-2.9, 6.45, 0), Vector3(0.2, 3.2, 4.3), Color("d0a572"))
	_box(self, at + Vector3(2.9, 6.45, 0), Vector3(0.2, 3.2, 4.3), Color("d0a572"))
	for x in [-2.25, 2.25]:
		_box(self, at + Vector3(x, 5.7, 2.4), Vector3(1.6, 1.6, 0.2), WOOD_LIGHT)
		_box(self, at + Vector3(x, 7.2, 2.4), Vector3(1.6, 0.4, 0.2), WOOD_LIGHT)
	for sign_value in [-1.0, 1.0]:
		var roof := _box(self, at + Vector3(1.65 * sign_value, 8.58, 0), Vector3(3.8, 0.18, 5.8), PLUM_DARK)
		roof.rotation.z = -0.48 * sign_value
	for x in [-3.4, 3.4]:
		_box(self, at + Vector3(x, 5.5, 0), Vector3(0.15, 1.1, 5.7), WOOD_LIGHT)
	# Ladder is illustrative, with a small ground sign clearly describing the trail stop.
	for x in [-0.52, 0.52]:
		_beam(self, at + Vector3(x, 0.1, 4.2), at + Vector3(x, 4.9, 2.4), 0.13, WOOD_LIGHT)
	for i in range(12):
		var t := float(i) / 11.0
		_box(self, at + Vector3(0, lerpf(0.3, 4.8, t), lerpf(4.1, 2.4, t)), Vector3(1.15, 0.11, 0.15), WOOD)
	_sign(Vector3(-17, 0, 23.5), "CANOPY TRAIL", "The best view is the shared one.", -0.3)
	_bench(Vector3(-24, 0, 12), 0.2)
	_tree(Vector3(-36, 0, 16), 10.8, LEAF_LIGHT.darkened(0.2))
	_tree(Vector3(-33, 0, 34), 9.5, LEAF_LIGHT)

func _build_golf() -> void:
	var green := _cylinder(self, Vector3(26, 0.035, 25), 8.6, 8.6, 0.1, Color("6eaa70"), 40)
	green.scale.z = 0.77
	var putting := _cylinder(self, Vector3(26, 0.095, 25), 7.6, 7.6, 0.07, Color("95bb80"), 40)
	putting.scale.z = 0.75
	# Short decorative golf lane, with a real visit marker beside the main trail.
	_box(self, Vector3(26, 0.18, 30.2), Vector3(4.2, 0.22, 0.24), WOOD_LIGHT)
	for x in [23.8, 28.2]:
		_box(self, Vector3(x, 0.18, 26.3), Vector3(0.23, 0.22, 8.0), WOOD_LIGHT)
	_cylinder(self, Vector3(26, 0.15, 23), 0.22, 0.22, 0.035, PLUM_DARK, 20)
	_cylinder(self, Vector3(26, 1.65, 23), 0.035, 0.035, 3.0, SILVER, 8)
	_box(self, Vector3(26.55, 2.8, 23), Vector3(1.05, 0.55, 0.045), PLUM)
	_label(self, "M", Vector3(26.56, 2.8, 23.036), 32)
	_leaf(self, Vector3(26, 0.22, 29), Vector3.ONE * 0.12, Color("f9f2d9"))
	_sign(Vector3(16.5, 0, 23.5), "THE LITTLE GREEN", "A good place to slow down.", 0.18)
	_bench(Vector3(32, 0, 17), -PI * 0.45)
	_tree(Vector3(35, 0, 27), 8.8, LEAF_LIGHT)
	_tree(Vector3(24, 0, 37), 9.5, LEAF, 1)

func _build_plaza() -> void:
	_box(self, Vector3(0, 0.068, 5), Vector3(12, 0.13, 10), PATH.lightened(0.04))
	for x in [-5.5, 5.5]:
		for z in [0.0, 10.0]:
			_box(self, Vector3(x, 0.4, z), Vector3(1.4, 0.8, 1.4), PLUM_DARK)
			_box(self, Vector3(x, 0.84, z), Vector3(1.55, 0.12, 1.55), SILVER)
			_flowers(Vector3(x, 0.91, z), 7, Color("c393aa"), 0.37)
	_bench(Vector3(-7.0, 0, 6.0), PI * 0.5)
	_bench(Vector3(7.0, 0, 6.0), -PI * 0.5)
	_sign(Vector3(-6, 0, 27), "METALLIC MEADOW", "A little room for play.", 0.1)
	# A small metal arch marks the arrival; supports sit off the walking route.
	for x in [-4.2, 4.2]:
		_box(self, Vector3(x, 2.8, 32), Vector3(0.22, 5.6, 0.22), PLUM_DARK)
	_box(self, Vector3(0, 5.5, 32), Vector3(8.6, 0.22, 0.22), SILVER)
	_box(self, Vector3(0, 4.95, 32), Vector3(6.2, 0.85, 0.13), PLUM)
	_label(self, "WELCOME TO THE MEADOW", Vector3(0, 4.97, 32.10), 29)

func _fence(from: Vector2, to: Vector2) -> void:
	var length_value := from.distance_to(to)
	var count := int(ceil(length_value / 3.0))
	for i in range(count + 1):
		var p := from.lerp(to, float(i) / float(count))
		_box(self, Vector3(p.x, 0.65, p.y), Vector3(0.19, 1.3, 0.19), WOOD_LIGHT)
	for y in [0.42, 0.96]:
		_beam(self, Vector3(from.x, y, from.y), Vector3(to.x, y, to.y), 0.13, WOOD)

func _build_boundary() -> void:
	# The fence hugs the authoritative world bounds and explains where the space ends.
	_fence(Vector2(-42, -42), Vector2(42, -42))
	_fence(Vector2(-42, -42), Vector2(-42, 42))
	_fence(Vector2(42, -42), Vector2(42, 42))
	_fence(Vector2(-42, 42), Vector2(42, 42))
	var trees := [Vector3(-39, 0, -35), Vector3(-31, 0, -37), Vector3(-20, 0, -37), Vector3(-9, 0, -36), Vector3(2, 0, -35), Vector3(13, 0, -37), Vector3(25, 0, -36), Vector3(36, 0, -35), Vector3(-37, 0, -2), Vector3(37, 0, 7), Vector3(-35, 0, 39), Vector3(-20, 0, 39), Vector3(12, 0, 38)]
	for i in range(trees.size()):
		_tree(trees[i], 8.0 + float(i % 4), LEAF.lightened(float(i % 3) * 0.055), i)
	for i in range(16):
		var x := -60.0 + float(i) * 8.0
		_pine(Vector3(x, 0, -48.0 - float(i % 3) * 4.0), 11.0 + float(i % 4) * 2.0)

func _sign(at: Vector3, heading: String, subtitle: String, yaw: float = 0.0) -> void:
	var sign_node := Node3D.new()
	sign_node.position = at
	sign_node.rotation.y = yaw
	add_child(sign_node)
	for x in [-1.55, 1.55]:
		_box(sign_node, Vector3(x, 1.2, 0), Vector3(0.16, 2.4, 0.17), WOOD)
	_box(sign_node, Vector3(0, 2.16, 0), Vector3(4.0, 1.1, 0.18), PLUM_DARK)
	_box(sign_node, Vector3(0, 2.72, 0), Vector3(4.16, 0.12, 0.3), SILVER)
	_label(sign_node, heading, Vector3(0, 2.35, 0.105), 31)
	_label(sign_node, subtitle, Vector3(0, 1.98, 0.105), 17, Color("d5c5ba"))

func _bench(at: Vector3, yaw: float) -> void:
	var bench := Node3D.new()
	bench.position = at
	bench.rotation.y = yaw
	add_child(bench)
	for x in [-1.2, 1.2]:
		for z in [-0.36, 0.36]:
			_box(bench, Vector3(x, 0.36, z), Vector3(0.12, 0.7, 0.12), PLUM_DARK)
		_box(bench, Vector3(x, 0.98, -0.42), Vector3(0.12, 1.25, 0.12), PLUM_DARK)
	for z in [-0.32, 0.0, 0.32]:
		_box(bench, Vector3(0, 0.75, z), Vector3(3.0, 0.15, 0.25), WOOD_LIGHT)
	for y in [1.08, 1.40]:
		_box(bench, Vector3(0, y, -0.43), Vector3(3.0, 0.25, 0.13), WOOD)

func _flowers(at: Vector3, count: int, color: Color, spread: float) -> void:
	for i in range(count):
		var angle := float(i) * 2.39996
		var radius := sqrt(float(i)) * spread
		var p := at + Vector3(cos(angle) * radius, 0, sin(angle) * radius)
		var height := 0.25 + float(i % 3) * 0.08
		_cylinder(self, p + Vector3(0, height * 0.5, 0), 0.018, 0.012, height, LEAF, 5)
		_leaf(self, p + Vector3(0, height, 0), Vector3(0.12, 0.065, 0.12), color.lightened(float(i % 2) * 0.14))
		_leaf(self, p + Vector3(0, height + 0.04, 0), Vector3(0.04, 0.025, 0.04), Color("f6d28a"))

func _lamp(at: Vector3) -> void:
	_box(self, at + Vector3(0, 1.55, 0), Vector3(0.13, 3.1, 0.13), PLUM_DARK)
	_box(self, at + Vector3(0.25, 3.05, 0), Vector3(0.65, 0.14, 0.14), PLUM_DARK)
	_box(self, at + Vector3(0.5, 2.8, 0), Vector3(0.42, 0.50, 0.42), WOOD_LIGHT)
	_box(self, at + Vector3(0.5, 3.08, 0), Vector3(0.56, 0.08, 0.56), SILVER)
	_box(self, at + Vector3(0.5, 2.51, 0), Vector3(0.5, 0.09, 0.5), PLUM_DARK)

func _build_markers() -> void:
	for key in MeadowWorld.LANDMARKS:
		var item: Dictionary = MeadowWorld.LANDMARKS[key]
		var at: Vector3 = item["position"]
		var marker := Node3D.new()
		marker.position = at
		add_child(marker)
		_cylinder(marker, Vector3(0, 0.12, 0), 0.66, 0.62, 0.24, PLUM_DARK, 8)
		_box(marker, Vector3(0, 0.73, 0), Vector3(0.24, 1.2, 0.24), WOOD_LIGHT)
		var badge := _box(marker, Vector3(0, 1.48, 0), Vector3(0.84, 0.84, 0.16), SILVER)
		badge.rotation.z = PI / 4.0
		var core := _box(marker, Vector3(0, 1.49, 0.11), Vector3(0.46, 0.46, 0.10), PLUM)
		core.rotation.z = PI / 4.0
		marker_lights[key] = core
		_label(marker, "TRAIL MARKER", Vector3(0, 2.3, 0), 24)

func _build_small_details() -> void:
	for point in [Vector3(-4, 0, 29), Vector3(4, 0, 20), Vector3(-4, 0, 12), Vector3(4, 0, -2), Vector3(-11, 0, -4), Vector3(12, 0, -4), Vector3(-11, 0, 20), Vector3(12, 0, 20)]:
		_lamp(point)
	for patch in [Vector3(-9, 0.04, 25), Vector3(8, 0.04, 29), Vector3(-10, 0.04, -11), Vector3(10, 0.04, -13), Vector3(-35, 0.04, 6), Vector3(35, 0.04, 12)]:
		_flowers(patch, 22, Color("e8d394") if patch.x > 0 else Color("c98cac"), 0.47)
	# A few carefully placed split stones at garden edges, away from all essential routes.
	for p in [Vector3(-36, 0.35, -7), Vector3(-35, 0.25, -5.7), Vector3(35, 0.3, -28), Vector3(33.7, 0.2, -28.8)]:
		var rock := _leaf(self, p, Vector3(0.9, 0.55, 0.75), Color("a0a99a"))
		rock.rotation.y = p.z

func apply_snapshot(snapshot: Dictionary) -> void:
	for landmark in snapshot.get("landmarks", []):
		var key: String = str(landmark.get("id", ""))
		if marker_lights.has(key):
			var node: MeshInstance3D = marker_lights[key]
			node.material_override = _material(Color("f3d18a") if landmark.get("lit", false) else PLUM)

func ring_bell() -> void:
	bell_phase = 2.4

func _process(delta: float) -> void:
	if bell_mesh == null:
		return
	bell_phase = maxf(0.0, bell_phase - delta)
	bell_mesh.rotation.z = sin(bell_phase * 13.0) * 0.15 * minf(bell_phase, 1.0) if not reduced_motion else 0.0

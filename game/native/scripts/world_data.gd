class_name MeadowWorld
extends RefCounted
## The simulation and authored world share these exact metres and collision bounds.

const MIN_X: float = -42.0
const MAX_X: float = 42.0
const MIN_Z: float = -42.0
const MAX_Z: float = 42.0
const PLAYER_RADIUS: float = 0.42
const INTERACTION_RADIUS: float = 2.4
const BELL_POSITION: Vector3 = Vector3(-22.0, 0.0, -21.0)
const LANDMARKS: Dictionary = {
	"west": {"label": "Lookout lantern", "position": Vector3(-20.0, 0.0, 20.0), "radius": INTERACTION_RADIUS},
	"east": {"label": "Orchard lantern", "position": Vector3(20.0, 0.0, -10.0), "radius": INTERACTION_RADIUS},
	"grove": {"label": "Putting green lantern", "position": Vector3(22.0, 0.0, 18.0), "radius": INTERACTION_RADIUS},
}
## Rect2 uses x/z, not x/y. Barn front opens between x=-25 and x=-19.
const COLLIDERS: Array[Rect2] = [
	Rect2(-31.0, -28.0, 18.0, 0.6),
	Rect2(-31.0, -28.0, 0.6, 18.0),
	Rect2(-13.6, -28.0, 0.6, 18.0),
	Rect2(-31.0, -10.6, 6.0, 0.6),
	Rect2(-19.0, -10.6, 6.0, 0.6),
	# Authored orchard, canopy, golf and boundary tree trunks.
	Rect2(16.45, -20.55, 1.1, 1.1),
	Rect2(23.45, -21.55, 1.1, 1.1),
	Rect2(31.45, -21.55, 1.1, 1.1),
	Rect2(28.45, -13.55, 1.1, 1.1),
	Rect2(31.45, -3.55, 1.1, 1.1),
	Rect2(20.45, 0.45, 1.1, 1.1),
	Rect2(-29.0, 25.0, 2.0, 2.0),
	Rect2(-36.55, 15.45, 1.1, 1.1),
	Rect2(-33.55, 33.45, 1.1, 1.1),
	Rect2(34.45, 26.45, 1.1, 1.1),
	Rect2(23.45, 36.45, 1.1, 1.1),
	Rect2(-39.55, -35.55, 1.1, 1.1),
	Rect2(-31.55, -37.55, 1.1, 1.1),
	Rect2(-20.55, -37.55, 1.1, 1.1),
	Rect2(-9.55, -36.55, 1.1, 1.1),
	Rect2(1.45, -35.55, 1.1, 1.1),
	Rect2(12.45, -37.55, 1.1, 1.1),
	Rect2(24.45, -36.55, 1.1, 1.1),
	Rect2(35.45, -35.55, 1.1, 1.1),
	Rect2(-37.55, -2.55, 1.1, 1.1),
	Rect2(36.45, 6.45, 1.1, 1.1),
	Rect2(-35.55, 38.45, 1.1, 1.1),
	Rect2(-20.55, 38.45, 1.1, 1.1),
	Rect2(11.45, 37.45, 1.1, 1.1),
	# Four square plaza planters, 1.4 metres wide.
	Rect2(-6.2, -0.7, 1.4, 1.4),
	Rect2(4.8, -0.7, 1.4, 1.4),
	Rect2(-6.2, 9.3, 1.4, 1.4),
	Rect2(4.8, 9.3, 1.4, 1.4),
]

static func spawn_position(index: int) -> Vector3:
	var column: int = index % 8
	var row: int = (index / 8) % 4
	return Vector3((float(column) - 3.5) * 1.35, 0.0, 15.0 + float(row) * 1.35)

static func vector_array(value: Vector3) -> Array:
	return [snappedf(value.x, 0.001), snappedf(value.y, 0.001), snappedf(value.z, 0.001)]

static func array_vector(value: Array) -> Vector3:
	if value.size() != 3:
		return Vector3.ZERO
	return Vector3(float(value[0]), float(value[1]), float(value[2]))

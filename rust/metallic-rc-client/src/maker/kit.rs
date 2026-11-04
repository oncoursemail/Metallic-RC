//! What the Maker Pen makes: the shapes and spawnable props of the palette, the ShapeConfig colours and materials,
//! the network address of a maker object, and the shapes' meshes and colliders.
//!
//! Address of a maker object: `maker/<shape>/<colour id>/<material id>#m<uid>` for a shape, `<prefab address>#m<uid>`
//! for a prop. The `#m<uid>` part marks it as made with the pen and names it across respawns (undo, saves).
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};

use crate::collide::Obb;

const PALETTE: &str = include_str!("../../rooms/shape_palette.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Wedge,
    Pyramid,
    Torus,
    Capsule,
}

impl Shape {
    pub const ALL: [Shape; 8] = [Shape::Cube, Shape::Sphere, Shape::Cylinder, Shape::Cone, Shape::Wedge, Shape::Pyramid, Shape::Torus, Shape::Capsule];

    pub fn key(self) -> &'static str {
        match self {
            Shape::Cube => "cube",
            Shape::Sphere => "sphere",
            Shape::Cylinder => "cylinder",
            Shape::Cone => "cone",
            Shape::Wedge => "wedge",
            Shape::Pyramid => "pyramid",
            Shape::Torus => "torus",
            Shape::Capsule => "capsule",
        }
    }

    pub fn from_key(k: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|s| s.key() == k)
    }

    /// The mesh, fitting a 1 m box round the origin (INFERRED: our own meshes; the game builds its shapes at runtime).
    pub fn mesh(self) -> Mesh {
        match self {
            Shape::Cube => Cuboid::new(1.0, 1.0, 1.0).into(),
            Shape::Sphere => Sphere::new(0.5).mesh().uv(32, 18),
            Shape::Cylinder => Cylinder::new(0.5, 1.0).mesh().resolution(32).into(),
            Shape::Cone => Cone { radius: 0.5, height: 1.0 }.mesh().resolution(32).into(),
            Shape::Torus => Torus::new(0.2, 0.5).into(),
            Shape::Capsule => Capsule3d::new(0.25, 0.5).into(),
            Shape::Wedge => flat_mesh(&wedge_triangles()),
            Shape::Pyramid => flat_mesh(&pyramid_triangles()),
        }
    }

    /// Boxes the player bumps into and the pen aims at, in the shape's own space (INFERRED: boxes round the curved
    /// shapes; the wedge is four steps of 0.25, which the player walks up).
    pub fn colliders(self) -> Vec<Obb> {
        let b = |c: [f32; 3], s: [f32; 3]| Obb { center: Vec3::from(c), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::from(s) * 0.5 };
        match self {
            Shape::Cube => vec![b([0.0; 3], [1.0; 3])],
            Shape::Sphere => vec![b([0.0; 3], [0.8; 3])],
            Shape::Cylinder => vec![b([0.0; 3], [0.85, 1.0, 0.85])],
            Shape::Capsule => vec![b([0.0; 3], [0.45, 1.0, 0.45])],
            Shape::Torus => vec![b([0.0; 3], [1.0, 0.3, 1.0])],
            Shape::Cone | Shape::Pyramid => vec![b([0.0, -0.25, 0.0], [0.8, 0.5, 0.8]), b([0.0, 0.25, 0.0], [0.35, 0.5, 0.35])],
            Shape::Wedge => (0..4)
                .map(|i| {
                    let h = (i + 1) as f32 * 0.25;
                    b([0.0, -0.5 + h * 0.5, 0.375 - i as f32 * 0.25], [1.0, h, 0.25])
                })
                .collect(),
        }
    }
}

/// A ramp: high at the back (-z), down to the floor at the front (+z).
fn wedge_triangles() -> Vec<[Vec3; 3]> {
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let (a, b, c, d) = (v(-0.5, -0.5, 0.5), v(0.5, -0.5, 0.5), v(0.5, -0.5, -0.5), v(-0.5, -0.5, -0.5));
    let (e, f) = (v(0.5, 0.5, -0.5), v(-0.5, 0.5, -0.5));
    vec![
        [a, c, b], [a, d, c], // bottom
        [d, f, e], [d, e, c], // back
        [a, b, e], [a, e, f], // slope
        [a, f, d],            // left
        [b, c, e],            // right
    ]
}

fn pyramid_triangles() -> Vec<[Vec3; 3]> {
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let (a, b, c, d, top) = (v(-0.5, -0.5, 0.5), v(0.5, -0.5, 0.5), v(0.5, -0.5, -0.5), v(-0.5, -0.5, -0.5), v(0.0, 0.5, 0.0));
    vec![[a, c, b], [a, d, c], [a, b, top], [b, c, top], [c, d, top], [d, a, top]]
}

/// A flat-shaded mesh from counter-clockwise triangles, with box-projected UVs.
fn flat_mesh(tris: &[[Vec3; 3]]) -> Mesh {
    let (mut pos, mut nor, mut uv) = (Vec::new(), Vec::new(), Vec::new());
    for t in tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
        for p in t {
            pos.push(p.to_array());
            nor.push(n.to_array());
            let a = n.abs();
            uv.push(if a.x >= a.y && a.x >= a.z { [p.z + 0.5, 0.5 - p.y] } else if a.y >= a.z { [p.x + 0.5, p.z + 0.5] } else { [p.x + 0.5, 0.5 - p.y] });
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
}

/// A spawnable prop of the palette: the game's prefab address, a label and the half size of the box the pen aims at
/// before its colliders have loaded.
pub struct PropItem {
    pub address: &'static str,
    pub label: &'static str,
    pub half: [f32; 3],
}

/// Sandbox props from the 2026 Addressables (sizes from the world lane's inventory).
pub const PROPS: [PropItem; 12] = [
    PropItem { address: "(Cardboard_box_Sandbox)Variant", label: "Box", half: [0.55, 0.4, 0.4] },
    PropItem { address: "(Chair_Cafe_Round_Sandbox)Variant", label: "Cafe chair", half: [0.3, 0.45, 0.35] },
    PropItem { address: "(Chair_Beanbag_Sandbox)Variant", label: "Beanbag", half: [0.55, 0.35, 0.55] },
    PropItem { address: "(LawnChair_Sandbox)Variant", label: "Lawn chair", half: [0.35, 0.45, 0.35] },
    PropItem { address: "(Table_Cafe_Short_Sandbox)Variant", label: "Table", half: [0.6, 0.4, 0.6] },
    PropItem { address: "(Table_Picnic_Sandbox)Variant", label: "Picnic table", half: [0.95, 0.9, 0.4] },
    PropItem { address: "(Sandbox_GameTable_Small)", label: "Game table", half: [0.55, 0.4, 0.55] },
    PropItem { address: "(Hay_Bale_Sandbox)Variant", label: "Hay bale", half: [0.65, 0.4, 0.4] },
    PropItem { address: "(MetalBarrel_Sandbox)Variant", label: "Barrel", half: [0.4, 0.45, 0.4] },
    PropItem { address: "(Concrete_Barrier_Sandbox)Variant", label: "Barrier", half: [0.45, 0.45, 1.15] },
    PropItem { address: "(CrashPad_1x2_Thick_Sandbox)", label: "Crash pad", half: [1.0, 0.2, 0.5] },
    PropItem { address: "(Tire_Car_Sandbox)Variant", label: "Tire", half: [0.5, 0.2, 0.5] },
];

/// What a maker object is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Shape { shape: Shape, color: i32, material: i32 },
    Prop(String),
}

impl Kind {
    /// The network address with the maker mark.
    pub fn address(&self, uid: u32) -> String {
        match self {
            Kind::Shape { shape, color, material } => format!("maker/{}/{color}/{material}#m{uid:x}", shape.key()),
            Kind::Prop(a) => format!("{a}#m{uid:x}"),
        }
    }

    /// A maker object's kind and uid from its address; `None` for anything else.
    pub fn parse(address: &str) -> Option<(Kind, u32)> {
        let (base, uid) = address.rsplit_once("#m")?;
        let uid = u32::from_str_radix(uid, 16).ok()?;
        let kind = match base.strip_prefix("maker/") {
            Some(rest) => {
                let mut p = rest.split('/');
                let shape = Shape::from_key(p.next()?)?;
                Kind::Shape { shape, color: p.next()?.parse().ok()?, material: p.next()?.parse().ok()? }
            }
            None if !base.is_empty() => Kind::Prop(base.to_string()),
            None => return None,
        };
        Some((kind, uid))
    }

    pub fn label(&self) -> String {
        match self {
            Kind::Shape { shape, .. } => {
                let k = shape.key();
                k[..1].to_uppercase() + &k[1..]
            }
            Kind::Prop(a) => PROPS.iter().find(|p| p.address == a).map_or_else(|| a.clone(), |p| p.label.to_string()),
        }
    }

    /// Colliders to aim at before a prop's own have loaded (shapes: their own).
    pub fn colliders(&self) -> Vec<Obb> {
        match self {
            Kind::Shape { shape, .. } => shape.colliders(),
            Kind::Prop(a) => {
                let half = PROPS.iter().find(|p| p.address == a).map_or([0.4; 3], |p| p.half);
                vec![Obb { center: Vec3::new(0.0, half[1], 0.0), axes: [Vec3::X, Vec3::Y, Vec3::Z], half: Vec3::from(half) }]
            }
        }
    }
}

/// One colour of the ShapeConfig palette.
#[derive(Clone, Debug)]
pub struct Swatch {
    pub id: i32,
    pub rgb: [f32; 3],
    pub name: String,
}

/// One ShapeConfig material: id, smoothness, name.
#[derive(Clone, Debug)]
pub struct Finish {
    pub id: i32,
    pub smoothness: f32,
    pub name: String,
}

/// The palette's colours and materials in the file's order (colours grouped by hue).
pub fn palette() -> (Vec<Swatch>, Vec<Finish>) {
    let (mut colors, mut finishes) = (Vec::new(), Vec::new());
    for line in PALETTE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| f.get(i).and_then(|x| x.parse::<f32>().ok()).unwrap_or(1.0);
        let Some(id) = f.get(1).and_then(|x| x.parse::<i32>().ok()) else { continue };
        match f[0] {
            "color" if f.len() >= 6 => colors.push(Swatch { id, rgb: [num(2), num(3), num(4)], name: f[5..].join(" ") }),
            "material" if f.len() >= 8 => finishes.push(Finish { id, smoothness: num(4), name: f[7..].join(" ") }),
            _ => {}
        }
    }
    (colors, finishes)
}

/// ShapeMaterial ids that glow (Neon) or shine like metal (Shiny Metal, Galvanized Metal, Metal Floor Tread).
pub const NEON: i32 = 1;
pub const METALS: [i32; 3] = [6, 18, 23];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_round_trip() {
        let k = Kind::Shape { shape: Shape::Wedge, color: 13, material: 9 };
        let a = k.address(0xbeef);
        assert_eq!(a, "maker/wedge/13/9#mbeef");
        assert_eq!(Kind::parse(&a), Some((k, 0xbeef)));
        let p = Kind::Prop("(Hay_Bale_Sandbox)Variant".into());
        assert_eq!(Kind::parse(&p.address(7)), Some((p, 7)));
        assert_eq!(Kind::parse("debug/Box"), None);
        assert_eq!(Kind::parse("maker/blob/1/2#m1"), None);
        assert_eq!(Kind::parse("#m1"), None);
    }

    #[test]
    fn the_palette_has_the_game_colours_and_materials() {
        let (c, m) = palette();
        // the asset has 60 of the enum's 61 ids
        assert_eq!(c.len(), 60);
        assert!(c.iter().any(|s| s.id == 0 && s.name == "Red"));
        assert!(m.len() >= 26 && m.iter().any(|f| f.id == 9));
    }

    #[test]
    fn shapes_fit_their_box_and_the_wedge_is_walkable() {
        for s in Shape::ALL {
            let mesh = s.mesh();
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("{s:?}") };
            assert!(p.iter().all(|v| v.iter().all(|c| c.abs() <= 0.5 + 1e-4)), "{s:?}");
            assert!(!s.colliders().is_empty());
        }
        // each step of the wedge is 0.25 higher than the one before (the player's step-up)
        let tops: Vec<f32> = Shape::Wedge.colliders().iter().map(|b| b.top()).collect();
        assert_eq!(tops, vec![-0.25, 0.0, 0.25, 0.5]);
    }
}

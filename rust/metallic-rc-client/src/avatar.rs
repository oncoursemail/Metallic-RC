//! The player character, our own design built from meshes in code: a friendly bot with a rounded shell head whose
//! dark face screen shows glowing eyes and mouth, a torso with a badge and belt, two legs with boots, and floating
//! hands with glowing cuffs; optionally an antenna, headphones or a halo. Colours come from the player's `Look`.
//!
//! An avatar is a root entity at the feet (turned with the body) carrying `Rig` and `Drive`: whoever owns it fills
//! `Drive` (velocity, synced head and hand poses, talking) and `animate` poses the parts. Remote avatars get a
//! `NameTag` drawn over their head.
pub mod anim;
pub mod look;
pub mod panel;

use bevy::{
    mesh::VertexAttributeValues,
    prelude::*,
    transform::TransformSystems,
    ui::{UiSystems, UiTransform, Val2},
};

use anim::{Anim, Input};
pub use look::{Accessory, Look};
pub use panel::{LocalLook, LookPanel};

/// Heights in the avatar's own space: feet at 0, facing -z.
const HIP: f32 = 0.72;
const LEG: f32 = 0.665;
const NECK: f32 = 1.40;
/// the head shell's centre above the neck
const HEAD: f32 = 0.155;
/// the top of the head, for name tags
pub const TOP: f32 = 1.72;
/// name tags further than this are not drawn
const TAG_RANGE: f32 = 30.0;

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_kit)
            .add_systems(PostUpdate, (animate.before(TransformSystems::Propagate), place_tags.after(TransformSystems::Propagate).before(UiSystems::Prepare)));
    }
}

/// The shared meshes.
#[derive(Resource)]
pub struct Kit {
    shell: Handle<Mesh>,
    face: Handle<Mesh>,
    ball: Handle<Mesh>,
    torso: Handle<Mesh>,
    badge: Handle<Mesh>,
    belt: Handle<Mesh>,
    neck: Handle<Mesh>,
    leg: Handle<Mesh>,
    boot: Handle<Mesh>,
    hand: Handle<Mesh>,
    cuff: Handle<Mesh>,
    stalk: Handle<Mesh>,
    band: Handle<Mesh>,
    cup: Handle<Mesh>,
    halo: Handle<Mesh>,
}

/// A rounded box: a sphere pushed out toward the corners (a superellipsoid; `round` 1 is the ellipsoid, toward 0
/// the box), with its exact normals.
pub fn rounded_box(size: Vec3, round: f32) -> Mesh {
    let mut m = Sphere::new(1.0).mesh().uv(40, 24);
    let half = size / 2.0;
    let Some(VertexAttributeValues::Float32x3(ps)) = m.attribute(Mesh::ATTRIBUTE_POSITION).cloned() else { unreachable!() };
    let shaped: Vec<[f32; 3]> = ps.iter().map(|p| std::array::from_fn(|i| p[i].signum() * p[i].abs().powf(round) * half[i])).collect();
    // the surface is sum |x/a|^(2/round) = 1; its gradient is the normal
    let k = 2.0 / round;
    let normals: Vec<[f32; 3]> = shaped.iter().map(|p| {
        let g = Vec3::from_array(std::array::from_fn(|i| p[i].signum() * (p[i].abs() / half[i]).powf(k - 1.0) / half[i]));
        g.normalize_or(Vec3::Y).to_array()
    }).collect();
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, shaped);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    m
}

fn make_kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut add = |m: Mesh| meshes.add(m);
    commands.insert_resource(Kit {
        shell: add(rounded_box(Vec3::new(0.36, 0.31, 0.32), 0.45)),
        face: add(rounded_box(Vec3::new(0.29, 0.2, 0.05), 0.35)),
        ball: add(Sphere::new(1.0).mesh().uv(24, 14)),
        torso: add(rounded_box(Vec3::new(0.42, 0.62, 0.27), 0.6)),
        badge: add(Cylinder::new(0.045, 0.012).into()),
        belt: add(Torus::new(0.19, 0.235).mesh().into()),
        neck: add(Cylinder::new(0.055, 0.1).into()),
        leg: add(Capsule3d::new(0.068, 0.48).into()),
        boot: add(rounded_box(Vec3::new(0.14, 0.11, 0.24), 0.5)),
        hand: add(rounded_box(Vec3::new(0.095, 0.07, 0.115), 0.6)),
        cuff: add(Torus::new(0.03, 0.042).into()),
        stalk: add(Cylinder::new(0.008, 0.14).into()),
        band: add(Torus::new(0.17, 0.2).mesh().angle_range(0.0..=std::f32::consts::PI).into()),
        cup: add(Cylinder::new(0.065, 0.05).into()),
        halo: add(Torus::new(0.12, 0.15).into()),
    });
}

struct Mats {
    skin: Handle<StandardMaterial>,
    shirt: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
}

fn rgb(c: [u8; 3]) -> Color {
    Color::srgb_u8(c[0], c[1], c[2])
}

impl Mats {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        Mats {
            skin: materials.add(StandardMaterial { perceptual_roughness: 0.35, metallic: 0.2, ..default() }),
            shirt: materials.add(StandardMaterial { perceptual_roughness: 0.85, ..default() }),
            accent: materials.add(StandardMaterial::default()),
            dark: materials.add(StandardMaterial { base_color: Color::srgb(0.07, 0.08, 0.1), perceptual_roughness: 0.25, ..default() }),
        }
    }

    fn paint(&self, materials: &mut Assets<StandardMaterial>, look: &Look) {
        if let Some(mut m) = materials.get_mut(&self.skin) {
            m.base_color = rgb(look.skin);
        }
        if let Some(mut m) = materials.get_mut(&self.shirt) {
            m.base_color = rgb(look.shirt);
        }
        if let Some(mut m) = materials.get_mut(&self.accent) {
            m.base_color = rgb(look.accent);
            // glows: readable on the dark face screen and at a distance
            m.emissive = rgb(look.accent).to_linear() * 4.0;
        }
    }
}

/// How the avatar moves this frame; filled in by its owner.
#[derive(Component, Default, Clone)]
pub struct Drive {
    pub velocity: Vec3,
    pub grounded: Option<bool>,
    pub flying: bool,
    /// the hook for voice: the mouth moves while this is set
    pub talking: bool,
    /// where the head looks, in the world (the synced head pose)
    pub head: Option<Quat>,
    /// the hands in the world, when synced
    pub hands: [Option<Transform>; 2],
    /// the eyes above the feet (lower when crouching), when known
    pub head_height: Option<f32>,
    pub grip: [anim::Grip; 2],
}

struct Parts {
    upper: Entity,
    head: Entity,
    eyes: [Entity; 2],
    mouth: Entity,
    legs: [Entity; 2],
    hands: [Entity; 2],
    /// holds the accessory's parts; its first child moves
    extra: Entity,
}

#[derive(Component)]
pub struct Rig {
    pub anim: Anim,
    /// change it to repaint the avatar
    pub look: Look,
    /// shows only the hands (the local player's own view)
    pub first_person: bool,
    shown: Option<(Look, bool)>,
    mats: Mats,
    parts: Parts,
}

/// A remote avatar's name over its head: the UI node that shows it.
#[derive(Component)]
pub struct NameTag(pub Entity);

/// The UI node of a name tag, and the avatar it belongs to.
#[derive(Component)]
struct TagOf(Entity);

const EYE: Vec3 = Vec3::new(0.026, 0.036, 0.012);
const MOUTH: Vec3 = Vec3::new(0.05, 0.03, 0.01);

/// Spawns an avatar at `at` (feet, turned with the body); with a `name`, also its name tag.
pub fn spawn(commands: &mut Commands, kit: &Kit, materials: &mut Assets<StandardMaterial>, look: Look, at: Transform, name: Option<&str>, seed: u32) -> Entity {
    let mats = Mats::new(materials);
    let m = |h: &Handle<StandardMaterial>| MeshMaterial3d(h.clone());
    let part = |c: &mut Commands, parent: Entity, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform| {
        c.spawn((Mesh3d(mesh.clone()), m(mat), t, ChildOf(parent))).id()
    };
    let root = commands.spawn((at, Visibility::default())).id();

    let upper = commands.spawn((Transform::from_xyz(0.0, HIP, 0.0), Visibility::default(), ChildOf(root))).id();
    part(commands, upper, &kit.torso, &mats.shirt, Transform::from_xyz(0.0, 1.04 - HIP, 0.0));
    part(commands, upper, &kit.badge, &mats.accent,
        Transform::from_xyz(-0.08, 1.2 - HIP, -0.132).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)));
    part(commands, upper, &kit.belt, &mats.dark, Transform::from_xyz(0.0, 0.8 - HIP, 0.0).with_scale(Vec3::new(1.05, 1.0, 0.66)));
    part(commands, upper, &kit.neck, &mats.dark, Transform::from_xyz(0.0, NECK - 0.02 - HIP, 0.0));
    let head = commands.spawn((Transform::from_xyz(0.0, NECK - HIP, 0.0), Visibility::default(), ChildOf(upper))).id();
    part(commands, head, &kit.shell, &mats.skin, Transform::from_xyz(0.0, HEAD, 0.0));
    part(commands, head, &kit.face, &mats.dark, Transform::from_xyz(0.0, HEAD - 0.005, -0.152));
    let eyes = [-1.0, 1.0].map(|s| part(commands, head, &kit.ball, &mats.accent, Transform::from_xyz(s * 0.06, HEAD + 0.015, -0.176).with_scale(EYE)));
    let mouth = part(commands, head, &kit.ball, &mats.accent, Transform::from_xyz(0.0, HEAD - 0.05, -0.176).with_scale(MOUTH));
    let extra = commands.spawn((Transform::default(), Visibility::default(), ChildOf(head))).id();

    let legs = [-1.0, 1.0].map(|s| {
        let pivot = commands.spawn((Transform::from_xyz(s * 0.1, HIP, 0.0), Visibility::default(), ChildOf(root))).id();
        part(commands, pivot, &kit.leg, &mats.dark, Transform::from_xyz(0.0, -0.3, 0.0));
        part(commands, pivot, &kit.boot, &mats.skin, Transform::from_xyz(0.0, -LEG, -0.04));
        pivot
    });
    let hands = [-1.0, 1.0].map(|s| {
        let hand = commands.spawn((Transform::default(), Visibility::default(), ChildOf(root))).id();
        part(commands, hand, &kit.hand, &mats.skin, Transform::default());
        // the thumb on the inner side
        part(commands, hand, &kit.ball, &mats.skin, Transform::from_xyz(-s * 0.05, 0.012, -0.01).with_scale(Vec3::splat(0.026)));
        part(commands, hand, &kit.cuff, &mats.accent,
            Transform::from_xyz(0.0, -0.002, 0.052).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.9, 1.0, 0.75)));
        hand
    });

    let parts = Parts { upper, head, eyes, mouth, legs, hands, extra };
    commands.entity(root).insert((Rig { anim: Anim::new(seed), look, first_person: false, shown: None, mats, parts }, Drive::default()));
    if let Some(name) = name {
        let tag = commands.spawn((
            Node { position_type: PositionType::Absolute, padding: UiRect::axes(px(8.0), px(3.0)), border_radius: BorderRadius::all(px(6.0)), ..default() },
            UiTransform { translation: Val2::percent(-50.0, -100.0), ..default() },
            BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.6)),
            Visibility::Hidden,
            TagOf(root),
        )).with_child((Text::new(name), TextFont::from_font_size(17.0), TextColor(Color::WHITE))).id();
        commands.entity(root).insert(NameTag(tag));
    }
    root
}

/// Spawns the parts of an accessory under the head's holder.
fn dress(commands: &mut Commands, kit: &Kit, mats: &Mats, extra: Entity, a: Accessory) {
    let top = HEAD + 0.155;
    let mut part = |mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform, parent: Entity| {
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t, ChildOf(parent))).id()
    };
    let quarter = std::f32::consts::FRAC_PI_2;
    match a {
        Accessory::None => {}
        Accessory::Antenna => {
            // the stalk is the sway pivot, standing on the head; the tip rides on it
            let stalk = part(&kit.stalk, &mats.dark, Transform::from_xyz(0.1, top, 0.02), extra);
            part(&kit.ball, &mats.accent, Transform::from_xyz(0.0, 0.08, 0.0).with_scale(Vec3::splat(0.028)), stalk);
        }
        Accessory::Headphones => {
            // a half ring over the top, cups over the sides
            part(&kit.band, &mats.dark, Transform::from_xyz(0.0, HEAD + 0.01, 0.0).with_rotation(Quat::from_rotation_x(-quarter)), extra);
            for s in [-1.0, 1.0] {
                let cup = Transform::from_xyz(s * 0.19, HEAD - 0.01, 0.0).with_rotation(Quat::from_rotation_z(quarter));
                part(&kit.cup, &mats.shirt, cup, extra);
                part(&kit.cuff, &mats.accent, cup.with_translation(cup.translation + Vec3::X * s * 0.026).with_scale(Vec3::new(1.0, 0.4, 1.0)), extra);
            }
        }
        Accessory::Halo => {
            part(&kit.halo, &mats.accent, Transform::from_xyz(0.0, top + 0.1, 0.0), extra);
        }
    }
}

/// Poses every avatar from its `Drive` and repaints or re-dresses it when its look or view changed.
#[allow(clippy::type_complexity)]
fn animate(
    mut commands: Commands,
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rigs: Query<(&Transform, &mut Rig, &Drive)>,
    mut parts: Query<&mut Transform, Without<Rig>>,
    mut visibility: Query<&mut Visibility>,
    children: Query<&Children>,
) {
    let Some(kit) = kit else { return };
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    for (root, mut rig, drive) in &mut rigs {
        let rig = &mut *rig;
        if rig.shown.is_none_or(|(l, fp)| l != rig.look || fp != rig.first_person) {
            rig.mats.paint(&mut materials, &rig.look);
            if rig.shown.is_none_or(|(l, _)| l.accessory != rig.look.accessory) {
                commands.entity(rig.parts.extra).despawn_related::<Children>();
                dress(&mut commands, &kit, &rig.mats, rig.parts.extra, rig.look.accessory);
            }
            let hide = if rig.first_person { Visibility::Hidden } else { Visibility::Inherited };
            for e in [rig.parts.upper, rig.parts.legs[0], rig.parts.legs[1]] {
                if let Ok(mut v) = visibility.get_mut(e) {
                    *v = hide;
                }
            }
            rig.shown = Some((rig.look, rig.first_person));
        }

        rig.anim.step(dt, Input { velocity: drive.velocity, grounded: drive.grounded, flying: drive.flying, talking: drive.talking });
        let pose = rig.anim.pose();
        let p = &rig.parts;
        let lean = Quat::from_rotation_x(-pose.lean);
        if let Ok(mut tr) = parts.get_mut(p.upper) {
            *tr = Transform::from_xyz(0.0, HIP + pose.bob, 0.0).with_rotation(lean);
        }
        for (i, e) in p.legs.into_iter().enumerate() {
            if let Ok(mut tr) = parts.get_mut(e) {
                tr.rotation = Quat::from_rotation_x(pose.legs[i]);
            }
        }
        // the head turns to the synced look, within what a neck allows, and undoes the torso's lean
        let (yaw, pitch) = drive.head.map_or((0.0, 0.0), |h| {
            let (y, x, _) = (root.rotation.inverse() * h).to_euler(EulerRot::YXZ);
            (y.clamp(-1.2, 1.2), x.clamp(-0.9, 0.9))
        });
        if let Ok(mut tr) = parts.get_mut(p.head) {
            tr.rotation = lean.inverse() * Quat::from_euler(EulerRot::YXZ, yaw, pitch * 0.8, 0.0);
        }
        for e in p.eyes {
            if let Ok(mut tr) = parts.get_mut(e) {
                tr.scale = EYE * Vec3::new(1.0, pose.eyes.max(0.08), 1.0);
            }
        }
        if let Ok(mut tr) = parts.get_mut(p.mouth) {
            // a flat line that opens into an oval
            tr.scale = MOUTH * Vec3::new(1.0 - 0.3 * pose.mouth, 0.25 + 0.95 * pose.mouth, 1.0);
        }
        let to_local = root.compute_affine().inverse();
        for (i, e) in p.hands.into_iter().enumerate() {
            let Ok(mut tr) = parts.get_mut(e) else { continue };
            *tr = match drive.hands[i] {
                Some(w) => Transform::from_matrix(Mat4::from(to_local) * w.to_matrix()),
                None => Transform::from_translation(pose.hands[i]).with_rotation(Quat::from_rotation_x(-0.2)),
            };
        }
        // the accessory's moving part: the antenna sways with the stride, the halo turns
        if let Some(&first) = children.get(p.extra).ok().and_then(|c| c.first()) {
            if let Ok(mut tr) = parts.get_mut(first) {
                match rig.look.accessory {
                    Accessory::Antenna => {
                        let sway = 0.12 * rig.anim.phase.sin() * (rig.anim.speed / 3.0).min(1.0) + 0.04 * (t * 1.7).sin();
                        tr.rotation = Quat::from_rotation_z(sway) * Quat::from_rotation_x(0.6 * pose.lean + 0.05);
                    }
                    Accessory::Halo => {
                        tr.rotation = Quat::from_rotation_y(t * 0.8);
                        tr.translation.y = HEAD + 0.255 + 0.012 * (t * 2.0).sin();
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Keeps each name tag over its avatar's head, readable at any distance up to `TAG_RANGE`; drops tags whose avatar is
/// gone.
fn place_tags(
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    owners: Query<&GlobalTransform, With<NameTag>>,
    mut tags: Query<(Entity, &TagOf, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, eye)) = camera.single() else { return };
    for (e, of, mut node, mut vis) in &mut tags {
        let Ok(at) = owners.get(of.0) else {
            commands.entity(e).despawn();
            continue;
        };
        let top = at.transform_point(Vec3::Y * (TOP + 0.12));
        let near = top.distance(eye.translation()) < TAG_RANGE && (top - eye.translation()).dot(*eye.forward()) > 0.0;
        match camera.world_to_viewport(eye, top).ok().filter(|_| near) {
            Some(p) => {
                node.left = px(p.x);
                node.top = px(p.y);
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// The screen-mode hands of a player looking from `eye`: low in front of the view so they sit at the bottom of the
/// screen, swaying by `sway` (from turning) and `bob` (from walking).
pub fn screen_hands(eye: &Transform, sway: Vec2, bob: f32) -> [Transform; 2] {
    [-1.0, 1.0].map(|s| {
        let local = Vec3::new(s * 0.22 + sway.x, -0.21 + sway.y + bob * (1.0 + 0.3 * s), -0.56);
        Transform::from_translation(eye.transform_point(local))
            .with_rotation(eye.rotation * Quat::from_rotation_x(0.25) * Quat::from_rotation_y(-s * 0.15))
    })
}

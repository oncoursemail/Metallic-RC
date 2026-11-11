//! The player character, our own design ("Pip"; the design sheet is in docs/agents/metallic-rc-avatar2-20261104.md):
//! a soft rounded robot with a pebble-shaped head whose dark face screen shows glowing eyes and mouth, round ear pods,
//! an egg-shaped body over a rounded hip bell, stubby legs with chunky boots, round shoulder pods and floating mitten
//! hands with glowing cuffs. The look (`look`) sets the colour zones (shell, shirt, pants, screen, accent), the eye
//! style, a head accessory and an outfit piece (`gear`). Every part is a smooth mesh built in code (`mesh`) in a soft
//! rim-lit material (`toon`) with a dark outline, so the robot reads against bright rooms.
//!
//! An avatar is a root entity at the feet (turned with the body) carrying `Rig` and `Drive`: whoever owns it fills
//! `Drive` (velocity, synced head and hand poses, grips) and `animate` poses the parts. It blinks, breathes and sways;
//! the voice's loudness (`voice::Loudness`) moves the mouth and pulses the screen; the eyes glide across the screen
//! toward whoever talks nearby or the camera, else glance about. Remote avatars get a `NameTag` over their head.
pub mod anim;
pub mod face;
pub mod gear;
pub mod look;
pub mod mesh;
pub mod panel;
pub mod real;
pub mod skin;
pub mod toon;

use bevy::{
    light::NotShadowCaster,
    prelude::*,
    render::render_resource::Face,
    transform::TransformSystems,
    ui::{UiSystems, UiTransform, Val2},
};

use anim::{Anim, Input};
pub use look::{Accessory, Eyes, Look, Outfit};
pub use panel::{LocalLook, LookPanel};
use toon::{toon, Toon};

/// Heights in the avatar's own space: feet at 0, facing -z.
const HIP: f32 = 0.72;
const NECK: f32 = 1.40;
/// the head shell's centre above the neck
const HEAD: f32 = 0.155;
/// the waist above the hips, where the body's breathing scales from
const WAIST: f32 = 0.12;
/// the legs either side of the middle, and the boots' half height
const STANCE: f32 = 0.085;
const BOOT: f32 = 0.06;
/// the top of the head, for name tags
pub const TOP: f32 = 1.72;
/// name tags further than this are not drawn
const TAG_RANGE: f32 = 30.0;
/// eyes above the feet standing and fully crouched (the player's 1.55 m and crouch height 0.725), to tell how far a
/// player crouches from its synced head height
const STAND_EYE: f32 = 1.55;
const CROUCH_EYE: f32 = 1.55 * 0.725;
/// the hands' size against the meshes'
const HAND: f32 = 1.15;
/// outline widths: the body's, and small parts'
const LINE: f32 = 0.008;
const FINE: f32 = 0.0045;
/// the eyes follow the camera this close, and a talking avatar this close
const SEE: f32 = 8.0;
const HEAR: f32 = 6.0;

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "avatar/toon.wesl");
        app.add_plugins(MaterialPlugin::<Toon>::default())
            .add_systems(Startup, (make_kit, real::load_roomie))
            .add_systems(Update, (real::attach_real, real::idle, real::animate_face))
            .add_systems(PostUpdate, (dress, animate).chain().before(TransformSystems::Propagate))
            .add_systems(PostUpdate, real::animate_skeleton.before(TransformSystems::Propagate))
            .add_systems(PostUpdate, place_tags.after(TransformSystems::Propagate).before(UiSystems::Prepare));
    }
}

/// A mesh and its outline (the mesh grown and drawn inside out), if it has one.
#[derive(Clone)]
struct Shape {
    mesh: Handle<Mesh>,
    hull: Option<Handle<Mesh>>,
}

/// The shared meshes and materials.
#[derive(Resource)]
pub struct Kit {
    shell: Shape,
    bezel: Shape,
    screen: Shape,
    ball: Shape,
    happy: Shape,
    smile: Shape,
    ear: Shape,
    ring: Shape,
    neck: Shape,
    torso: Shape,
    core: Shape,
    shoulder: Shape,
    hips: Shape,
    leg: Shape,
    boot: Shape,
    sole: Shape,
    palm: Shape,
    mitten: Shape,
    finger: Shape,
    thumb: Shape,
    cuff: Shape,
    gear: gear::Shapes,
    outline: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    joint: Handle<Toon>,
    leaf: Handle<Toon>,
}

fn make_kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut plain: ResMut<Assets<StandardMaterial>>, mut toons: ResMut<Assets<Toon>>) {
    let mut shape = |m: Mesh, line: f32| Shape { hull: (line > 0.0).then(|| meshes.add(mesh::hull(&m, line))), mesh: meshes.add(m) };
    let v2 = Vec2::new;
    let (s, b) = (face::SCREEN, face::BEZEL);
    let kit = Kit {
        shell: shape(mesh::head(), LINE),
        bezel: shape(mesh::head_patch(b.0, b.1, b.2, 0.002), 0.0),
        screen: shape(mesh::head_patch(s.0, s.1, s.2, 0.0045), 0.0),
        ball: shape(Sphere::new(1.0).mesh().uv(24, 14), 0.0),
        happy: shape(mesh::arc(0.022, 0.0062, 2.3), 0.0),
        smile: shape(mesh::arc(0.026, 0.0052, 1.9).rotated_by(Quat::from_rotation_z(std::f32::consts::PI)), 0.0),
        ear: shape(mesh::rounded_box(Vec3::new(0.05, 0.1, 0.1), 0.72), FINE),
        ring: shape(Torus::new(0.022, 0.031).into(), 0.0),
        neck: shape(Sphere::new(0.052).mesh().uv(24, 14), FINE),
        torso: shape(mesh::lathe(&[v2(0.0, 0.0), v2(0.135, 0.01), v2(0.158, 0.07), v2(0.194, 0.21), v2(0.207, 0.33), v2(0.196, 0.44), v2(0.155, 0.51),
            v2(0.08, 0.55), v2(0.0, 0.555)], 0.82), LINE),
        core: shape(Torus::new(0.032, 0.044).into(), 0.0),
        shoulder: shape(mesh::rounded_box(Vec3::new(0.115, 0.105, 0.115), 0.8), LINE),
        hips: shape(mesh::lathe(&[v2(0.0, -0.13), v2(0.13, -0.125), v2(0.168, -0.08), v2(0.176, 0.0), v2(0.164, 0.09), v2(0.14, 0.15),
            v2(0.0, 0.16)], 0.86), LINE),
        leg: shape(mesh::lathe(&[v2(0.0, -0.62), v2(0.062, -0.615), v2(0.07, -0.57), v2(0.076, -0.3), v2(0.083, -0.08), v2(0.07, 0.0),
            v2(0.0, 0.02)], 1.0), LINE),
        boot: shape(mesh::rounded_box(Vec3::new(0.165, 0.13, 0.26), 0.55), LINE),
        sole: shape(mesh::rounded_box(Vec3::new(0.17, 0.03, 0.266), 0.4), 0.0),
        palm: shape(mesh::rounded_box(Vec3::new(0.088, 0.042, 0.08), 0.7), FINE),
        mitten: shape(mesh::rounded_box(Vec3::new(0.062, 0.036, 0.072), 0.75), FINE),
        finger: shape(Capsule3d::new(0.0155, 0.04).mesh().longitudes(16).latitudes(12).build().rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), FINE),
        thumb: shape(Capsule3d::new(0.0165, 0.03).mesh().longitudes(16).latitudes(12).build().rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), FINE),
        cuff: shape(Torus::new(0.036, 0.043).into(), 0.0),
        gear: gear::Shapes::make(&mut shape),
        outline: plain.add(StandardMaterial { base_color: Color::srgb(0.07, 0.075, 0.11), unlit: true, cull_mode: Some(Face::Front), ..default() }),
        white: plain.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() }),
        joint: toons.add(toon(StandardMaterial { base_color: Color::srgb(0.17, 0.18, 0.22), perceptual_roughness: 0.45, ..default() }, 0.2, 0.06)),
        leaf: toons.add(toon(StandardMaterial { base_color: Color::srgb(0.36, 0.76, 0.38), perceptual_roughness: 0.6, ..default() }, 0.3, 0.12)),
    };
    commands.insert_resource(kit);
}

/// A part's material: soft (lit, with rim) or plain (the unlit glows and the outline).
#[derive(Clone)]
enum Mat {
    Toon(Handle<Toon>),
    Plain(Handle<StandardMaterial>),
}

/// One avatar's own materials, repainted from its look.
struct Mats {
    skin: Handle<Toon>,
    shirt: Handle<Toon>,
    pants: Handle<Toon>,
    screen: Handle<Toon>,
    /// accessories in the accent colour that are cloth, not light (scarf, bow tie)
    cloth: Handle<Toon>,
    glow: Handle<StandardMaterial>,
}

fn rgb(c: [u8; 3]) -> Color {
    Color::srgb_u8(c[0], c[1], c[2])
}

impl Mats {
    fn new(toons: &mut Assets<Toon>, plain: &mut Assets<StandardMaterial>) -> Self {
        let soft = |toons: &mut Assets<Toon>, roughness: f32, rim: f32, fill: f32| {
            toons.add(toon(StandardMaterial { perceptual_roughness: roughness, reflectance: 0.4, ..default() }, rim, fill))
        };
        Mats {
            skin: soft(toons, 0.38, 0.4, 0.1),
            shirt: soft(toons, 0.8, 0.35, 0.1),
            pants: soft(toons, 0.7, 0.3, 0.08),
            screen: soft(toons, 0.12, 0.18, 0.0),
            cloth: soft(toons, 0.85, 0.35, 0.1),
            glow: plain.add(StandardMaterial { unlit: true, ..default() }),
        }
    }

    fn paint(&self, toons: &mut Assets<Toon>, plain: &mut Assets<StandardMaterial>, look: &Look, pulse: f32) {
        for (h, c) in [(&self.skin, look.skin), (&self.shirt, look.shirt), (&self.pants, look.pants), (&self.screen, look.visor), (&self.cloth, look.accent)] {
            if let Some(mut m) = toons.get_mut(h) {
                m.base.base_color = rgb(c);
            }
        }
        self.pulse(toons, plain, look, pulse);
    }

    /// The lights brighten and the screen glows a little with the voice (`pulse` 0..1).
    fn pulse(&self, toons: &mut Assets<Toon>, plain: &mut Assets<StandardMaterial>, look: &Look, pulse: f32) {
        let accent = rgb(look.accent).to_linear();
        if let Some(mut m) = plain.get_mut(&self.glow) {
            m.base_color = (accent * (1.0 + 0.6 * pulse)).with_alpha(1.0).into();
        }
        if let Some(mut m) = toons.get_mut(&self.screen) {
            // alpha 0: not scaled by the camera's exposure
            m.base.emissive = (accent * (0.03 + 0.2 * pulse)).with_alpha(0.0);
        }
    }

    fn of(&self, zone: Zone) -> Mat {
        match zone {
            Zone::Skin => Mat::Toon(self.skin.clone()),
            Zone::Shirt => Mat::Toon(self.shirt.clone()),
            Zone::Pants => Mat::Toon(self.pants.clone()),
            Zone::Screen => Mat::Toon(self.screen.clone()),
            Zone::Cloth => Mat::Toon(self.cloth.clone()),
            Zone::Glow => Mat::Plain(self.glow.clone()),
        }
    }
}

/// Which of an avatar's materials a part wears (`Kit` has the shared ones).
#[derive(Clone, Copy)]
enum Zone {
    Skin,
    Shirt,
    Pants,
    Screen,
    Cloth,
    Glow,
}

/// Spawns a part under `parent`, with its outline when the shape has one. The glows and outlines cast no shadow.
fn part(c: &mut Commands, kit: &Kit, parent: Entity, shape: &Shape, mat: &Mat, t: Transform) -> Entity {
    let e = c.spawn((Mesh3d(shape.mesh.clone()), t, ChildOf(parent))).id();
    match mat {
        Mat::Toon(h) => c.entity(e).insert(MeshMaterial3d(h.clone())),
        Mat::Plain(h) => c.entity(e).insert((MeshMaterial3d(h.clone()), NotShadowCaster)),
    };
    if let Some(h) = &shape.hull {
        c.spawn((Mesh3d(h.clone()), MeshMaterial3d(kit.outline.clone()), NotShadowCaster, Transform::IDENTITY, ChildOf(e)));
    }
    e
}

/// An empty holder that parts hang from.
fn pivot(c: &mut Commands, parent: Entity, t: Transform) -> Entity {
    c.spawn((t, Visibility::default(), ChildOf(parent))).id()
}

/// How the avatar moves this frame; filled in by its owner.
#[derive(Component, Default, Clone)]
pub struct Drive {
    pub velocity: Vec3,
    pub grounded: Option<bool>,
    pub flying: bool,
    /// the mouth moves while this is set (besides the voice's own loudness)
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
    torso: Entity,
    head: Entity,
    face: Entity,
    /// where each eye sits on the screen (moved as it looks); the eye's shape and highlight hang from it
    eyes: [Entity; 2],
    smile: Entity,
    mouth: Entity,
    legs: [Entity; 2],
    shafts: [Entity; 2],
    boots: [Entity; 2],
    hands: [Entity; 2],
    /// the outer fingers, the index finger and the thumb of each hand
    fingers: [[Entity; 3]; 2],
    /// holders for the head accessory and the outfit
    hat: Entity,
    wear: Entity,
}

/// What the avatar shows now, kept between frames.
struct Built {
    mats: Mats,
    parts: Parts,
    shown: Look,
    first_person: bool,
    /// each eye's shape and highlight
    eye_shapes: [(Entity, Option<Entity>); 2],
    /// the moving part of the head accessory and of the outfit
    wiggle: [Option<Entity>; 2],
    /// where the eyes look (-1..1), the fingers' curl and the lights' voice pulse, eased
    gaze: Vec2,
    curls: [[f32; 3]; 2],
    pulse: f32,
}

#[derive(Component)]
pub struct Rig {
    pub anim: Anim,
    /// change it to repaint the avatar
    pub look: Look,
    /// shows only the hands (the local player's own view)
    pub first_person: bool,
    /// whose voice moves the mouth: a remote player's actor number, or None for the local microphone
    actor: Option<u16>,
    seed: u32,
    built: Option<Built>,
}

/// A remote avatar's name over its head: the UI node that shows it.
#[derive(Component)]
pub struct NameTag(pub Entity);

/// The UI node of a name tag, and the avatar it belongs to.
#[derive(Component)]
struct TagOf(Entity);

/// The light in a name tag that shows the avatar's colour and brightens while it talks.
#[derive(Component)]
struct TagLight;

/// Spawns an avatar at `at` (feet, turned with the body); with a `name`, a remote player's (its name tag over it,
/// its voice found by `seed`, its actor number), else the local player's. The parts are made on its first frame.
pub fn spawn(commands: &mut Commands, _kit: &Kit, _materials: &mut Assets<StandardMaterial>, look: Look, at: Transform, name: Option<&str>, seed: u32) -> Entity {
    let actor = name.map(|_| seed as u16);
    let root = commands.spawn((at, Visibility::default(), Rig { anim: Anim::new(seed), look, first_person: false, actor, seed, built: None },
        Drive::default())).id();
    if let Some(name) = name {
        let tag = commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                column_gap: px(7.0),
                padding: UiRect::new(px(9.0), px(11.0), px(4.0), px(4.0)),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(13.0)),
                ..default()
            },
            UiTransform { translation: Val2::percent(-50.0, -100.0), ..default() },
            BackgroundColor(Color::srgba(0.06, 0.07, 0.1, 0.72)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.14)),
            Visibility::Hidden,
            TagOf(root),
        )).with_children(|t| {
            t.spawn((Node { width: px(8.0), height: px(8.0), border_radius: BorderRadius::all(px(4.0)), ..default() }, BackgroundColor(rgb(look.accent)), TagLight));
            t.spawn((Text::new(name), TextFont::from_font_size(15.0), TextColor(Color::srgb(0.95, 0.96, 0.98))));
        }).id();
        commands.entity(root).insert(NameTag(tag));
    }
    root
}

/// Makes an avatar's parts.
fn build(c: &mut Commands, kit: &Kit, mats: Mats, root: Entity, look: &Look) -> Built {
    use std::f32::consts::FRAC_PI_2;
    let z = |zone| mats.of(zone);
    let joint = Mat::Toon(kit.joint.clone());
    let upper = pivot(c, root, Transform::from_xyz(0.0, HIP, 0.0));
    part(c, kit, upper, &kit.hips, &z(Zone::Pants), Transform::default());
    let torso = pivot(c, upper, Transform::from_xyz(0.0, WAIST, 0.0));
    part(c, kit, torso, &kit.torso, &z(Zone::Shirt), Transform::default());
    // the chest light: a glowing disc in a ring of shell
    let chest = Transform::from_xyz(0.0, 0.33, -0.166).with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.05));
    part(c, kit, torso, &kit.core, &z(Zone::Skin), chest);
    part(c, kit, torso, &kit.ball, &z(Zone::Glow), chest.with_scale(Vec3::new(0.033, 0.008, 0.033)));
    for s in [-1.0, 1.0] {
        part(c, kit, torso, &kit.shoulder, &z(Zone::Skin), Transform::from_xyz(s * 0.198, 0.47, 0.0).with_rotation(Quat::from_rotation_z(s * 0.25)));
    }
    part(c, kit, upper, &kit.neck, &joint, Transform::from_xyz(0.0, NECK - HIP - 0.01, 0.0));
    let wear = pivot(c, upper, Transform::default());

    let head = pivot(c, upper, Transform::from_xyz(0.0, NECK - HIP, 0.0));
    let face = pivot(c, head, Transform::from_xyz(0.0, HEAD, 0.0));
    part(c, kit, face, &kit.shell, &z(Zone::Skin), Transform::default());
    part(c, kit, face, &kit.bezel, &joint, Transform::default());
    part(c, kit, face, &kit.screen, &z(Zone::Screen), Transform::default());
    for s in [-1.0f32, 1.0] {
        let side = Quat::from_rotation_z(-s * FRAC_PI_2);
        part(c, kit, face, &kit.ear, &z(Zone::Shirt), Transform::from_xyz(s * 0.2, -0.01, 0.01));
        part(c, kit, face, &kit.ring, &z(Zone::Glow), Transform::from_xyz(s * 0.226, -0.01, 0.01).with_rotation(side));
    }
    let eyes = [0, 1].map(|_| pivot(c, face, Transform::default()));
    let (at, n) = mesh::on_head(0.0, face::MOUTH_LAT, 0.009);
    let mouth_at = pivot(c, face, Transform::from_translation(at).looking_to(n, Vec3::Y));
    let smile = part(c, kit, mouth_at, &kit.smile, &z(Zone::Glow), Transform::default());
    let mouth = part(c, kit, mouth_at, &kit.ball, &z(Zone::Glow), Transform::from_scale(Vec3::ZERO));
    let hat = pivot(c, face, Transform::default());

    let mut shafts = [Entity::PLACEHOLDER; 2];
    let mut boots = [Entity::PLACEHOLDER; 2];
    let legs = [0, 1].map(|i| {
        let s = if i == 0 { -1.0 } else { 1.0 };
        let leg = pivot(c, root, Transform::from_xyz(s * STANCE, HIP, 0.0));
        shafts[i] = pivot(c, leg, Transform::default());
        part(c, kit, shafts[i], &kit.leg, &z(Zone::Pants), Transform::default());
        boots[i] = pivot(c, leg, Transform::from_xyz(0.0, -HIP + BOOT, 0.0));
        part(c, kit, boots[i], &kit.boot, &z(Zone::Skin), Transform::from_xyz(0.0, 0.006, -0.035));
        part(c, kit, boots[i], &kit.sole, &joint, Transform::from_xyz(0.0, -BOOT + 0.012, -0.035));
        leg
    });

    let mut fingers = [[Entity::PLACEHOLDER; 3]; 2];
    let hands = [0, 1].map(|i| {
        let s = if i == 0 { -1.0 } else { 1.0 };
        let wrist = pivot(c, root, Transform::default());
        let hand = pivot(c, wrist, Transform::from_scale(Vec3::splat(HAND)));
        part(c, kit, hand, &kit.palm, &z(Zone::Skin), Transform::default());
        // the outer three fingers as one mitten, the index finger beside it on the thumb's side, the thumb inside
        let mitten = pivot(c, hand, Transform::from_xyz(s * 0.01, 0.002, -0.032));
        part(c, kit, mitten, &kit.mitten, &z(Zone::Skin), Transform::from_xyz(0.0, 0.0, -0.03));
        let index = pivot(c, hand, Transform::from_xyz(-s * 0.029, 0.005, -0.034));
        part(c, kit, index, &kit.finger, &z(Zone::Skin), Transform::from_xyz(0.0, 0.0, -0.028));
        let thumb = pivot(c, hand, Transform::from_xyz(-s * 0.042, -0.006, -0.004));
        part(c, kit, thumb, &kit.thumb, &z(Zone::Skin), Transform::from_xyz(0.0, 0.0, -0.024));
        part(c, kit, hand, &kit.cuff, &z(Zone::Glow), Transform::from_xyz(0.0, 0.0, 0.046).with_rotation(Quat::from_rotation_x(FRAC_PI_2))
            .with_scale(Vec3::new(1.0, 0.5, 0.62)));
        fingers[i] = [mitten, index, thumb];
        wrist
    });

    let parts = Parts { upper, torso, head, face, eyes, smile, mouth, legs, shafts, boots, hands, fingers, hat, wear };
    let eye_shapes = dress_eyes(c, kit, &mats, &parts, look.eyes);
    let wiggle = [gear::head(c, kit, &mats, hat, look.accessory), gear::body(c, kit, &mats, wear, look.outfit)];
    Built { mats, parts, shown: *look, first_person: false, eye_shapes, wiggle, gaze: Vec2::ZERO, curls: [anim::Grip::Open.curls(); 2], pulse: 0.0 }
}

/// Half sizes of the eye shapes (the arc of the happy eye is its own size).
fn eye_size(e: Eyes) -> Vec3 {
    match e {
        Eyes::Pill => Vec3::new(0.025, 0.037, 0.007),
        Eyes::Round => Vec3::new(0.031, 0.031, 0.007),
        Eyes::Wide => Vec3::new(0.039, 0.021, 0.007),
        Eyes::Happy => Vec3::ONE,
    }
}

/// Puts the eyes of style `e` on the eye holders: the light, and a white highlight up on the inner side.
fn dress_eyes(c: &mut Commands, kit: &Kit, mats: &Mats, p: &Parts, e: Eyes) -> [(Entity, Option<Entity>); 2] {
    [0, 1].map(|i| {
        c.entity(p.eyes[i]).despawn_related::<Children>();
        let shape = part(c, kit, p.eyes[i], if e == Eyes::Happy { &kit.happy } else { &kit.ball }, &mats.of(Zone::Glow), Transform::from_scale(eye_size(e)));
        let size = eye_size(e);
        let shine = (e != Eyes::Happy).then(|| {
            let s = if i == 0 { 1.0 } else { -1.0 };
            part(c, kit, p.eyes[i], &kit.ball, &Mat::Plain(kit.white.clone()),
                Transform::from_xyz(s * size.x * 0.38, size.y * 0.42, -0.0045).with_scale(Vec3::splat(0.0068)))
        });
        (shape, shine)
    })
}

/// Makes new avatars' parts, and repaints or re-dresses them when their look or view changed.
fn dress(
    mut commands: Commands,
    kit: Option<Res<Kit>>,
    roomie: Option<Res<real::Roomie>>,
    mut toons: ResMut<Assets<Toon>>,
    mut plain: ResMut<Assets<StandardMaterial>>,
    mut rigs: Query<(Entity, &mut Rig)>,
    mut visibility: Query<&mut Visibility>,
) {
    // the real Rec Room body (real::attach_real) stands in for the procedural parts when the game files are present.
    if roomie.is_some() {
        return;
    }
    let Some(kit) = kit else { return };
    for (root, mut rig) in &mut rigs {
        let rig = &mut *rig;
        let b = match &mut rig.built {
            Some(b) => b,
            None => {
                let mats = Mats::new(&mut toons, &mut plain);
                mats.paint(&mut toons, &mut plain, &rig.look, 0.0);
                rig.built.insert(build(&mut commands, &kit, mats, root, &rig.look))
            }
        };
        let look = rig.look;
        if b.shown != look {
            b.mats.paint(&mut toons, &mut plain, &look, b.pulse);
            if b.shown.eyes != look.eyes {
                b.eye_shapes = dress_eyes(&mut commands, &kit, &b.mats, &b.parts, look.eyes);
            }
            if b.shown.accessory != look.accessory {
                commands.entity(b.parts.hat).despawn_related::<Children>();
                b.wiggle[0] = gear::head(&mut commands, &kit, &b.mats, b.parts.hat, look.accessory);
            }
            if b.shown.outfit != look.outfit {
                commands.entity(b.parts.wear).despawn_related::<Children>();
                b.wiggle[1] = gear::body(&mut commands, &kit, &b.mats, b.parts.wear, look.outfit);
            }
            b.shown = look;
        }
        if b.first_person != rig.first_person {
            let hide = if rig.first_person { Visibility::Hidden } else { Visibility::Inherited };
            for e in [b.parts.upper, b.parts.legs[0], b.parts.legs[1]] {
                if let Ok(mut v) = visibility.get_mut(e) {
                    *v = hide;
                }
            }
            b.first_person = rig.first_person;
        }
    }
}

/// Eases `from` toward `to` at `rate` per second.
fn ease(from: f32, to: f32, rate: f32, dt: f32) -> f32 {
    from + (to - from) * (1.0 - (-rate * dt).exp())
}

/// Poses every avatar from its `Drive`, its voice and what it sees.
#[allow(clippy::too_many_arguments)]
fn animate(
    time: Res<Time>,
    roomie: Option<Res<real::Roomie>>,
    loud: Option<Res<crate::voice::Loudness>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut toons: ResMut<Assets<Toon>>,
    mut plain: ResMut<Assets<StandardMaterial>>,
    mut rigs: Query<(Entity, &Transform, &mut Rig, &Drive)>,
    mut parts: Query<&mut Transform, Without<Rig>>,
    globals: Query<&GlobalTransform>,
    mut visibility: Query<&mut Visibility>,
) {
    // the real body is static for now (real::attach_real); skip the procedural posing when it is in use.
    if roomie.is_some() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    let voice_of = |rig: &Rig| loud.as_ref().map_or(0.0, |l| rig.actor.map_or(l.me, |a| l.actors.get(&a).copied().unwrap_or(0.0)));
    // who is talking, and where their heads are (last frame's), for the others' eyes to find
    let talkers: Vec<(Entity, Vec3)> = rigs.iter().filter(|(_, _, r, d)| d.talking || voice_of(r) > 0.12)
        .filter_map(|(e, _, r, _)| Some((e, globals.get(r.built.as_ref()?.parts.face).ok()?.translation()))).collect();
    let eye = camera.iter().next().map(|g| g.translation());
    for (me, root, mut rig, drive) in &mut rigs {
        let voice = voice_of(&rig);
        let rig = &mut *rig;
        let Some(b) = &mut rig.built else { continue };
        // how far down the head is tells how far the player crouches (also for remote players)
        let crouch = drive.head_height.map_or(0.0, |h| ((STAND_EYE - h) / (STAND_EYE - CROUCH_EYE)).clamp(0.0, 1.0));
        rig.anim.step(dt, Input { velocity: drive.velocity, grounded: drive.grounded, flying: drive.flying, talking: drive.talking, voice, crouch });
        let pose = rig.anim.pose();
        let p = &b.parts;
        let lean = Quat::from_rotation_x(-pose.lean);
        let hips = HIP + pose.bob - pose.drop;
        if let Ok(mut tr) = parts.get_mut(p.upper) {
            *tr = Transform::from_xyz(0.0, hips, 0.0).with_rotation(lean * Quat::from_rotation_z(pose.sway));
        }
        if let Ok(mut tr) = parts.get_mut(p.torso) {
            // breathing: the chest rises and fills out a little
            tr.scale = Vec3::new(1.0 + 0.012 * pose.breath, 1.0 + 0.016 * pose.breath, 1.0 + 0.022 * pose.breath);
        }
        // the legs hang from the hips and stretch to the ground; the boots stay flatter than the legs swing
        for i in 0..2 {
            let s = if i == 0 { -1.0 } else { 1.0 };
            if let Ok(mut tr) = parts.get_mut(p.legs[i]) {
                *tr = Transform::from_xyz(s * STANCE, hips, 0.0).with_rotation(Quat::from_rotation_x(pose.legs[i]));
            }
            if let Ok(mut tr) = parts.get_mut(p.shafts[i]) {
                tr.scale = Vec3::new(1.0, hips / HIP, 1.0);
            }
            if let Ok(mut tr) = parts.get_mut(p.boots[i]) {
                *tr = Transform::from_xyz(0.0, -hips + BOOT, 0.0).with_rotation(Quat::from_rotation_x(-0.7 * pose.legs[i]));
            }
        }
        // hand shapes: the fingers curl to the grip (a fist round something held, the index finger out to point)
        for i in 0..2 {
            let s = if i == 0 { -1.0 } else { 1.0 };
            let want = drive.grip[i].curls();
            for (k, c) in b.curls[i].iter_mut().enumerate() {
                *c = ease(*c, want[k], 18.0, dt);
            }
            let [mitten, index, thumb] = b.curls[i];
            let f = p.fingers[i];
            for (e, r) in [(f[0], Quat::from_rotation_x(-mitten)), (f[1], Quat::from_rotation_x(-index)),
                (f[2], Quat::from_rotation_y(s * (0.55 - 0.35 * thumb)) * Quat::from_rotation_x(-0.7 * thumb))] {
                if let Ok(mut tr) = parts.get_mut(e) {
                    tr.rotation = r;
                }
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

        // the eyes: toward someone talking nearby, else the camera, else wandering
        let head = globals.get(p.face).ok().map(|g| g.affine().inverse());
        let seen = |at: Vec3| head.and_then(|h| face::gaze(h.transform_point3(at)));
        let near = |at: &Vec3, range: f32| globals.get(p.face).is_ok_and(|g| g.translation().distance(*at) < range);
        let target = talkers.iter().filter(|(e, at)| *e != me && near(at, HEAR)).find_map(|(_, at)| seen(*at))
            .or_else(|| eye.filter(|at| near(at, SEE)).and_then(seen))
            .unwrap_or_else(|| face::glance(rig.seed, t));
        b.gaze = Vec2::new(ease(b.gaze.x, target.x, 14.0, dt), ease(b.gaze.y, target.y, 14.0, dt));
        let size = 1.0 + 0.1 * pose.wide;
        for i in 0..2 {
            let (lon, lat) = face::eye_at(i, b.gaze);
            let (at, n) = mesh::on_head(lon, lat, 0.0085);
            if let Ok(mut tr) = parts.get_mut(p.eyes[i]) {
                *tr = Transform::from_translation(at).looking_to(n, Vec3::Y).with_scale(Vec3::splat(size));
            }
            let open = Vec3::new(1.0, pose.eyes.max(0.08) * (1.0 + 0.15 * pose.wide), 1.0);
            let (shape, shine) = b.eye_shapes[i];
            if let Ok(mut tr) = parts.get_mut(shape) {
                tr.scale = eye_size(b.shown.eyes) * open;
            }
            if let Some(Ok(mut tr)) = shine.map(|e| parts.get_mut(e)) {
                tr.scale = Vec3::splat(0.0068) * open;
            }
        }
        // the mouth: a small smile, opening into an oval as it talks
        let m = pose.mouth;
        if let Ok(mut v) = visibility.get_mut(p.smile) {
            v.set_if_neq(if m < 0.08 { Visibility::Inherited } else { Visibility::Hidden });
        }
        if let Ok(mut tr) = parts.get_mut(p.mouth) {
            tr.scale = if m < 0.08 { Vec3::ZERO } else { Vec3::new(0.034 - 0.008 * m, 0.006 + 0.026 * m, 0.006) };
        }
        if (m - b.pulse).abs() > 0.03 || (m == 0.0 && b.pulse != 0.0) {
            b.pulse = if m < 0.03 { 0.0 } else { m };
            b.mats.pulse(&mut toons, &mut plain, &b.shown, b.pulse);
        }

        let to_local = root.compute_affine().inverse();
        for (i, e) in p.hands.into_iter().enumerate() {
            let Ok(mut tr) = parts.get_mut(e) else { continue };
            *tr = match drive.hands[i] {
                Some(w) => Transform::from_matrix(Mat4::from(to_local) * w.to_matrix()),
                None => Transform::from_translation(pose.hands[i]).with_rotation(Quat::from_rotation_x(-0.2)),
            };
        }
        gear::wiggle(&mut parts, b.wiggle, b.shown, &rig.anim, &pose, t);
    }
}

/// Keeps each name tag over its avatar's head, readable at any distance up to `TAG_RANGE`, its light in the
/// avatar's colour and brighter while it talks; drops tags whose avatar is gone.
#[allow(clippy::type_complexity)]
fn place_tags(
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    owners: Query<(&GlobalTransform, &Rig), With<NameTag>>,
    loud: Option<Res<crate::voice::Loudness>>,
    mut tags: Query<(Entity, &TagOf, &mut Node, &mut Visibility, &Children), Without<TagLight>>,
    mut lights: Query<(&mut BackgroundColor, &mut Node), With<TagLight>>,
) {
    let Ok((camera, eye)) = camera.single() else { return };
    for (e, of, mut node, mut vis, children) in &mut tags {
        let Ok((at, rig)) = owners.get(of.0) else {
            commands.entity(e).despawn();
            continue;
        };
        let top = at.transform_point(Vec3::Y * (TOP + 0.14));
        let near = top.distance(eye.translation()) < TAG_RANGE && (top - eye.translation()).dot(*eye.forward()) > 0.0;
        match camera.world_to_viewport(eye, top).ok().filter(|_| near) {
            Some(p) => {
                node.left = px(p.x);
                node.top = px(p.y);
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
                continue;
            }
        }
        let voice = loud.as_ref().and_then(|l| l.actors.get(&rig.actor?).copied()).unwrap_or(0.0);
        if let Some(Ok((mut colour, mut dot))) = children.first().map(|&c| lights.get_mut(c)) {
            let c = rgb(rig.look.accent).to_linear() * (0.75 + 0.5 * voice.min(1.0));
            colour.set_if_neq(BackgroundColor(c.with_alpha(1.0).into()));
            let size = px(8.0 + 4.0 * voice.min(1.0));
            if dot.width != size {
                dot.width = size;
                dot.height = size;
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

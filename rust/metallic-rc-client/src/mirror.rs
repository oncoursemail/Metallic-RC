//! Mirrors: the game's `AG/Mirror` draws `_BaseMap (smudge) * reflection(screen uv)`, the reflection being a second
//! camera's picture of the room mirrored in the glass (rendering.md, VERIFIED). Here: one mirror camera for the
//! nearest mirror in front of the player, with an oblique near plane on the glass (as Bevy's mirror example does),
//! rendering into a window-sized image that the mirror's `AgSurface` samples at screen coordinates.
use bevy::{
    camera::{Exposure, Hdr, RenderTarget},
    core_pipeline::tonemapping::Tonemapping,
    light::{GeneratedEnvironmentMapLight, Skybox},
    math::reflection_matrix,
    prelude::*,
    render::render_resource::TextureFormat,
    window::PrimaryWindow,
};

use crate::render::AgMaterial;

/// A mirror's glass: the mesh's own front normal and centre (mesh space).
#[derive(Component, Clone, Copy)]
pub struct Mirror {
    pub normal: Vec3,
    pub center: Vec3,
    /// the farthest vertex from the centre
    pub radius: f32,
}

#[derive(Component)]
pub struct MirrorCamera;

/// The mirror camera's picture.
#[derive(Resource, Default)]
struct Reflection(Option<Handle<Image>>);

/// The mirror camera's near plane sits this far in front of the glass, so the glass itself (sampling its own
/// picture) and the frame's lip are cut away; on the plane exactly, oblique views kept them.
const GLASS_CLEARANCE: f32 = 0.03;

/// Past this distance the mirror camera stops rendering.
const REACH: f32 = 15.0;

pub struct MirrorPlugin;

impl Plugin for MirrorPlugin {
    fn build(&self, app: &mut App) {
        // before the cascades are built: a mirror camera switched on after them is a view without cascades, and
        // Bevy's prepare_lights panics on that (seen as an intermittent crash when the mirror first comes on screen)
        app.init_resource::<Reflection>().add_systems(PostUpdate, follow.after(TransformSystems::Propagate)
            .before(bevy::light::SimulationLightSystems::UpdateDirectionalLightCascades));
    }
}

#[allow(clippy::too_many_arguments)]
fn follow(
    mut commands: Commands,
    window: Single<&Window, With<PrimaryWindow>>,
    main: Query<(&GlobalTransform, &Projection, Option<&Skybox>, Option<&GeneratedEnvironmentMapLight>, Option<&Exposure>), (With<Camera3d>, Without<MirrorCamera>)>,
    mut cams: Query<(Entity, &mut Camera, &mut Transform, &mut Projection, &RenderTarget, &mut Exposure), With<MirrorCamera>>,
    mirrors: Query<(&Mirror, &GlobalTransform, &MeshMaterial3d<AgMaterial>)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<AgMaterial>>,
    mut reflection: ResMut<Reflection>,
) {
    let Some((eye, Projection::Perspective(lens), sky, env, exposure)) = main.iter().next() else { return };
    // the nearest mirror on screen; its glass reflects toward whichever side the eye is on (the Dorm mirror's mesh
    // normals face the wall)
    let aspect = window.width() / window.height().max(1.0);
    let half_view = ((lens.fov / 2.0).tan() * (1.0 + aspect * aspect).sqrt()).atan();
    let best = mirrors.iter().filter_map(|(m, g, mat)| {
        let mut n = Vec3::from((g.affine().matrix3 * Vec3A::from(m.normal)).normalize_or_zero());
        let c = g.transform_point(m.center);
        let mut d = n.dot(eye.translation() - c);
        if d < 0.0 {
            (n, d) = (-n, -d);
        }
        let to = c - eye.translation();
        let seen = to.angle_between(*eye.forward()) < half_view + (m.radius / to.length().max(m.radius)).asin();
        (d > 0.05 && d < REACH && seen).then_some((d, n, c, mat))
    }).min_by(|a, b| a.0.total_cmp(&b.0));

    // half the window's size: the picture is seen through the smudge map, and this halves the mirror's cost
    let size = UVec2::new((window.physical_width() / 2).max(1), (window.physical_height() / 2).max(1));
    let image = match &reflection.0 {
        Some(h) if images.get(h).is_some_and(|i| i.size() == size) => h.clone(),
        _ => {
            let h = images.add(Image::new_target_texture(size.x, size.y, TextureFormat::Bgra8UnormSrgb, None));
            reflection.0 = Some(h.clone());
            h
        }
    };
    let Some((_, n, c, mat)) = best else {
        for (_, mut cam, ..) in &mut cams {
            cam.is_active = false;
        }
        return;
    };
    if let Some(mut m) = materials.get_mut(&mat.0) {
        if m.extension.emission_map.as_ref() != Some(&image) {
            m.extension.emission_map = Some(image.clone());
        }
    }

    // the eye mirrored in the glass's plane, with the glass as the near plane (Bevy's mirror example)
    let plane = Mat4::from_translation(c) * Mat4::from_mat3a(reflection_matrix(n)) * Mat4::from_translation(-c);
    let place = Transform::from_matrix(plane * eye.to_matrix());
    let view_from_world = eye.affine().matrix3.inverse();
    let near_normal = (view_from_world * Vec3A::from(-n)).normalize();
    let projection = Projection::Perspective(PerspectiveProjection {
        near_clip_plane: Vec3::from(near_normal).extend(n.dot(c - eye.translation()) - GLASS_CLEARANCE),
        ..lens.clone()
    });

    match cams.iter_mut().next() {
        Some((e, mut cam, mut t, mut p, target, mut ev)) => {
            cam.is_active = true;
            if let Some(x) = exposure {
                *ev = *x;
            }
            *t = place;
            *p = projection;
            if target.as_image() != Some(&image) {
                commands.entity(e).insert(RenderTarget::Image(image.into()));
            }
        }
        None => {
            let mut e = commands.spawn((
                Camera3d::default(),
                // the reflection flips every triangle's winding
                Camera { order: -1, invert_culling: true, ..default() },
                RenderTarget::Image(image.into()),
                place,
                projection,
                MirrorCamera,
                // the same tone curve as the main view (render::LookPlugin skips this camera)
                (Hdr, Tonemapping::KhronosPbrNeutral, Exposure { ev100: 9.7 - 0.21 }),
            ));
            if let (Some(sky), Some(env)) = (sky, env) {
                e.insert((sky.clone(), env.clone()));
            }
        }
    }
}

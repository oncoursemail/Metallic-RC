//! The avatar's own material: the standard lit material plus a soft fill (the shaded side never goes black) and a
//! rim light round the silhouette, so the robot reads as a soft toy against bright or busy rooms. The shader is
//! `toon.wesl`, embedded in the binary.
use bevy::{
    material::OpaqueRendererMethod,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub type Toon = ExtendedMaterial<StandardMaterial, Soft>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Soft {
    /// the rim light's colour (rgb) and strength (a)
    #[uniform(100)]
    pub rim: LinearRgba,
    /// x: how tight the rim is to the edge (a power), y: how much of the base colour is added unlit (the fill)
    #[uniform(100)]
    pub shade: Vec4,
}

impl MaterialExtension for Soft {
    fn fragment_shader() -> ShaderRef {
        "embedded://metallic_rc_client/avatar/toon.wesl".into()
    }
}

/// A soft material: `rim` 0..1 strength of the rim light, `fill` share of the colour that does not need light.
pub fn toon(base: StandardMaterial, rim: f32, fill: f32) -> Toon {
    ExtendedMaterial {
        // the rim and fill are added after lighting, which only the forward pass does
        base: StandardMaterial { opaque_render_method: OpaqueRendererMethod::Forward, ..base },
        extension: Soft { rim: LinearRgba::new(1.0, 0.96, 0.9, rim), shade: Vec4::new(3.0, fill, 0.0, 0.0) },
    }
}

// The avatar's soft material (toon.rs): standard lighting, then a rim light round the silhouette and a little of the
// base colour added unlit so the shaded side stays soft. Forward pass only.
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
};

struct Soft {
    rim: vec4<f32>,
    shade: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> soft: Soft;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );
    var lit = apply_pbr_lighting(pbr_input);
    // rim and fill follow how lit the scene is here, so a dark room stays dark
    let light = saturate(dot(lit.rgb, vec3(0.2126, 0.7152, 0.0722)) * 3.0 + 0.12);
    let facing = saturate(dot(pbr_input.N, pbr_input.V));
    let rim = pow(1.0 - facing, soft.shade.x) * soft.rim.a * light;
    lit = vec4(lit.rgb + soft.rim.rgb * rim + pbr_input.material.base_color.rgb * soft.shade.y * light, lit.a);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, lit);
    return out;
}

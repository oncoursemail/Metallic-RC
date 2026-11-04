// Rec Room's AG surface shaders (AG/Standard*, AG/MakerPenShape, AG/Unlit*, URP Lit) as a StandardMaterial
// extension: the albedo, emission and specular inputs follow the game's compiled programs (rendering.md, look lane
// notes); Bevy's PBR does the lighting, shadows, ambient, lightmaps and fog.
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
};

struct AgParams {
    // the family tint (linear rgb, opacity)
    tint: vec4<f32>,
    // _BaseMap_ST: uv * xy + zw
    uv_st: vec4<f32>,
    // rgb _SpecColor (linear), a _Smoothness
    spec: vec4<f32>,
    // rgb _EmissionColor (linear), a _EmissivePower
    emission: vec4<f32>,
    flags: u32,
}

const AG_MASKED: u32 = 1u;
const AG_BASE_MAP: u32 = 2u;
const AG_SPEC_MAP: u32 = 4u;
const AG_EMISSION_MAP: u32 = 8u;
const AG_URP_LIT: u32 = 16u;
const AG_ALPHA_FROM_MAP: u32 = 32u;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> ag: AgParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var ag_base: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var ag_base_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var ag_spec: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var ag_spec_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var ag_emission: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var ag_emission_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    var uv = vec2<f32>(0.0);
@if(VERTEX_UVS_A)
    uv = in.uv * ag.uv_st.xy + ag.uv_st.zw;
    var vc = vec4<f32>(1.0);
@if(VERTEX_COLORS)
    vc = in.color;

    let tint = ag.tint.rgb * vc.rgb;
    let masked = (ag.flags & AG_MASKED) != 0u;
    var b = vec4<f32>(1.0);
    if (ag.flags & AG_BASE_MAP) != 0u {
        b = textureSample(ag_base, ag_base_sampler, uv);
    }
    var sg = vec4<f32>(0.0);
    if (ag.flags & AG_SPEC_MAP) != 0u {
        sg = textureSample(ag_spec, ag_spec_sampler, uv);
    }
    var em = vec3<f32>(1.0);
    if (ag.flags & AG_EMISSION_MAP) != 0u {
        em = textureSample(ag_emission, ag_emission_sampler, uv).rgb;
    }

    // albedo: the AG lit shaders mask the tint with the base map's alpha; URP Lit and the unlit ones multiply
    var albedo = b.rgb * tint;
    var emission = ag.emission.rgb * em;
    if masked {
        albedo = b.rgb * mix(vec3<f32>(1.0), tint, b.a);
        emission += b.rgb * b.a * ag.emission.a * tint;
    }
    var alpha = ag.tint.a * vc.a;
    if !masked || (ag.flags & AG_ALPHA_FROM_MAP) != 0u {
        alpha *= b.a;
    }

    // specular workflow: F0 = map rgb * _SpecColor, gloss = map a * _Smoothness; no map = black (no gloss);
    // URP Lit without a map: dielectric 0.04 and gloss = _Smoothness
    var spec = sg.rgb * ag.spec.rgb;
    var gloss = sg.a * ag.spec.a;
    if (ag.flags & (AG_SPEC_MAP | AG_URP_LIT)) == AG_URP_LIT {
        spec = vec3<f32>(0.04);
        gloss = ag.spec.a;
    }
    // Unity's diffuse keeps 1 - max(F0) of the albedo
    let keep = 1.0 - max(spec.r, max(spec.g, spec.b));
    pbr_input.material.base_color = vec4<f32>(albedo * keep, alpha);
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = sqrt(spec / 0.16);
    pbr_input.material.perceptual_roughness = clamp(1.0 - gloss, 0.089, 1.0);
    // emissive alpha 0: added as is, like URP adds emission to the frame
    pbr_input.material.emissive = vec4<f32>(emission, 0.0);

    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = vec4<f32>(albedo, alpha);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

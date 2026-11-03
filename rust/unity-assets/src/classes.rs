//! Built-in Unity class IDs (used when a file has no type tree for a name).

pub const GAME_OBJECT: i32 = 1;
pub const TRANSFORM: i32 = 4;
pub const MATERIAL: i32 = 21;
pub const MESH_RENDERER: i32 = 23;
pub const TEXTURE_2D: i32 = 28;
pub const MESH_FILTER: i32 = 33;
pub const MESH: i32 = 43;
pub const SHADER: i32 = 48;
pub const MONO_BEHAVIOUR: i32 = 114;
pub const SKINNED_MESH_RENDERER: i32 = 137;
pub const ASSET_BUNDLE: i32 = 142;
pub const RECT_TRANSFORM: i32 = 224;

pub fn class_name(id: i32) -> Option<&'static str> {
    Some(match id {
        1 => "GameObject",
        2 => "Component",
        4 => "Transform",
        8 => "Behaviour",
        20 => "Camera",
        21 => "Material",
        23 => "MeshRenderer",
        25 => "Renderer",
        28 => "Texture2D",
        29 => "OcclusionCullingSettings",
        30 => "GraphicsSettings",
        33 => "MeshFilter",
        43 => "Mesh",
        48 => "Shader",
        49 => "TextAsset",
        54 => "Rigidbody",
        64 => "MeshCollider",
        65 => "BoxCollider",
        74 => "AnimationClip",
        82 => "AudioSource",
        83 => "AudioClip",
        84 => "RenderTexture",
        89 => "Cubemap",
        90 => "Avatar",
        91 => "AnimatorController",
        95 => "Animator",
        104 => "RenderSettings",
        108 => "Light",
        111 => "Animation",
        114 => "MonoBehaviour",
        115 => "MonoScript",
        117 => "Texture3D",
        128 => "Font",
        135 => "SphereCollider",
        136 => "CapsuleCollider",
        137 => "SkinnedMeshRenderer",
        142 => "AssetBundle",
        150 => "PreloadData",
        157 => "LightmapSettings",
        187 => "Texture2DArray",
        196 => "NavMeshSettings",
        198 => "ParticleSystem",
        199 => "ParticleSystemRenderer",
        205 => "LODGroup",
        212 => "SpriteRenderer",
        213 => "Sprite",
        221 => "AnimatorOverrideController",
        222 => "CanvasRenderer",
        223 => "Canvas",
        224 => "RectTransform",
        225 => "CanvasGroup",
        258 => "LightProbes",
        687078895 => "SpriteAtlas",
        1001 => "PrefabInstance",
        1660057539 => "SceneRoots",
        _ => return None,
    })
}

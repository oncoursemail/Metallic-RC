//! The OpenXR seam (feature `xr`). Not linked yet: the only maintained Bevy OpenXR stack, `bevy_mod_openxr`
//! (bevy_oxr), supports Bevy 0.20 only in an unmerged branch (PR #212, `bevy_0.20`), and building it pulls the OpenXR
//! loader. When it is added (see docs/agents/metallic-rc-engine-20261104.md, "Android and Quest"):
//! - Cargo.toml: `bevy_mod_openxr = { git = "https://github.com/awtterpip/bevy_oxr", rev = "<PR #212 head>", optional = true }`
//!   and `xr = ["dep:bevy_mod_openxr"]`;
//! - here: `add_xr_plugins(DefaultPlugins)` replaces `DefaultPlugins` in main.rs (a few lines), the player camera
//!   becomes the XR camera rig, hands drive `avatar::Drive::hands`;
//! - Quest: `quality::tier_cap` gives Low (to be measured), MSAA 4x and no post passes, `NoIndirectDrawing`
//!   (`platform::PlatformPlugin`), fixed foveation through `XR_FB_foveation`.
use bevy::prelude::*;

pub struct XrPlugin;

impl Plugin for XrPlugin {
    fn build(&self, _app: &mut App) {
        warn!("built with the xr feature, but no OpenXR runtime is linked yet (platform/xr.rs)");
    }
}

//! What differs per platform: which one this build is, touch controls (phones; also on a PC with a touch screen),
//! mobile render defaults, and the seam for OpenXR (feature `xr`, standalone Quest and PC VR).
//! Everything here compiles on every target; the parts that only make sense on one are switched on at run time or by
//! `cfg`, so a Windows build checks the Android paths too.
use bevy::prelude::*;

pub mod touch;
pub mod xr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    Android,
    Other,
}

impl Platform {
    pub const fn current() -> Platform {
        if cfg!(target_os = "android") {
            Platform::Android
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            Platform::Other
        }
    }

    pub const fn mobile(self) -> bool {
        matches!(self, Platform::Android)
    }
}

/// Platform defaults: touch controls on phones (or when asked), and on mobile GPUs the CPU-side batching path (the
/// Quest's Adreno needs `NoIndirectDrawing`: bevy_oxr's Bevy 0.20 port reports indirect drawing still broken there).
pub struct PlatformPlugin {
    pub touch: bool,
}

impl Default for PlatformPlugin {
    fn default() -> Self {
        PlatformPlugin { touch: Platform::current().mobile() }
    }
}

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        if self.touch {
            app.add_plugins(touch::TouchControlsPlugin);
        }
        if Platform::current().mobile() {
            app.add_systems(Update, mobile_cameras);
        }
        #[cfg(feature = "xr")]
        app.add_plugins(xr::XrPlugin);
    }
}

fn mobile_cameras(mut commands: Commands, new: Query<Entity, Added<Camera3d>>) {
    for cam in &new {
        commands.entity(cam).insert(bevy::render::view::NoIndirectDrawing);
    }
}

//! Local interaction in screen mode: a reticle, grabbing the prop under it, holding, throwing and dropping it, and
//! simple rigid-body physics for props that are let go.
use bevy::prelude::*;

use crate::avatar::anim::Grip;

/// Where the interaction wants the local player's hands (world poses; `None` leaves them to the screen pose) and
/// their shapes.
#[derive(Resource, Default)]
pub struct LocalHands {
    pub place: [Option<Transform>; 2],
    pub grip: [Grip; 2],
}

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, _app: &mut App) {}
}

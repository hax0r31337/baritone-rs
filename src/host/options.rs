//! The client options ported code reads (`Minecraft.options`).

use serde::{Deserialize, Serialize};

/// `net.minecraft.client.Options`, as far as ported code reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// `sensitivity().get()`: mouse sensitivity, 0 to 1. `LookBehavior` turns rotations into
    /// whole mouse steps of this sensitivity.
    pub sensitivity: f64,
    /// `autoJump().get()`. Baritone turns it off while it moves the player, and back on after.
    pub auto_jump: bool,
}

impl Default for Options {
    /// The vanilla defaults.
    fn default() -> Self {
        Self {
            sensitivity: 0.5,
            auto_jump: false,
        }
    }
}

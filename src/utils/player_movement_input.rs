// Ported from baritone src/main/java/baritone/utils/PlayerMovementInput.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream subclasses the client's `ClientInput`, which `LocalPlayer.aiStep` ticks. The host
// ticks this one when the player's `baritone_input` is set, at the point where the client
// would tick its input, and moves the player with the result.

use crate::api::utils::input::Input;
use crate::utils::InputOverrideHandler;

/// `net.minecraft.world.entity.player.Input`: the keys held this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyPresses {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
}

/// `ClientInput`'s state after `tick()`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerMovementInput {
    /// `moveVector.x`: positive is left.
    pub left_impulse: f32,
    /// `moveVector.y`: positive is forward.
    pub forward_impulse: f32,
    pub key_presses: KeyPresses,
}

impl PlayerMovementInput {
    pub fn tick(&mut self, handler: &InputOverrideHandler) {
        let mut left_impulse = 0.0f32;
        let mut forward_impulse = 0.0f32;
        let jumping = handler.is_input_forced_down(Input::Jump); // oppa gangnam style

        let up = handler.is_input_forced_down(Input::MoveForward);
        if up {
            forward_impulse += 1.0;
        }

        let down = handler.is_input_forced_down(Input::MoveBack);
        if down {
            forward_impulse -= 1.0;
        }

        let left = handler.is_input_forced_down(Input::MoveLeft);
        if left {
            left_impulse += 1.0;
        }

        let right = handler.is_input_forced_down(Input::MoveRight);
        if right {
            left_impulse -= 1.0;
        }

        let sneaking = handler.is_input_forced_down(Input::Sneak);
        if sneaking {
            left_impulse = (left_impulse as f64 * 0.3) as f32;
            forward_impulse = (forward_impulse as f64 * 0.3) as f32;
        }
        self.left_impulse = left_impulse;
        self.forward_impulse = forward_impulse;

        let sprinting = handler.is_input_forced_down(Input::Sprint);

        self.key_presses = KeyPresses {
            forward: up,
            backward: down,
            left,
            right,
            jump: jumping,
            shift: sneaking,
            sprint: sprinting,
        };
    }
}

// Ported from baritone src/main/java/baritone/pathing/movement/MovementOption.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::utils::input::Input;
use crate::pathing::movement::MovementState;

const SPRINT_MULTIPLIER: f32 = 1.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementOption {
    pub input1: Option<Input>,
    pub input2: Option<Input>,
    pub motion_x: f32,
    pub motion_z: f32,
}

impl MovementOption {
    pub fn new(input1: Input, input2: Input, motion_x: f32, motion_z: f32) -> Self {
        Self {
            input1: Some(input1),
            input2: Some(input2),
            motion_x,
            motion_z,
        }
    }

    /// `MovementOption(Input, float, float)`
    pub fn single(input1: Input, motion_x: f32, motion_z: f32) -> Self {
        Self {
            input1: Some(input1),
            input2: None,
            motion_x,
            motion_z,
        }
    }

    pub fn set_inputs(&self, movement_state: &mut MovementState) {
        if let Some(input1) = self.input1 {
            movement_state.set_input(input1, true);
        }
        if let Some(input2) = self.input2 {
            movement_state.set_input(input2, true);
        }
    }

    pub fn distance_to_sq(&self, other_x: f32, other_z: f32) -> f32 {
        (self.motion_x - other_x).abs() + (self.motion_z - other_z).abs()
    }

    pub fn get_options(motion_x: f32, motion_z: f32, can_sprint: bool) -> [MovementOption; 8] {
        let forward_x = if can_sprint {
            motion_x * SPRINT_MULTIPLIER
        } else {
            motion_x
        };
        let forward_z = if can_sprint {
            motion_z * SPRINT_MULTIPLIER
        } else {
            motion_z
        };
        [
            MovementOption::single(Input::MoveForward, forward_x, forward_z),
            MovementOption::single(Input::MoveBack, -motion_x, -motion_z),
            MovementOption::single(Input::MoveLeft, -motion_z, motion_x),
            MovementOption::single(Input::MoveRight, motion_z, -motion_x),
            MovementOption::new(
                Input::MoveForward,
                Input::MoveLeft,
                forward_x - motion_z,
                forward_z + motion_x,
            ),
            MovementOption::new(
                Input::MoveForward,
                Input::MoveRight,
                forward_x + motion_z,
                forward_z - motion_x,
            ),
            MovementOption::new(
                Input::MoveBack,
                Input::MoveLeft,
                -motion_x - motion_z,
                -motion_z + motion_x,
            ),
            MovementOption::new(
                Input::MoveBack,
                Input::MoveRight,
                -motion_x + motion_z,
                -motion_z - motion_x,
            ),
        ]
    }
}

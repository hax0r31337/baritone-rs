// Ported from baritone src/main/java/baritone/pathing/movement/MovementState.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream's setters return `this` for chaining; the port's return `&mut Self`. A new state's
// status is `PREPPING`: upstream leaves it null, and every state it creates sets `PREPPING`
// right away.

use std::collections::BTreeMap;

use crate::api::pathing::movement::MovementStatus;
use crate::api::utils::Rotation;
use crate::api::utils::input::Input;

#[derive(Clone, Debug, PartialEq)]
pub struct MovementState {
    status: MovementStatus,
    target: MovementTarget,
    input_state: BTreeMap<Input, bool>,
}

impl Default for MovementState {
    fn default() -> Self {
        Self {
            status: MovementStatus::Prepping,
            target: MovementTarget::default(),
            input_state: BTreeMap::new(),
        }
    }
}

impl MovementState {
    pub fn set_status(&mut self, status: MovementStatus) -> &mut Self {
        self.status = status;
        self
    }

    pub fn get_status(&self) -> MovementStatus {
        self.status
    }

    pub fn get_target(&self) -> &MovementTarget {
        &self.target
    }

    pub fn set_target(&mut self, target: MovementTarget) -> &mut Self {
        self.target = target;
        self
    }

    pub fn set_input(&mut self, input: Input, forced: bool) -> &mut Self {
        self.input_state.insert(input, forced);
        self
    }

    pub fn get_input_states(&self) -> &BTreeMap<Input, bool> {
        &self.input_state
    }

    pub fn get_input_states_mut(&mut self) -> &mut BTreeMap<Input, bool> {
        &mut self.input_state
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MovementTarget {
    /// Yaw and pitch angles that must be matched
    pub rotation: Option<Rotation>,

    /// Whether or not this target must force rotations.
    ///
    /// `true` if we're trying to place or break blocks, `false` if we're trying to look at the movement location
    force_rotations: bool,
}

impl MovementTarget {
    pub fn new(rotation: Rotation, force_rotations: bool) -> Self {
        Self {
            rotation: Some(rotation),
            force_rotations,
        }
    }

    pub fn get_rotation(&self) -> Option<Rotation> {
        self.rotation
    }

    pub fn has_to_force_rotations(&self) -> bool {
        self.force_rotations
    }
}

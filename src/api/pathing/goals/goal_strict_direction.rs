// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalStrictDirection.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::settings_util::maybe_censor;
use crate::mc::Direction;

/// Dig a tunnel in a certain direction, but if you have to deviate from the path, go back to
/// where you started
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalStrictDirection {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub dx: i32,
    pub dz: i32,
}

impl GoalStrictDirection {
    /// Panics for vertical directions, like upstream's `IllegalArgumentException`.
    pub fn new(origin: BetterBlockPos, direction: Direction) -> Self {
        let dx = direction.get_step_x();
        let dz = direction.get_step_z();
        if dx == 0 && dz == 0 {
            panic!("{direction}");
        }
        Self {
            x: origin.x,
            y: origin.y,
            z: origin.z,
            dx,
            dz,
        }
    }
}

impl Goal for GoalStrictDirection {
    fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let x_off = x.wrapping_sub(self.x);
        let z_off = z.wrapping_sub(self.z);
        let distance_from_start_in_desired_direction = x_off
            .wrapping_mul(self.dx)
            .wrapping_add(z_off.wrapping_mul(self.dz));

        let distance_from_start_in_incorrect_direction = x_off
            .wrapping_mul(self.dz)
            .wrapping_abs()
            .wrapping_add(z_off.wrapping_mul(self.dx).wrapping_abs());

        let vertical_distance_from_start = y.wrapping_sub(self.y).wrapping_abs();

        // we want heuristic to decrease as desiredDirection increases
        let mut heuristic = distance_from_start_in_desired_direction
            .wrapping_neg()
            .wrapping_mul(100) as f64;

        heuristic += distance_from_start_in_incorrect_direction.wrapping_mul(1000) as f64;
        heuristic += vertical_distance_from_start.wrapping_mul(1000) as f64;
        heuristic
    }

    fn heuristic_at_goal(&self) -> f64 {
        f64::NEG_INFINITY
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalStrictDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalStrictDirection{{x={}, y={}, z={}, dx={}, dz={}}}",
            maybe_censor(self.x),
            maybe_censor(self.y),
            maybe_censor(self.z),
            maybe_censor(self.dx),
            maybe_censor(self.dz)
        )
    }
}

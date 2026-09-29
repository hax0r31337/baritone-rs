// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalXZ.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::settings_util::maybe_censor;
use crate::mc::{Vec3, mth};
use crate::settings::with_settings;

const SQRT_2: f64 = std::f64::consts::SQRT_2;

/// Useful for long-range goals that don't have a specific Y level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalXZ {
    /// The X block position of this goal
    x: i32,
    /// The Z block position of this goal
    z: i32,
}

impl GoalXZ {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// `new GoalXZ(BetterBlockPos)`
    pub fn from_pos(pos: BetterBlockPos) -> Self {
        Self::new(pos.x, pos.z)
    }

    pub fn calculate(x_diff: f64, z_diff: f64) -> f64 {
        //This is a combination of pythagorean and manhattan distance
        //It takes into account the fact that pathing can either walk diagonally or forwards

        //It's not possible to walk forward 1 and right 2 in sqrt(5) time
        //It's really 1+sqrt(2) because it'll walk forward 1 then diagonally 1
        let x = x_diff.abs();
        let z = z_diff.abs();
        let straight;
        let mut diagonal;
        if x < z {
            straight = z - x;
            diagonal = x;
        } else {
            straight = x - z;
            diagonal = z;
        }
        diagonal *= SQRT_2;
        (diagonal + straight) * with_settings(|settings| settings.cost_heuristic) // big TODO tune
    }

    pub fn from_direction(origin: Vec3, yaw: f32, distance: f64) -> GoalXZ {
        let theta = (yaw as f64).to_radians() as f32;
        let x = origin.x - mth::sin(theta as f64) as f64 * distance;
        let z = origin.z + mth::cos(theta as f64) as f64 * distance;
        GoalXZ::new(mth::floor(x), mth::floor(z))
    }

    pub fn get_x(&self) -> i32 {
        self.x
    }

    pub fn get_z(&self) -> i32 {
        self.z
    }
}

impl Goal for GoalXZ {
    fn is_in_goal(&self, x: i32, _y: i32, z: i32) -> bool {
        x == self.x && z == self.z
    }

    fn heuristic(&self, x: i32, _y: i32, z: i32) -> f64 {
        //mostly copied from GoalBlock
        let x_diff = x.wrapping_sub(self.x);
        let z_diff = z.wrapping_sub(self.z);
        Self::calculate(x_diff as f64, z_diff as f64)
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalXZ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalXZ{{x={},z={}}}",
            maybe_censor(self.x),
            maybe_censor(self.z)
        )
    }
}

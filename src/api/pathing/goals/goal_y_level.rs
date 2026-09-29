// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalYLevel.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, goal_equals};
use crate::api::pathing::movement::with_action_costs;
use crate::api::utils::settings_util::maybe_censor;

/// Useful for mining (getting to diamond / iron level)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalYLevel {
    /// The target Y level
    pub level: i32,
}

impl GoalYLevel {
    pub fn new(level: i32) -> Self {
        Self { level }
    }

    pub fn calculate(goal_y: i32, current_y: i32) -> f64 {
        if current_y > goal_y {
            // need to descend
            return with_action_costs(|costs| costs.fall_n_blocks_cost[2]) / 2.0
                * current_y.wrapping_sub(goal_y) as f64;
        }
        if current_y < goal_y {
            // need to ascend
            return goal_y.wrapping_sub(current_y) as f64
                * with_action_costs(|costs| costs.jump_one_block_cost);
        }
        0.0
    }
}

impl Goal for GoalYLevel {
    fn is_in_goal(&self, _x: i32, y: i32, _z: i32) -> bool {
        y == self.level
    }

    fn heuristic(&self, _x: i32, y: i32, _z: i32) -> f64 {
        Self::calculate(self.level, y)
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalYLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalYLevel{{y={}}}", maybe_censor(self.level))
    }
}

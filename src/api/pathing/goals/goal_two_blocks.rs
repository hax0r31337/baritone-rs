// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalTwoBlocks.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalBlock, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::interfaces::IGoalRenderPos;
use crate::api::utils::settings_util::maybe_censor;

/// Useful if the goal is just to mine a block. This goal will be satisfied if the specified
/// BlockPos is at to or above the specified position for this goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalTwoBlocks {
    /// The X block position of this goal
    pub x: i32,
    /// The Y block position of this goal
    pub y: i32,
    /// The Z block position of this goal
    pub z: i32,
}

impl GoalTwoBlocks {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// `new GoalTwoBlocks(BlockPos)`
    pub fn from_pos(pos: BetterBlockPos) -> Self {
        Self::new(pos.x, pos.y, pos.z)
    }
}

impl Goal for GoalTwoBlocks {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        x == self.x && (y == self.y || y == self.y.wrapping_sub(1)) && z == self.z
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let x_diff = x.wrapping_sub(self.x);
        let y_diff = y.wrapping_sub(self.y);
        let z_diff = z.wrapping_sub(self.z);
        GoalBlock::calculate(
            x_diff as f64,
            if y_diff < 0 { y_diff + 1 } else { y_diff },
            z_diff as f64,
        )
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }

    fn as_goal_render_pos(&self) -> Option<&dyn IGoalRenderPos> {
        Some(self)
    }
}

impl IGoalRenderPos for GoalTwoBlocks {
    fn get_goal_pos(&self) -> BetterBlockPos {
        BetterBlockPos::new(self.x, self.y, self.z)
    }
}

impl fmt::Display for GoalTwoBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalTwoBlocks{{x={},y={},z={}}}",
            maybe_censor(self.x),
            maybe_censor(self.y),
            maybe_censor(self.z)
        )
    }
}

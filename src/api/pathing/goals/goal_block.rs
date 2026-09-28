// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalBlock.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalXZ, GoalYLevel, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::interfaces::IGoalRenderPos;
use crate::api::utils::settings_util::maybe_censor;

/// A specific BlockPos goal
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalBlock {
    /// The X block position of this goal
    pub x: i32,
    /// The Y block position of this goal
    pub y: i32,
    /// The Z block position of this goal
    pub z: i32,
}

impl GoalBlock {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// `new GoalBlock(BlockPos)`
    pub fn from_pos(pos: BetterBlockPos) -> Self {
        Self::new(pos.x, pos.y, pos.z)
    }

    pub fn calculate(x_diff: f64, y_diff: i32, z_diff: f64) -> f64 {
        let mut heuristic = 0.0;

        // if yDiff is 1 that means that currentY-goalY==1 which means that we're 1 block above where we should be
        // therefore going from 0,yDiff,0 to a GoalYLevel of 0 is accurate
        heuristic += GoalYLevel::calculate(0, y_diff);

        //use the pythagorean and manhattan mixture from GoalXZ
        heuristic += GoalXZ::calculate(x_diff, z_diff);
        heuristic
    }
}

impl Goal for GoalBlock {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        x == self.x && y == self.y && z == self.z
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let x_diff = x.wrapping_sub(self.x);
        let y_diff = y.wrapping_sub(self.y);
        let z_diff = z.wrapping_sub(self.z);
        Self::calculate(x_diff as f64, y_diff, z_diff as f64)
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }

    fn as_goal_render_pos(&self) -> Option<&dyn IGoalRenderPos> {
        Some(self)
    }
}

impl IGoalRenderPos for GoalBlock {
    /// The position of this goal as a BlockPos
    fn get_goal_pos(&self) -> BetterBlockPos {
        BetterBlockPos::new(self.x, self.y, self.z)
    }
}

impl fmt::Display for GoalBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalBlock{{x={},y={},z={}}}",
            maybe_censor(self.x),
            maybe_censor(self.y),
            maybe_censor(self.z)
        )
    }
}

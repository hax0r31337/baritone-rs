// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalGetToBlock.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalBlock, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::interfaces::IGoalRenderPos;
use crate::api::utils::settings_util::maybe_censor;

/// Don't get into the block, but get directly adjacent to it. Useful for chests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalGetToBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl GoalGetToBlock {
    pub fn new(pos: BetterBlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

impl Goal for GoalGetToBlock {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        let x_diff = x.wrapping_sub(self.x);
        let y_diff = y.wrapping_sub(self.y);
        let z_diff = z.wrapping_sub(self.z);
        x_diff
            .wrapping_abs()
            .wrapping_add((if y_diff < 0 { y_diff + 1 } else { y_diff }).wrapping_abs())
            .wrapping_add(z_diff.wrapping_abs())
            <= 1
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

impl IGoalRenderPos for GoalGetToBlock {
    fn get_goal_pos(&self) -> BetterBlockPos {
        BetterBlockPos::new(self.x, self.y, self.z)
    }
}

impl fmt::Display for GoalGetToBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalGetToBlock{{x={},y={},z={}}}",
            maybe_censor(self.x),
            maybe_censor(self.y),
            maybe_censor(self.z)
        )
    }
}

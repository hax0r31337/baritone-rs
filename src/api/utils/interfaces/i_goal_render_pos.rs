// Ported from baritone src/api/java/baritone/api/utils/interfaces/IGoalRenderPos.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::utils::BetterBlockPos;

/// Goals that have a single representative position. `goal instanceof IGoalRenderPos` is
/// `goal.as_goal_render_pos()` in the port.
pub trait IGoalRenderPos {
    fn get_goal_pos(&self) -> BetterBlockPos;
}

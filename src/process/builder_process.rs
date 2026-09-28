// Ported from baritone src/main/java/baritone/process/BuilderProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only `GoalBreak`, which the farm process uses, and `placementPlausible`, which the backfill
// process uses; the builder itself is not ported (plans/port.md). `placementPlausible` is a
// function of the player context. For `Level.isUnobstructed`, the entities in the way are the
// local player and the entities whose `blocks_building` is set (the host leaves spectators
// out), and an entity is in the way when its box overlaps one of the block's collision boxes
// (`Shapes.joinIsNotEmpty(shape, Shapes.create(box), AND)`).

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalGetToBlock, goal_equals};
use crate::api::utils::interfaces::IGoalRenderPos;
use crate::api::utils::settings_util::maybe_censor;
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::BlockState;
use crate::mc::Aabb;

/// `placementPlausible(BlockPos, BlockState)`
pub fn placement_plausible(
    ctx: &dyn IPlayerContext,
    pos: BetterBlockPos,
    state: &BlockState,
) -> bool {
    let voxelshape = state.get_collision_shape(pos);
    voxelshape.is_empty() || {
        let boxes: Vec<Aabb> = voxelshape.to_aabbs().map(|b| b.move_pos(pos)).collect();
        is_unobstructed(ctx, &boxes)
    }
}

/// `Level.isUnobstructed(null, shape)`
fn is_unobstructed(ctx: &dyn IPlayerContext, shape: &[Aabb]) -> bool {
    let overlaps = |bb: &Aabb| shape.iter().any(|b| b.intersects(bb));
    !overlaps(&ctx.player().bounding_box)
        && !ctx
            .entities()
            .iter()
            .any(|entity| entity.blocks_building && overlaps(&entity.bounding_box))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalBreak {
    goal: GoalGetToBlock,
}

impl GoalBreak {
    pub fn new(pos: BetterBlockPos) -> Self {
        Self {
            goal: GoalGetToBlock::new(pos),
        }
    }
}

impl Goal for GoalBreak {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        // can't stand right on top of a block, that might not work (what if it's unsupported, can't break then)
        if y > self.goal.y {
            return false;
        }
        // but any other adjacent works for breaking, including inside or below
        self.goal.is_in_goal(x, y, z)
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.goal.heuristic(x, y, z)
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }

    fn as_goal_render_pos(&self) -> Option<&dyn IGoalRenderPos> {
        Some(self)
    }
}

impl IGoalRenderPos for GoalBreak {
    fn get_goal_pos(&self) -> BetterBlockPos {
        self.goal.get_goal_pos()
    }
}

impl fmt::Display for GoalBreak {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalBreak{{x={},y={},z={}}}",
            maybe_censor(self.goal.x),
            maybe_censor(self.goal.y),
            maybe_censor(self.goal.z)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pathing::goals::GoalGetToBlock;

    #[test]
    fn goal_break() {
        let goal = GoalBreak::new(BetterBlockPos::new(3, 64, 3));
        assert!(goal.is_in_goal(3, 63, 3));
        assert!(goal.is_in_goal(4, 64, 3));
        assert!(!goal.is_in_goal(3, 65, 3));
        assert!(!goal.is_in_goal(5, 64, 3));
        assert_eq!(
            goal.heuristic(10, 70, 3),
            GoalGetToBlock::new(BetterBlockPos::new(3, 64, 3)).heuristic(10, 70, 3)
        );
        assert!(goal.equals(&GoalBreak::new(BetterBlockPos::new(3, 64, 3))));
        assert!(!goal.equals(&GoalGetToBlock::new(BetterBlockPos::new(3, 64, 3))));
        assert_eq!(goal.to_string(), "GoalBreak{x=3,y=64,z=3}");
    }
}

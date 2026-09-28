// Ported from baritone src/api/java/baritone/api/pathing/goals/Goal.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::any::Any;
use std::fmt;

use crate::api::utils::BetterBlockPos;
use crate::api::utils::interfaces::IGoalRenderPos;

/// An abstract Goal for pathing, can be anything from a specific block to just a Y coordinate.
///
/// Port notes: `equals` compares the concrete type and fields like upstream's `equals`
/// (`dyn Goal` also implements `PartialEq` through it). `hashCode` is not ported: kept code
/// never hashes goals. `instanceof` checks downcast through `Any`
/// (`(goal as &dyn Any).downcast_ref::<GoalBlock>()`), except `instanceof IGoalRenderPos`,
/// which is [`Goal::as_goal_render_pos`].
pub trait Goal: Any + Send + Sync + fmt::Debug + fmt::Display {
    /// Returns whether or not the specified position meets the requirement for this goal based.
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool;

    /// Estimate the number of ticks it will take to get to the goal
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64;

    /// `isInGoal(BlockPos)`
    fn is_in_goal_pos(&self, pos: BetterBlockPos) -> bool {
        self.is_in_goal(pos.x, pos.y, pos.z)
    }

    /// `heuristic(BlockPos)`
    fn heuristic_pos(&self, pos: BetterBlockPos) -> f64 {
        self.heuristic(pos.x, pos.y, pos.z)
    }

    /// `heuristic()`: the heuristic at the goal.
    /// i.e. `heuristic_at_goal() == heuristic(x,y,z)` when `is_in_goal(x,y,z) == true`.
    /// This is needed by `PathingBehavior#estimatedTicksToGoal` because some Goals actually do
    /// not have a heuristic of 0 when that condition is met
    fn heuristic_at_goal(&self) -> f64 {
        0.0
    }

    /// `equals(Object)`
    fn equals(&self, other: &dyn Goal) -> bool;

    /// `this instanceof IGoalRenderPos ? (IGoalRenderPos) this : null`
    fn as_goal_render_pos(&self) -> Option<&dyn IGoalRenderPos> {
        None
    }
}

impl PartialEq for dyn Goal {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

/// `equals` for goals whose Java `equals` is `getClass() == o.getClass()` plus field equality.
pub fn goal_equals<T: Goal + PartialEq>(this: &T, other: &dyn Goal) -> bool {
    (other as &dyn Any)
        .downcast_ref::<T>()
        .is_some_and(|other| this == other)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api::pathing::goals::{
        GoalBlock, GoalComposite, GoalInverted, GoalRunAway, GoalTwoBlocks, GoalXZ, GoalYLevel,
    };

    #[test]
    fn equals_compares_type_and_fields() {
        let a: Arc<dyn Goal> = Arc::new(GoalBlock::new(1, 2, 3));
        assert!(a.equals(&GoalBlock::new(1, 2, 3)));
        assert!(!a.equals(&GoalBlock::new(1, 2, 4)));
        // same fields, different class
        assert!(!a.equals(&GoalTwoBlocks::new(1, 2, 3)));
        assert!(*a == *(Arc::new(GoalBlock::new(1, 2, 3)) as Arc<dyn Goal>));
    }

    #[test]
    fn equals_nested() {
        let composite = |level| {
            GoalComposite::new(vec![
                Arc::new(GoalXZ::new(5, 6)),
                Arc::new(GoalYLevel::new(level)),
            ])
        };
        assert!(composite(12).equals(&composite(12)));
        assert!(!composite(12).equals(&composite(13)));
        let inverted = |level| GoalInverted::new(Arc::new(composite(level)));
        assert!(inverted(12).equals(&inverted(12)));
        assert!(!inverted(12).equals(&inverted(13)));
        assert!(!inverted(12).equals(&composite(12)));
    }

    #[test]
    fn run_away_equality_and_render_pos() {
        let from = vec![BetterBlockPos::new(0, 64, 0)];
        let a = GoalRunAway::new(5.5, None, from.clone());
        assert!(a.equals(&GoalRunAway::new(5.5, None, from.clone())));
        assert!(!a.equals(&GoalRunAway::new(5.5, Some(64), from)));
        assert!(a.as_goal_render_pos().is_none());
        assert_eq!(
            GoalBlock::new(1, 2, 3)
                .as_goal_render_pos()
                .map(|g| g.get_goal_pos()),
            Some(BetterBlockPos::new(1, 2, 3))
        );
    }

    #[test]
    #[should_panic(expected = "Positions to run away from must not be empty")]
    fn run_away_rejects_empty() {
        GoalRunAway::new(1.0, None, vec![]);
    }
}

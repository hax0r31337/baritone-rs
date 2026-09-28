// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalInverted.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;
use std::sync::Arc;

use crate::api::pathing::goals::{Goal, goal_equals};

/// Invert any goal.
///
/// In the old chat control system, #invert just tried to pick a GoalRunAway that
/// *effectively* inverted the current goal. This goal just reverses the heuristic to act as a
/// TRUE invert. Inverting a Y level? Baritone tries to get away from that Y level. Inverting a
/// GoalBlock? Baritone will try to make distance whether it's in the X, Y or Z directions. And
/// of course, you can always invert a GoalXZ.
#[derive(Clone, Debug)]
pub struct GoalInverted {
    pub origin: Arc<dyn Goal>,
}

impl PartialEq for GoalInverted {
    fn eq(&self, other: &Self) -> bool {
        self.origin.equals(&*other.origin)
    }
}

impl GoalInverted {
    pub fn new(origin: Arc<dyn Goal>) -> Self {
        Self { origin }
    }
}

impl Goal for GoalInverted {
    fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        -self.origin.heuristic(x, y, z)
    }

    fn heuristic_at_goal(&self) -> f64 {
        f64::NEG_INFINITY
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalInverted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalInverted{{{}}}", self.origin)
    }
}

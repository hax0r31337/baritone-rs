// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalComposite.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;
use std::sync::Arc;

use crate::api::pathing::goals::{Goal, goal_equals};
use crate::java;

/// A composite of many goals, any one of which satisfies the composite.
/// For example, a GoalComposite of block goals for every oak log in loaded chunks
/// would result in it pathing to the easiest oak log to get to
#[derive(Clone, Debug, PartialEq)]
pub struct GoalComposite {
    /// An array of goals that any one of must be satisfied
    goals: Vec<Arc<dyn Goal>>,
}

impl GoalComposite {
    pub fn new(goals: Vec<Arc<dyn Goal>>) -> Self {
        Self { goals }
    }

    pub fn goals(&self) -> &[Arc<dyn Goal>] {
        &self.goals
    }
}

impl Goal for GoalComposite {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        self.goals.iter().any(|goal| goal.is_in_goal(x, y, z))
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let mut min = f64::MAX;
        for g in &self.goals {
            // TODO technically this isn't admissible...?
            min = java::min_f64(min, g.heuristic(x, y, z)); // whichever is closest
        }
        min
    }

    fn heuristic_at_goal(&self) -> f64 {
        let mut min = f64::MAX;
        for g in &self.goals {
            // just take the highest value that is guaranteed to be inside the goal
            min = java::min_f64(min, g.heuristic_at_goal());
        }
        min
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalComposite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GoalComposite[")?;
        for (i, goal) in self.goals.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{goal}")?;
        }
        f.write_str("]")
    }
}

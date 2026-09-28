// Ported from baritone src/main/java/baritone/pathing/calc/PathNode.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Nodes live in an arena (`Vec<PathNode>`) owned by the search; `previous` is an index into
// it. `equals`/`hashCode` compare the position, like upstream.

use std::hash::{Hash, Hasher};

use crate::api::pathing::goals::Goal;
use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::api::utils::settings_util::maybe_censor;

/// A node in the path, containing the cost and steps to get to it.
#[derive(Clone, Copy, Debug)]
pub struct PathNode {
    /// The position of this node
    pub x: i32,
    pub y: i32,
    pub z: i32,

    /// Cached, should always be equal to goal.heuristic(pos)
    pub estimated_cost_to_goal: f64,

    /// Total cost of getting from start to here
    /// Mutable and changed by PathFinder
    pub cost: f64,

    /// Should always be equal to estimatedCosttoGoal + cost
    /// Mutable and changed by PathFinder
    pub combined_cost: f64,

    /// In the graph search, what previous node contributed to the cost
    /// Mutable and changed by PathFinder
    ///
    /// An index into the search's node arena.
    pub previous: Option<u32>,

    /// Where is this node in the array flattenization of the binary heap? Needed for decrease-key operations.
    pub heap_position: i32,
}

impl PathNode {
    pub fn new(x: i32, y: i32, z: i32, goal: &dyn Goal) -> Self {
        let estimated_cost_to_goal = goal.heuristic(x, y, z);
        if estimated_cost_to_goal.is_nan() {
            panic!(
                "{} calculated implausible heuristic NaN at {} {} {}",
                goal,
                maybe_censor(x),
                maybe_censor(y),
                maybe_censor(z)
            );
        }
        Self {
            x,
            y,
            z,
            estimated_cost_to_goal,
            cost: COST_INF,
            combined_cost: 0.0,
            previous: None,
            heap_position: -1,
        }
    }

    pub fn is_open(&self) -> bool {
        self.heap_position != -1
    }

    /// `hashCode()`
    pub fn hash_code(&self) -> i32 {
        BetterBlockPos::long_hash(self.x, self.y, self.z) as i32
    }
}

impl PartialEq for PathNode {
    fn eq(&self, other: &Self) -> bool {
        // GOTTA GO FAST
        // ALL THESE CHECKS ARE FOR PEOPLE WHO WANT SLOW CODE
        // SKRT SKRT
        self.x == other.x && self.y == other.y && self.z == other.z
    }
}

impl Eq for PathNode {}

impl Hash for PathNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash_code().hash(state);
    }
}

// Ported from baritone src/main/java/baritone/pathing/calc/openset/LinkedListOpenSet.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The list is a `Vec` whose last element is the head (`first`). `removeLowest` on an empty set
// panics instead of returning `null`.

use crate::pathing::calc::PathNode;
use crate::pathing::calc::openset::IOpenSet;

/// A linked list implementation of an open set. This is the original implementation from MineBot.
/// It has incredibly fast insert performance, at the cost of O(n) removeLowest.
/// It sucks. BinaryHeapOpenSet results in more than 10x more nodes considered in 4 seconds.
#[derive(Clone, Debug, Default)]
pub struct LinkedListOpenSet {
    /// Head last.
    list: Vec<u32>,
}

impl LinkedListOpenSet {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IOpenSet for LinkedListOpenSet {
    fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    fn insert(&mut self, _nodes: &mut [PathNode], path_node: u32) {
        self.list.push(path_node);
    }

    fn update(&mut self, _nodes: &mut [PathNode], _node: u32) {}

    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> u32 {
        // walk from the head; the first of equal minima wins
        let mut best = self
            .list
            .len()
            .checked_sub(1)
            .expect("remove from an empty LinkedListOpenSet");
        let mut best_value = nodes[self.list[best] as usize].combined_cost;
        for i in (0..best).rev() {
            let comp = nodes[self.list[i] as usize].combined_cost;
            if comp < best_value {
                best_value = comp;
                best = i;
            }
        }
        self.list.remove(best)
    }
}

// Ported from baritone src/main/java/baritone/pathing/calc/openset/IOpenSet.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Nodes are indices into the search's arena, which every call takes.

use crate::pathing::calc::PathNode;

/// An open set for A* or similar graph search algorithm
pub trait IOpenSet {
    /// Inserts the specified node into the heap
    fn insert(&mut self, nodes: &mut [PathNode], node: u32);

    /// Returns `true` if the heap has no elements; `false` otherwise.
    fn is_empty(&self) -> bool;

    /// Removes and returns the minimum element in the heap.
    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> u32;

    /// A faster path has been found to this node, decreasing its cost. Perform a decrease-key operation.
    fn update(&mut self, nodes: &mut [PathNode], node: u32);
}

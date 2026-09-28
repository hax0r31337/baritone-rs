// Ported from baritone src/main/java/baritone/pathing/calc/openset/BinaryHeapOpenSet.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::pathing::calc::PathNode;
use crate::pathing::calc::openset::IOpenSet;

/// Stands in for `null` in slot 0, which the heap never reads through.
const NONE: u32 = u32::MAX;

/// A binary heap implementation of an open set. This is the one used in the AStarPathFinder.
#[derive(Clone, Debug)]
pub struct BinaryHeapOpenSet {
    /// The array backing the heap. Slot 0 is unused; `array.len() == size + 1`.
    array: Vec<u32>,
}

impl Default for BinaryHeapOpenSet {
    fn default() -> Self {
        Self::new()
    }
}

impl BinaryHeapOpenSet {
    /// The initial capacity of the heap (2^10)
    const INITIAL_CAPACITY: usize = 1024;

    pub fn new() -> Self {
        Self::with_capacity(Self::INITIAL_CAPACITY)
    }

    /// `BinaryHeapOpenSet(int)`
    pub fn with_capacity(size: usize) -> Self {
        let mut array = Vec::with_capacity(size);
        array.push(NONE);
        Self { array }
    }

    pub fn size(&self) -> usize {
        self.array.len() - 1
    }
}

impl IOpenSet for BinaryHeapOpenSet {
    fn insert(&mut self, nodes: &mut [PathNode], value: u32) {
        self.array.push(value);
        nodes[value as usize].heap_position = self.size() as i32;
        self.update(nodes, value);
    }

    fn update(&mut self, nodes: &mut [PathNode], val: u32) {
        let mut index = nodes[val as usize].heap_position as usize;
        let mut parent_ind = index >> 1;
        let cost = nodes[val as usize].combined_cost;
        let mut parent_node = self.array[parent_ind];
        while index > 1 && nodes[parent_node as usize].combined_cost > cost {
            self.array[index] = parent_node;
            self.array[parent_ind] = val;
            nodes[val as usize].heap_position = parent_ind as i32;
            nodes[parent_node as usize].heap_position = index as i32;
            index = parent_ind;
            parent_ind = index >> 1;
            parent_node = self.array[parent_ind];
        }
    }

    fn is_empty(&self) -> bool {
        self.size() == 0
    }

    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> u32 {
        let size = self.size();
        if size == 0 {
            panic!("Cannot remove from empty heap");
        }
        let result = self.array[1];
        let val = self.array[size];
        self.array[1] = val;
        nodes[val as usize].heap_position = 1;
        self.array.pop();
        let size = size - 1;
        nodes[result as usize].heap_position = -1;
        if size < 2 {
            return result;
        }
        let mut index = 1;
        let mut smaller_child = 2;
        let cost = nodes[val as usize].combined_cost;
        loop {
            let mut smaller_child_node = self.array[smaller_child];
            let mut smaller_child_cost = nodes[smaller_child_node as usize].combined_cost;
            if smaller_child < size {
                let right_child_node = self.array[smaller_child + 1];
                let right_child_cost = nodes[right_child_node as usize].combined_cost;
                if smaller_child_cost > right_child_cost {
                    smaller_child += 1;
                    smaller_child_cost = right_child_cost;
                    smaller_child_node = right_child_node;
                }
            }
            if cost <= smaller_child_cost {
                break;
            }
            self.array[index] = smaller_child_node;
            self.array[smaller_child] = val;
            nodes[val as usize].heap_position = smaller_child as i32;
            nodes[smaller_child_node as usize].heap_position = index as i32;
            index = smaller_child;
            smaller_child <<= 1;
            if smaller_child > size {
                break;
            }
        }
        result
    }
}

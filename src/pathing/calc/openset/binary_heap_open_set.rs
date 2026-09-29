// Ported from baritone src/main/java/baritone/pathing/calc/openset/BinaryHeapOpenSet.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::pathing::calc::PathNode;
use crate::pathing::calc::openset::IOpenSet;

/// Stands in for `null` in slot 0, which the heap never reads through.
const NONE: Entry = Entry {
    combined_cost: f64::NAN,
    node: u32::MAX,
};

/// A node and its `combined_cost`. Not in upstream: keeping the cost in the heap spares a
/// sift a cache miss on every node it compares. The cost is copied when the node is inserted
/// or updated, the only times a queued node's cost changes.
#[derive(Clone, Copy, Debug)]
struct Entry {
    combined_cost: f64,
    node: u32,
}

/// A binary heap implementation of an open set. This is the one used in the AStarPathFinder.
#[derive(Clone, Debug)]
pub struct BinaryHeapOpenSet {
    /// The array backing the heap. Slot 0 is unused; `array.len() == size + 1`.
    array: Vec<Entry>,
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
        self.array.push(Entry {
            combined_cost: nodes[value as usize].combined_cost,
            node: value,
        });
        nodes[value as usize].heap_position = self.size() as i32;
        self.update(nodes, value);
    }

    fn update(&mut self, nodes: &mut [PathNode], val: u32) {
        let mut index = nodes[val as usize].heap_position as usize;
        let mut parent_ind = index >> 1;
        let cost = nodes[val as usize].combined_cost;
        let entry = Entry {
            combined_cost: cost,
            node: val,
        };
        self.array[index] = entry;
        let mut parent = self.array[parent_ind];
        while index > 1 && parent.combined_cost > cost {
            self.array[index] = parent;
            self.array[parent_ind] = entry;
            nodes[val as usize].heap_position = parent_ind as i32;
            nodes[parent.node as usize].heap_position = index as i32;
            index = parent_ind;
            parent_ind = index >> 1;
            parent = self.array[parent_ind];
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
        let result = self.array[1].node;
        let val = self.array[size];
        self.array[1] = val;
        nodes[val.node as usize].heap_position = 1;
        self.array.pop();
        let size = size - 1;
        nodes[result as usize].heap_position = -1;
        if size < 2 {
            return result;
        }
        let mut index = 1;
        let mut smaller_child = 2;
        let cost = val.combined_cost;
        loop {
            let mut smaller_child_entry = self.array[smaller_child];
            if smaller_child < size {
                let right_child_entry = self.array[smaller_child + 1];
                if smaller_child_entry.combined_cost > right_child_entry.combined_cost {
                    smaller_child += 1;
                    smaller_child_entry = right_child_entry;
                }
            }
            if cost <= smaller_child_entry.combined_cost {
                break;
            }
            self.array[index] = smaller_child_entry;
            self.array[smaller_child] = val;
            nodes[val.node as usize].heap_position = smaller_child as i32;
            nodes[smaller_child_entry.node as usize].heap_position = index as i32;
            index = smaller_child;
            smaller_child <<= 1;
            if smaller_child > size {
                break;
            }
        }
        result
    }
}

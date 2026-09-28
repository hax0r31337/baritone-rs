pub mod binary_heap_open_set;
pub mod i_open_set;
pub mod linked_list_open_set;

pub use binary_heap_open_set::BinaryHeapOpenSet;
pub use i_open_set::IOpenSet;
pub use linked_list_open_set::LinkedListOpenSet;

#[cfg(test)]
mod tests {
    // Ported from baritone src/test/java/baritone/pathing/calc/openset/OpenSetsTest.java

    use rustc_hash::FxHashSet;

    use super::*;
    use crate::api::pathing::goals::GoalYLevel;
    use crate::pathing::calc::PathNode;

    /// `Math.random()` stand-in (xorshift64*, fixed seed).
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn remove_and_test(
        amount: usize,
        test: &mut [&mut dyn IOpenSet],
        nodes: &mut [PathNode],
        must_contain: Option<&FxHashSet<u32>>,
    ) {
        let mut results = vec![vec![0.0; amount]; test.len()];
        for (set, result) in test.iter_mut().zip(&mut results) {
            for slot in result.iter_mut() {
                let pn = set.remove_lowest(nodes);
                if let Some(must_contain) = must_contain {
                    assert!(must_contain.contains(&pn), "{pn}");
                }
                *slot = nodes[pn as usize].combined_cost;
            }
        }
        for result in &results[1..] {
            assert_eq!(*result, results[0]);
        }
        for i in 0..amount.saturating_sub(1) {
            assert!(results[0][i] < results[0][i + 1]);
        }
    }

    fn test_size(size: usize, random: &mut Random) {
        // Include LinkedListOpenSet even though it's not performant because I absolutely trust that it behaves properly
        // I'm really testing the heap implementations against it as the ground truth
        let mut heap = BinaryHeapOpenSet::new();
        let mut list = LinkedListOpenSet::new();
        assert!(heap.is_empty() && list.is_empty());

        // generate the pathnodes that we'll be testing the sets on
        let goal = GoalYLevel::new(0);
        let mut nodes: Vec<PathNode> = (0..size)
            .map(|_| {
                let mut pn = PathNode::new(0, 0, 0, &goal);
                pn.combined_cost = random.next();
                pn
            })
            .collect();

        // create a list of what the first removals should be
        let mut copy: Vec<u32> = (0..size as u32).collect();
        copy.sort_by(|a, b| {
            nodes[*a as usize]
                .combined_cost
                .total_cmp(&nodes[*b as usize].combined_cost)
        });
        let lowest_quarter: FxHashSet<u32> = copy[..size / 4].iter().copied().collect();

        for i in 0..size as u32 {
            heap.insert(&mut nodes, i);
            list.insert(&mut nodes, i);
        }
        assert!(!heap.is_empty() && !list.is_empty());

        // remove a quarter of the nodes and verify that they are indeed the size/4 lowest ones
        remove_and_test(
            size / 4,
            &mut [&mut heap, &mut list],
            &mut nodes,
            Some(&lowest_quarter),
        );

        // none of them should be empty (sanity check)
        assert!(!heap.is_empty() && !list.is_empty());
        let mut cnt = 0;
        let mut i = 0;
        while cnt < size / 2 && i < size {
            if lowest_quarter.contains(&(i as u32)) {
                // these were already removed and can't be updated to test
                i += 1;
                continue;
            }
            // multiplying it by a random number between 0 and 1 is guaranteed to decrease it
            nodes[i].combined_cost *= random.next();
            heap.update(&mut nodes, i as u32);
            list.update(&mut nodes, i as u32);
            cnt += 1;
            i += 1;
        }

        assert!(!heap.is_empty() && !list.is_empty());

        // remove the remaining 3/4
        remove_and_test(
            size - size / 4,
            &mut [&mut heap, &mut list],
            &mut nodes,
            None,
        );

        // every set should now be empty
        assert!(heap.is_empty() && list.is_empty());
    }

    #[test]
    fn open_sets() {
        let mut random = Random(0x9E3779B97F4A7C15);
        let mut sizes: Vec<usize> = (1..20).collect();
        sizes.extend((100..=1000).step_by(100));
        sizes.extend([5000, 10000]);
        for size in sizes {
            test_size(size, &mut random);
        }
    }
}

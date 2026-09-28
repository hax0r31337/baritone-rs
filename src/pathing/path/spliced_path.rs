// Ported from baritone src/main/java/baritone/pathing/path/SplicedPath.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream's spliced path shares the movement objects of the paths it joins; the port copies
// them, state included (both paths are dropped right after).

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::api::pathing::calc::IPath;
use crate::api::pathing::goals::Goal;
use crate::api::utils::BetterBlockPos;
use crate::pathing::movement::Movement;
use crate::utils::BlockStateInterface;
use crate::utils::pathing::path_base;

#[derive(Debug)]
pub struct SplicedPath {
    path: Vec<BetterBlockPos>,

    movements: Vec<Movement>,

    num_nodes: i32,

    goal: Arc<dyn Goal>,
}

impl SplicedPath {
    fn new(
        path: Vec<BetterBlockPos>,
        movements: Vec<Movement>,
        num_nodes_considered: i32,
        goal: Arc<dyn Goal>,
    ) -> Self {
        let res = Self {
            path,
            movements,
            num_nodes: num_nodes_considered,
            goal,
        };
        res.sanity_check();
        res
    }

    /// Panics if the paths overlap in a way `allow_overlap_cutoff` does not allow, like
    /// upstream's `IllegalStateException`.
    pub fn try_splice(
        first: Option<&dyn IPath>,
        second: Option<&dyn IPath>,
        allow_overlap_cutoff: bool,
    ) -> Option<SplicedPath> {
        let (Some(first), Some(second)) = (first, second) else {
            return None;
        };
        if first.get_dest() != second.get_src() {
            return None;
        }
        let second_pos: FxHashSet<BetterBlockPos> = second.positions().iter().copied().collect();
        let mut first_position_in_second = None;
        for i in 0..first.length() - 1 {
            // overlap in the very last element is fine (and required) so only go up to first.length() - 1
            if second_pos.contains(&first.positions()[i]) {
                first_position_in_second = Some(i);
                break;
            }
        }
        let first_position_in_second = match first_position_in_second {
            Some(i) => {
                if !allow_overlap_cutoff {
                    return None;
                }
                i
            }
            None => first.length() - 1,
        };
        let position_in_second = second
            .positions()
            .iter()
            .position(|p| *p == first.positions()[first_position_in_second]);
        if !allow_overlap_cutoff && position_in_second != Some(0) {
            panic!("Paths to be spliced are overlapping incorrectly");
        }
        let position_in_second =
            position_in_second.expect("IndexOutOfBoundsException: position in second");
        let mut positions = Vec::new();
        let mut movements = Vec::new();
        positions.extend_from_slice(&first.positions()[..first_position_in_second + 1]);
        movements.extend_from_slice(&first.movements()[..first_position_in_second]);

        positions.extend_from_slice(&second.positions()[position_in_second + 1..second.length()]);
        movements.extend_from_slice(&second.movements()[position_in_second..second.length() - 1]);
        Some(SplicedPath::new(
            positions,
            movements,
            first
                .get_num_nodes_considered()
                .wrapping_add(second.get_num_nodes_considered()),
            Arc::clone(first.get_goal()),
        ))
    }
}

impl IPath for SplicedPath {
    fn get_goal(&self) -> &Arc<dyn Goal> {
        &self.goal
    }

    fn movements(&self) -> &[Movement] {
        &self.movements
    }

    fn movements_mut(&mut self) -> &mut [Movement] {
        &mut self.movements
    }

    fn positions(&self) -> &[BetterBlockPos] {
        &self.path
    }

    fn get_num_nodes_considered(&self) -> i32 {
        self.num_nodes
    }

    fn length(&self) -> usize {
        self.path.len()
    }

    fn cutoff_at_loaded_chunks(self: Box<Self>, bsi: &BlockStateInterface) -> Box<dyn IPath> {
        path_base::cutoff_at_loaded_chunks(self, bsi)
    }

    fn static_cutoff(self: Box<Self>, destination: Option<&dyn Goal>) -> Box<dyn IPath> {
        path_base::static_cutoff(self, destination)
    }
}

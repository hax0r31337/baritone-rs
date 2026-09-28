// Ported from baritone src/main/java/baritone/pathing/path/CutoffPath.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream holds sublist views of the previous path; the port copies them (the previous path
// is always dropped right after).

use std::sync::Arc;

use crate::api::pathing::calc::IPath;
use crate::api::pathing::goals::Goal;
use crate::api::utils::BetterBlockPos;
use crate::pathing::movement::Movement;
use crate::utils::BlockStateInterface;
use crate::utils::pathing::path_base;

#[derive(Debug)]
pub struct CutoffPath {
    path: Vec<BetterBlockPos>,

    movements: Vec<Movement>,

    num_nodes: i32,

    goal: Arc<dyn Goal>,
}

impl CutoffPath {
    /// `CutoffPath(IPath, int, int)`
    pub fn new_range(
        prev: &dyn IPath,
        first_position_to_include: usize,
        last_position_to_include: usize,
    ) -> Self {
        let path =
            prev.positions()[first_position_to_include..last_position_to_include + 1].to_vec();
        let movements =
            prev.movements()[first_position_to_include..last_position_to_include].to_vec();
        let res = Self {
            path,
            movements,
            num_nodes: prev.get_num_nodes_considered(),
            goal: Arc::clone(prev.get_goal()),
        };
        res.sanity_check();
        res
    }

    /// `CutoffPath(IPath, int)`
    pub fn new(prev: &dyn IPath, last_position_to_include: usize) -> Self {
        Self::new_range(prev, 0, last_position_to_include)
    }
}

impl IPath for CutoffPath {
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

    fn cutoff_at_loaded_chunks(self: Box<Self>, bsi: &BlockStateInterface) -> Box<dyn IPath> {
        path_base::cutoff_at_loaded_chunks(self, bsi)
    }

    fn static_cutoff(self: Box<Self>, destination: Option<&dyn Goal>) -> Box<dyn IPath> {
        path_base::static_cutoff(self, destination)
    }
}

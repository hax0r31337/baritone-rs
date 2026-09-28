// Ported from baritone src/main/java/baritone/pathing/calc/Path.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Built from the search's node arena; keeps copies of the nodes on the path. The calculation
// context is passed to `post_process` instead of being stored.

use std::fmt;
use std::sync::Arc;

use crate::api::pathing::calc::IPath;
use crate::api::pathing::goals::Goal;
use crate::api::utils::BetterBlockPos;
use crate::api::utils::helper::log_debug;
use crate::java::min_f64;
use crate::pathing::calc::PathNode;
use crate::pathing::movement::{CalculationContext, Movement, Moves};
use crate::pathing::path::CutoffPath;
use crate::utils::BlockStateInterface;
use crate::utils::pathing::path_base;

/// A node based implementation of IPath
pub struct Path {
    /// The start position of this path
    start: BetterBlockPos,

    /// The end position of this path
    end: BetterBlockPos,

    /// The blocks on the path. Guaranteed that path.get(0) equals start and
    /// path.get(path.size()-1) equals end
    path: Vec<BetterBlockPos>,

    movements: Vec<Movement>,

    nodes: Vec<PathNode>,

    goal: Arc<dyn Goal>,

    num_nodes: i32,

    verified: bool,
}

impl Path {
    /// `Path(BetterBlockPos, PathNode, PathNode, int, Goal, CalculationContext)`; `start` and
    /// `end` index `nodes`, the search's arena.
    pub fn new(
        real_start: BetterBlockPos,
        start: u32,
        end: u32,
        num_nodes: i32,
        goal: Arc<dyn Goal>,
        nodes: &[PathNode],
    ) -> Self {
        let end_node = &nodes[end as usize];
        let end_pos = BetterBlockPos::new(end_node.x, end_node.y, end_node.z);

        let mut current = Some(end);
        let mut temp_path = Vec::new();
        let mut temp_nodes = Vec::new();
        while let Some(index) = current {
            let node = nodes[index as usize];
            temp_nodes.push(node);
            temp_path.push(BetterBlockPos::new(node.x, node.y, node.z));
            current = node.previous;
        }

        // If the position the player is at is different from the position we told A* to start from,
        // and A* gave us no movements, then add a fake node that will allow a movement to be created
        // that gets us to the single position in the path.
        // See PathingBehavior#createPathfinder and https://github.com/cabaletta/baritone/pull/4519
        let start_node = &nodes[start as usize];
        let start_node_pos = BetterBlockPos::new(start_node.x, start_node.y, start_node.z);
        let start_pos;
        if real_start != start_node_pos && start_node == end_node {
            start_pos = real_start;
            let mut fake_node = PathNode::new(real_start.x, real_start.y, real_start.z, &*goal);
            fake_node.cost = 0.0;
            temp_nodes.push(fake_node);
            temp_path.push(real_start);
        } else {
            start_pos = start_node_pos;
        }

        // Nodes are traversed last to first so we need to reverse the list
        temp_path.reverse();
        temp_nodes.reverse();
        Self {
            start: start_pos,
            end: end_pos,
            path: temp_path,
            movements: Vec::new(),
            nodes: temp_nodes,
            goal,
            num_nodes,
            verified: false,
        }
    }

    fn assemble_movements(&mut self, context: &CalculationContext) -> bool {
        if self.path.is_empty() || !self.movements.is_empty() {
            panic!("Path must not be empty");
        }
        for i in 0..self.path.len() - 1 {
            let cost = self.nodes[i + 1].cost - self.nodes[i].cost;
            match Self::run_backwards(context, self.path[i], self.path[i + 1], cost) {
                None => return true,
                Some(movement) => self.movements.push(movement),
            }
        }
        false
    }

    fn run_backwards(
        context: &CalculationContext,
        src: BetterBlockPos,
        dest: BetterBlockPos,
        cost: f64,
    ) -> Option<Movement> {
        for moves in Moves::VALUES {
            let mut movement = moves.apply0(context, src);
            if movement.get_dest() == dest {
                // have to calculate the cost at calculation time so we can accurately judge whether a cost increase happened between cached calculation and real execution
                // however, taking into account possible favoring that could skew the node cost, we really want the stricter limit of the two
                // so we take the minimum of the path node cost difference, and the calculated cost
                let calculated = movement.calculate_cost(context);
                movement.override_cost(min_f64(calculated, cost));
                return Some(movement);
            }
        }
        // this is no longer called from bestPathSoFar, now it's in postprocessing
        log_debug(&format!(
            "Movement became impossible during calculation {} {} {}",
            src,
            dest,
            dest.subtract(src)
        ));
        None
    }
}

impl IPath for Path {
    fn get_goal(&self) -> &Arc<dyn Goal> {
        &self.goal
    }

    fn post_process(mut self: Box<Self>, context: &CalculationContext) -> Box<dyn IPath> {
        if self.verified {
            panic!("Path must not be verified twice");
        }
        self.verified = true;
        let failed = self.assemble_movements(context);
        for movement in &mut self.movements {
            movement.check_loaded_chunk(context);
        }

        if failed {
            // at least one movement became impossible during calculation
            let res = CutoffPath::new(&*self, self.movements().len());
            if res.movements().len() != self.movements.len() {
                panic!("Path has wrong size after cutoff");
            }
            return Box::new(res);
        }
        // more post processing here
        self.sanity_check();
        self
    }

    fn movements(&self) -> &[Movement] {
        if !self.verified {
            // edge case note: this is called during verification
            panic!("Path not yet verified");
        }
        &self.movements
    }

    fn movements_mut(&mut self) -> &mut [Movement] {
        if !self.verified {
            panic!("Path not yet verified");
        }
        &mut self.movements
    }

    fn positions(&self) -> &[BetterBlockPos] {
        &self.path
    }

    fn get_num_nodes_considered(&self) -> i32 {
        self.num_nodes
    }

    fn get_src(&self) -> BetterBlockPos {
        self.start
    }

    fn get_dest(&self) -> BetterBlockPos {
        self.end
    }

    fn cutoff_at_loaded_chunks(self: Box<Self>, bsi: &BlockStateInterface) -> Box<dyn IPath> {
        path_base::cutoff_at_loaded_chunks(self, bsi)
    }

    fn static_cutoff(self: Box<Self>, destination: Option<&dyn Goal>) -> Box<dyn IPath> {
        path_base::static_cutoff(self, destination)
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Path")
            .field("goal", &self.goal)
            .field("positions", &self.path)
            .field("movements", &self.movements)
            .field("num_nodes", &self.num_nodes)
            .field("verified", &self.verified)
            .finish()
    }
}

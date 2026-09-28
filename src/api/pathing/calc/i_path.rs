// Ported from baritone src/api/java/baritone/api/pathing/calc/IPath.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Movements are the concrete `Movement` (the only `IMovement`), so paths hand out slices of
// them. Methods that return a new path take `self: Box<Self>`: upstream replaces its reference
// with the result every time. Indices and lengths are `usize`.

use std::fmt;
use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::api::pathing::goals::Goal;
use crate::api::utils::BetterBlockPos;
use crate::pathing::movement::{CalculationContext, Movement};
use crate::utils::BlockStateInterface;

/// Paths are calculated on one thread and executed on another.
pub trait IPath: Send + Sync + fmt::Debug {
    /// Ordered list of movements to carry out.
    /// movements.get(i).getSrc() should equal positions.get(i)
    /// movements.get(i).getDest() should equal positions.get(i+1)
    /// movements.size() should equal positions.size()-1
    fn movements(&self) -> &[Movement];

    /// All positions along the way.
    /// Should begin with the same as getSrc and end with the same as getDest
    fn positions(&self) -> &[BetterBlockPos];

    /// This path is actually going to be executed in the world. Do whatever additional processing is required.
    /// (as opposed to Path objects that are just constructed every frame for rendering)
    ///
    /// Upstream's `Path` keeps its calculation context; the port passes it in.
    fn post_process(self: Box<Self>, context: &CalculationContext) -> Box<dyn IPath> {
        let _ = context;
        panic!("UnsupportedOperationException");
    }

    /// Returns the number of positions in this path. Equivalent to `positions().size()`.
    fn length(&self) -> usize {
        self.positions().len()
    }

    /// The goal that this path was calculated towards
    fn get_goal(&self) -> &Arc<dyn Goal>;

    /// Returns the number of nodes that were considered during calculation before
    /// this path was found.
    fn get_num_nodes_considered(&self) -> i32;

    /// Returns the start position of this path. This is the first element in the
    /// list that is returned by [`IPath::positions`].
    fn get_src(&self) -> BetterBlockPos {
        self.positions()[0]
    }

    /// Returns the end position of this path. This is the last element in the
    /// list that is returned by [`IPath::positions`].
    fn get_dest(&self) -> BetterBlockPos {
        let pos = self.positions();
        pos[pos.len() - 1]
    }

    /// Returns the estimated number of ticks to complete the path from the given node index.
    fn ticks_remaining_from(&self, path_position: usize) -> f64 {
        let mut sum = 0.0;
        //this is fast because we aren't requesting recalculation, it's just cached
        let movements = self.movements();
        for movement in movements.iter().skip(path_position) {
            sum += movement.get_cost();
        }
        sum
    }

    /// Cuts off this path at the loaded chunk border, and returns the resulting path. Default
    /// implementation just returns this path, without the intended functionality.
    ///
    /// The argument is supposed to be a BlockStateInterface LOL LOL LOL LOL LOL
    fn cutoff_at_loaded_chunks(self: Box<Self>, bsi: &BlockStateInterface) -> Box<dyn IPath> {
        let _ = bsi;
        panic!("UnsupportedOperationException");
    }

    /// Cuts off this path using the min length and cutoff factor settings, and returns the resulting path.
    /// Default implementation just returns this path, without the intended functionality.
    fn static_cutoff(self: Box<Self>, destination: Option<&dyn Goal>) -> Box<dyn IPath> {
        let _ = destination;
        panic!("UnsupportedOperationException");
    }

    /// Performs a series of checks to ensure that the assembly of the path went as expected.
    fn sanity_check(&self) {
        let path = self.positions();
        let movements = self.movements();
        if self.get_src() != path[0] {
            panic!("Start node does not equal first path element");
        }
        if self.get_dest() != path[path.len() - 1] {
            panic!("End node does not equal last path element");
        }
        if path.len() != movements.len() + 1 {
            panic!("Size of path array is unexpected");
        }
        let mut seen_so_far = FxHashSet::default();
        for i in 0..path.len() - 1 {
            let src = path[i];
            let dest = path[i + 1];
            let movement = &movements[i];
            if src != movement.get_src() {
                panic!("Path source is not equal to the movement source");
            }
            if dest != movement.get_dest() {
                panic!("Path destination is not equal to the movement destination");
            }
            if seen_so_far.contains(&src) {
                panic!("Path doubles back on itself, making a loop");
            }
            seen_so_far.insert(src);
        }
    }
}

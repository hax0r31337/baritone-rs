// Ported from baritone src/main/java/baritone/utils/pathing/PathBase.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The abstract base class of every path. Rust traits cannot override the `IPath` defaults, so
// its two methods are functions that each path's `IPath` implementation calls.

use crate::api::pathing::calc::IPath;
use crate::api::pathing::goals::Goal;
use crate::pathing::path::CutoffPath;
use crate::settings::settings;
use crate::utils::BlockStateInterface;

/// `cutoffAtLoadedChunks(Object)`
pub fn cutoff_at_loaded_chunks(this: Box<dyn IPath>, bsi: &BlockStateInterface) -> Box<dyn IPath> {
    // <-- cursed cursed cursed
    if !settings().cutoff_at_load_boundary {
        return this;
    }
    for (i, pos) in this.positions().iter().enumerate() {
        if !bsi.world_contains_loaded_chunk(pos.x, pos.z) {
            return Box::new(CutoffPath::new(&*this, i));
        }
    }
    this
}

/// `staticCutoff(Goal)`
pub fn static_cutoff(this: Box<dyn IPath>, destination: Option<&dyn Goal>) -> Box<dyn IPath> {
    let settings = settings();
    let min = settings.path_cutoff_minimum_length;
    let length = this.length() as i32;
    if length < min {
        return this;
    }
    if destination.is_none_or(|destination| destination.is_in_goal_pos(this.get_dest())) {
        return this;
    }
    let factor = settings.path_cutoff_factor;
    let new_length = ((length.wrapping_sub(min) as f64 * factor) as i32)
        .wrapping_add(min)
        .wrapping_sub(1);
    let new_length = usize::try_from(new_length).expect("IndexOutOfBoundsException");
    Box::new(CutoffPath::new(&*this, new_length))
}

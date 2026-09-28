// Ported from baritone src/api/java/baritone/api/pathing/movement/IMovement.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: `update`, `reset`, `resetBlockCache` and `safeToCancel`, which belong
// to execution.

use crate::api::utils::BetterBlockPos;

/// Implemented by [`crate::pathing::movement::Movement`], the only movement type.
pub trait IMovement {
    fn get_cost(&self) -> f64;

    fn calculated_while_loaded(&self) -> bool;

    fn get_src(&self) -> BetterBlockPos;

    fn get_dest(&self) -> BetterBlockPos;

    /// `BlockPos getDirection()`
    fn get_direction(&self) -> BetterBlockPos;
}

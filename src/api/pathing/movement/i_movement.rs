// Ported from baritone src/api/java/baritone/api/pathing/movement/IMovement.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `update`, `reset`, `resetBlockCache` and `safeToCancel` are `Movement`'s own methods: they
// take the `Baritone` that runs the movement, which this trait does not know.

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

//! Ports of `baritone.behavior`. `Behavior` (the base class holding the `Baritone` and its
//! player context) is not ported: behaviors get what they need passed in. `WaypointBehavior` is
//! not ported (waypoints are part of the chunk cache).

pub mod inventory_behavior;
pub mod look;
pub mod look_behavior;
pub mod pathing_behavior;

pub use inventory_behavior::InventoryBehavior;
pub use look_behavior::LookBehavior;
pub use pathing_behavior::PathingBehavior;

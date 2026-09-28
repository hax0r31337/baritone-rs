//! Ports of `baritone.process`. Phase 4 brings the two processes execution needs: the custom
//! goal process (to drive pathing) and the inventory pauser (which `InventoryBehavior` asks).

pub mod custom_goal_process;
pub mod inventory_pauser_process;

pub use custom_goal_process::CustomGoalProcess;
pub use inventory_pauser_process::InventoryPauserProcess;

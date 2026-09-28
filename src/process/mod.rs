//! Ports of `baritone.process`: every process but the builder and elytra ones, which are not
//! ported (plans/port.md). Of `BuilderProcess`, only `GoalBreak` and `placementPlausible` are,
//! for the farm and backfill processes.

pub mod backfill_process;
pub mod builder_process;
pub mod custom_goal_process;
pub mod explore_process;
pub mod farm_process;
pub mod follow_process;
pub mod get_to_block_process;
pub mod inventory_pauser_process;
pub mod mine_process;

pub use backfill_process::BackfillProcess;
pub use custom_goal_process::CustomGoalProcess;
pub use explore_process::ExploreProcess;
pub use farm_process::FarmProcess;
pub use follow_process::FollowProcess;
pub use get_to_block_process::GetToBlockProcess;
pub use inventory_pauser_process::InventoryPauserProcess;
pub use mine_process::MineProcess;

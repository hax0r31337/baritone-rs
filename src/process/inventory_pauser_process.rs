// Ported from baritone src/main/java/baritone/process/InventoryPauserProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::any::Any;

use crate::Baritone;
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::IPlayerContext;

#[derive(Debug, Default)]
pub struct InventoryPauserProcess {
    pause_requested_last_tick: bool,
    safe_to_cancel_last_tick: bool,
    ticks_of_stationary: i32,
}

impl InventoryPauserProcess {
    pub fn new() -> Self {
        Self::default()
    }

    fn motion(baritone: &Baritone) -> f64 {
        baritone
            .player_context
            .player()
            .delta_movement
            .multiply(1.0, 0.0, 1.0)
            .length()
    }

    fn stationary_now(baritone: &Baritone) -> bool {
        Self::motion(baritone) < 0.00001
    }

    pub fn stationary_for_inventory_move(&mut self) -> bool {
        self.pause_requested_last_tick = true;
        self.safe_to_cancel_last_tick && self.ticks_of_stationary > 1
    }
}

impl IBaritoneProcess for InventoryPauserProcess {
    fn is_active(&mut self, baritone: &mut Baritone) -> bool {
        baritone.player_context.is_in_world()
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        _calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        //logDebug(pauseRequestedLastTick + " " + safeToCancelLastTick + " " + ticksOfStationary);
        self.safe_to_cancel_last_tick = is_safe_to_cancel;
        if self.pause_requested_last_tick {
            self.pause_requested_last_tick = false;
            if Self::stationary_now(baritone) {
                self.ticks_of_stationary = self.ticks_of_stationary.wrapping_add(1);
            }
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        }
        self.ticks_of_stationary = 0;
        Some(PathingCommand::new(None, PathingCommandType::Defer))
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {}

    fn priority(&self) -> f64 {
        5.1 // slightly higher than backfill
    }

    fn display_name0(&mut self, _baritone: &mut Baritone) -> String {
        "inventory pauser".to_owned()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

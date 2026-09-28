// Ported from baritone src/api/java/baritone/api/process/IBaritoneProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Processes live in the `Baritone` and get it passed in (upstream: `BaritoneProcessHelper`'s
// `baritone` and `ctx` fields). While a process runs one of its methods, it is taken out of
// the `Baritone` (`Baritone::with_process`), so it cannot reach itself through it. Upstream's
// `isActive` may change things (the follow process scans the world, the backfill process
// clears the forced inputs), so it takes `&mut` like the rest. `isTemporary` has no default:
// upstream's `BaritoneProcessHelper` supplies `false`, which is not ported as a class.

use std::any::Any;

use crate::Baritone;
use crate::api::process::PathingCommand;

/// Default priority. Most normal processes should have this value.
///
/// Some examples of processes that should have different values might include some kind of automated mob avoidance
/// that would be temporary and would forcefully take control. Same for something that pauses pathing for auto eat, etc.
///
/// The value is -1 beacuse that's what Impact 4.5's beta auto walk returns and I want to tie with it.
pub const DEFAULT_PRIORITY: f64 = -1.0;

/// A process that can control the PathingBehavior.
///
/// Differences between a baritone process and a behavior:
/// - Only one baritone process can be active at a time
/// - PathingBehavior can only be controlled by a process
///
/// That's it actually
pub trait IBaritoneProcess: Any + Send {
    /// Would this process like to be in control?
    fn is_active(&mut self, baritone: &mut Baritone) -> bool;

    /// Called when this process is in control of pathing; Returns what Baritone should do.
    ///
    /// `calc_failed`: `true` if this specific process was in control last tick,
    /// and there was a `PathEvent.CALC_FAILED` event last tick.
    /// `is_safe_to_cancel`: `true` if a `REQUEST_PAUSE` would happen this tick, and
    /// `IPathingBehavior` wouldn't actually tick. `false` if the PathExecutor reported
    /// pausing would be unsafe at the end of the last tick. Effectively "could request cancel or
    /// pause and have it happen right away"
    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand>;

    /// Returns whether or not this process should be treated as "temporary".
    ///
    /// If a process is temporary, it doesn't call `on_lost_control` on the processes that aren't execute because of it.
    ///
    /// For example, `CombatPauserProcess` and `PauseForAutoEatProcess` should return `true` always,
    /// and should return `is_active` `true` only if there's something in range this tick, or if the player would like
    /// to start eating this tick. `PauseForAutoEatProcess` should only actually right click once onTick is called with
    /// `is_safe_to_cancel` true though.
    fn is_temporary(&self) -> bool;

    /// Called if `is_active` returned `true`, but another non-temporary
    /// process has control. Effectively the same as cancel. You want control but you
    /// don't get it.
    fn on_lost_control(&mut self, baritone: &mut Baritone);

    /// Used to determine which Process gains control if multiple are reporting `is_active()`. The one
    /// that returns the highest value will be given control.
    fn priority(&self) -> f64 {
        DEFAULT_PRIORITY
    }

    /// Returns a user-friendly name for this process. Suitable for a HUD.
    fn display_name(&mut self, baritone: &mut Baritone) -> String {
        if !self.is_active(baritone) {
            // i love it when impcat's scuffed HUD calls displayName for inactive processes for 1 tick too long
            // causing NPEs when the displayname relies on fields that become null when inactive
            return "INACTIVE".to_owned();
        }
        self.display_name0(baritone)
    }

    fn display_name0(&mut self, baritone: &mut Baritone) -> String;

    /// For downcasting to the concrete process.
    fn as_any(&self) -> &dyn Any;

    /// For downcasting to the concrete process.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

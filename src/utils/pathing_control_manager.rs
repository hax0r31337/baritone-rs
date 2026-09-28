// Ported from baritone src/main/java/baritone/utils/PathingControlManager.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The processes live in the `Baritone` (`Baritone::processes`); the manager refers to them by
// their index there. Upstream keeps them in a `HashSet`, whose order decides which of two
// processes that become active in the same tick goes first; the port uses registration order.
// `postTick` is registered as a tick listener upstream; `GameEventHandler` calls it last.

use crate::Baritone;
use crate::api::event::events::tick_event;
use crate::api::pathing::goals::Goal;
use crate::api::process::{PathingCommand, PathingCommandType};
use crate::behavior::PathingBehavior;
use crate::java::double_compare;
use crate::settings::settings;

#[derive(Debug, Default)]
pub struct PathingControlManager {
    /// Indices into `Baritone::processes`, most recently activated first.
    active: Vec<usize>,
    in_control_last_tick: Option<usize>,
    in_control_this_tick: Option<usize>,
    command: Option<PathingCommand>,
}

impl PathingControlManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// `registerProcess(IBaritoneProcess)` for the process at `index` in
    /// `Baritone::processes`.
    pub(crate) fn register_process(baritone: &mut Baritone, index: usize) {
        baritone.with_process(index, |process, baritone| process.on_lost_control(baritone)); // make sure it's reset
    }

    /// called by PathingBehavior on TickEvent Type OUT
    pub fn cancel_everything(baritone: &mut Baritone) {
        let this = &mut baritone.pathing_control_manager;
        this.in_control_last_tick = None;
        this.in_control_this_tick = None;
        this.command = None;
        this.active.clear();
        for index in 0..baritone.processes.len() {
            baritone.with_process(index, |proc, baritone| {
                proc.on_lost_control(baritone);
                if proc.is_active(baritone) && !proc.is_temporary() {
                    // it's okay only for a temporary thing (like combat pause) to maintain control even if you say to cancel
                    panic!(
                        "{} stayed active after being cancelled",
                        proc.display_name(baritone)
                    );
                }
            });
        }
    }

    /// The index in `Baritone::processes` of the most recent process that had control
    pub fn most_recent_in_control(&self) -> Option<usize> {
        self.in_control_this_tick
    }

    /// The most recent pathing command executed
    pub fn most_recent_command(&self) -> Option<&PathingCommand> {
        self.command.as_ref()
    }

    pub fn pre_tick(baritone: &mut Baritone) {
        let this = &mut baritone.pathing_control_manager;
        this.in_control_last_tick = this.in_control_this_tick;
        this.in_control_this_tick = None;
        let command = Self::execute_processes(baritone);
        baritone.pathing_control_manager.command = command.clone();
        let Some(command) = command else {
            PathingBehavior::cancel_segment_if_safe(baritone);
            baritone.pathing_behavior.secret_internal_set_goal(None);
            return;
        };
        let this = &baritone.pathing_control_manager;
        if this.in_control_this_tick != this.in_control_last_tick
            && command.command_type != PathingCommandType::RequestPause
            && this
                .in_control_last_tick
                .is_some_and(|last| !baritone.process(last).is_temporary())
        {
            // if control has changed from a real process to another real process, and the new process wants to do something
            PathingBehavior::cancel_segment_if_safe(baritone);
            // get rid of the in progress stuff from the last process
        }
        match command.command_type {
            PathingCommandType::SetGoalAndPause => {
                PathingBehavior::secret_internal_set_goal_and_path(baritone, &command);
                baritone.pathing_behavior.request_pause();
            }
            PathingCommandType::RequestPause => {
                baritone.pathing_behavior.request_pause();
            }
            PathingCommandType::CancelAndSetGoal => {
                baritone
                    .pathing_behavior
                    .secret_internal_set_goal(command.goal.clone());
                PathingBehavior::cancel_segment_if_safe(baritone);
            }
            PathingCommandType::ForceRevalidateGoalAndPath
            | PathingCommandType::RevalidateGoalAndPath => {
                if !baritone.pathing_behavior.is_pathing()
                    && baritone.pathing_behavior.get_in_progress().is_none()
                {
                    PathingBehavior::secret_internal_set_goal_and_path(baritone, &command);
                }
            }
            PathingCommandType::SetGoalAndPath => {
                // now this i can do
                if command.goal.is_some() {
                    PathingBehavior::secret_internal_set_goal_and_path(baritone, &command);
                }
            }
            PathingCommandType::Defer => {
                panic!("Unexpected command type {}", command.command_type.name());
            }
        }
    }

    /// `postTick()`, from the tick listener upstream registers in the constructor.
    pub fn on_tick(baritone: &mut Baritone, event_type: tick_event::Type) {
        if event_type == tick_event::Type::In {
            Self::post_tick(baritone);
        }
    }

    fn post_tick(baritone: &mut Baritone) {
        // if we did this in pretick, it would suck
        // we use the time between ticks as calculation time
        // therefore, we only cancel and recalculate after the tick for the current path has executed
        // "it would suck" means it would actually execute a path every other tick
        let Some(command) = baritone.pathing_control_manager.command.clone() else {
            return;
        };
        match command.command_type {
            PathingCommandType::ForceRevalidateGoalAndPath => {
                if command.goal.as_ref().is_none_or(|goal| {
                    Self::force_revalidate(baritone, &**goal)
                        || Self::revalidate_goal(baritone, &**goal)
                }) {
                    // pwnage
                    baritone.pathing_behavior.soft_cancel_if_safe();
                }
                PathingBehavior::secret_internal_set_goal_and_path(baritone, &command);
            }
            PathingCommandType::RevalidateGoalAndPath => {
                if settings().cancel_on_goal_invalidation
                    && command
                        .goal
                        .as_ref()
                        .is_none_or(|goal| Self::revalidate_goal(baritone, &**goal))
                {
                    baritone.pathing_behavior.soft_cancel_if_safe();
                }
                PathingBehavior::secret_internal_set_goal_and_path(baritone, &command);
            }
            _ => {}
        }
    }

    pub fn force_revalidate(baritone: &Baritone, new_goal: &dyn Goal) -> bool {
        baritone.pathing_behavior.with_current(|current| {
            if let Some(current) = current {
                if new_goal.is_in_goal_pos(current.get_path().get_dest()) {
                    return false;
                }
                return !new_goal.equals(&**current.get_path().get_goal());
            }
            false
        })
    }

    pub fn revalidate_goal(baritone: &Baritone, new_goal: &dyn Goal) -> bool {
        baritone.pathing_behavior.with_current(|current| {
            if let Some(current) = current {
                let intended = current.get_path().get_goal();
                let end = current.get_path().get_dest();
                if intended.is_in_goal_pos(end) && !new_goal.is_in_goal_pos(end) {
                    // this path used to end in the goal
                    // but the goal has changed, so there's no reason to continue...
                    return true;
                }
            }
            false
        })
    }

    pub fn execute_processes(baritone: &mut Baritone) -> Option<PathingCommand> {
        for index in 0..baritone.processes.len() {
            let active =
                baritone.with_process(index, |process, baritone| process.is_active(baritone));
            let this = &mut baritone.pathing_control_manager;
            if active {
                if !this.active.contains(&index) {
                    // put a newly active process at the very front of the queue
                    this.active.insert(0, index);
                }
            } else {
                this.active.retain(|&p| p != index);
            }
        }
        // ties are broken by which was added to the beginning of the list first
        let mut active = std::mem::take(&mut baritone.pathing_control_manager.active);
        // Comparator.comparingDouble(priority).reversed(), a stable sort
        active.sort_by(|&a, &b| {
            double_compare(
                baritone.process(b).priority(),
                baritone.process(a).priority(),
            )
        });
        baritone.pathing_control_manager.active = active.clone();

        let mut iterator = active.into_iter();
        while let Some(proc) = iterator.next() {
            let calc_failed = baritone.pathing_control_manager.in_control_last_tick == Some(proc)
                && baritone.pathing_behavior.calc_failed_last_tick();
            let is_safe_to_cancel = baritone.pathing_behavior.is_safe_to_cancel();
            let exec = baritone.with_process(proc, |process, baritone| {
                process.on_tick(baritone, calc_failed, is_safe_to_cancel)
            });
            match exec {
                None => {
                    baritone.with_process(proc, |process, baritone| {
                        if process.is_active(baritone) {
                            panic!(
                                "{} actively returned null PathingCommand",
                                process.display_name(baritone)
                            );
                        }
                    });
                    // no need to call onLostControl; they are reporting inactive.
                }
                Some(exec) if exec.command_type != PathingCommandType::Defer => {
                    baritone.pathing_control_manager.in_control_this_tick = Some(proc);
                    if !baritone.process(proc).is_temporary() {
                        for remaining in iterator {
                            baritone.with_process(remaining, |process, baritone| {
                                process.on_lost_control(baritone)
                            });
                        }
                    }
                    return Some(exec);
                }
                Some(_) => {}
            }
        }
        None
    }
}

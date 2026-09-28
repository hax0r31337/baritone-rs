// Ported from baritone src/main/java/baritone/process/CustomGoalProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `ICustomGoalProcess.setGoalAndPath`. `setGoal`'s hand-off to the elytra process is left
// out: there is no elytra process, so it is never active.

use std::any::Any;
use std::sync::Arc;

use crate::Baritone;
use crate::api::pathing::goals::Goal;
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::IPlayerContext;
use crate::api::utils::helper::log_notification;
use crate::behavior::PathingBehavior;
use crate::settings::settings;

/// As set by ExampleBaritoneControl or something idk
#[derive(Debug, Default)]
pub struct CustomGoalProcess {
    /// The current goal
    goal: Option<Arc<dyn Goal>>,

    /// The most recent goal. Not invalidated upon [`IBaritoneProcess::on_lost_control`]
    most_recent_goal: Option<Arc<dyn Goal>>,

    /// The current process state.
    state: State,
}

/// The current process state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    None,
    GoalSet,
    PathRequested,
    Executing,
}

impl CustomGoalProcess {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the pathing goal
    pub fn set_goal(&mut self, goal: Option<Arc<dyn Goal>>) {
        self.goal = goal.clone();
        self.most_recent_goal = goal;
        if self.state == State::None {
            self.state = State::GoalSet;
        }
        if self.state == State::Executing {
            self.state = State::PathRequested;
        }
    }

    /// Starts path calculation and execution.
    pub fn path(&mut self) {
        self.state = State::PathRequested;
    }

    /// `ICustomGoalProcess.setGoalAndPath(Goal)`: sets the goal and begins the path execution.
    pub fn set_goal_and_path(&mut self, goal: Option<Arc<dyn Goal>>) {
        self.set_goal(goal);
        self.path();
    }

    /// Returns the current goal
    pub fn get_goal(&self) -> Option<&Arc<dyn Goal>> {
        self.goal.as_ref()
    }

    /// Returns the most recent set goal, which doesn't invalidate upon
    /// [`IBaritoneProcess::on_lost_control`]
    pub fn most_recent_goal(&self) -> Option<&Arc<dyn Goal>> {
        self.most_recent_goal.as_ref()
    }

    fn lost_control(&mut self) {
        self.state = State::None;
        self.goal = None;
    }
}

impl IBaritoneProcess for CustomGoalProcess {
    fn is_active(&self, _baritone: &Baritone) -> bool {
        self.state != State::None
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        calc_failed: bool,
        _is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        match self.state {
            State::GoalSet => Some(PathingCommand::new(
                self.goal.clone(),
                PathingCommandType::CancelAndSetGoal,
            )),
            State::PathRequested => {
                // return FORCE_REVALIDATE_GOAL_AND_PATH just once
                let ret = PathingCommand::new(
                    self.goal.clone(),
                    PathingCommandType::ForceRevalidateGoalAndPath,
                );
                self.state = State::Executing;
                Some(ret)
            }
            State::Executing => {
                if calc_failed {
                    self.lost_control();
                    return Some(PathingCommand::new(
                        self.goal.clone(),
                        PathingCommandType::CancelAndSetGoal,
                    ));
                }
                let ctx = &baritone.player_context;
                if self.goal.as_ref().is_none_or(|goal| {
                    goal.is_in_goal_pos(ctx.player_feet())
                        && goal.is_in_goal_pos(PathingBehavior::path_start(ctx))
                }) {
                    self.lost_control(); // we're there xd
                    let settings = settings();
                    if settings.disconnect_on_arrival {
                        baritone
                            .player_context
                            .player_controller()
                            .disconnect("[Baritone] Arrived at goal!");
                    }
                    if settings.notification_on_path_complete {
                        log_notification("Pathing complete", false);
                    }
                    return Some(PathingCommand::new(
                        self.goal.clone(),
                        PathingCommandType::CancelAndSetGoal,
                    ));
                }
                Some(PathingCommand::new(
                    self.goal.clone(),
                    PathingCommandType::SetGoalAndPath,
                ))
            }
            State::None => panic!("Unexpected state {:?}", self.state),
        }
    }

    fn is_temporary(&self) -> bool {
        false
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {
        self.lost_control();
    }

    fn display_name0(&self) -> String {
        match &self.goal {
            Some(goal) => format!("Custom Goal {goal}"),
            None => "Custom Goal null".to_owned(),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// Ported from baritone src/api/java/baritone/api/process/PathingCommandType.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathingCommandType {
    /// Set the goal and path.
    ///
    /// If you use this alongside a `None` goal, it will continue along its current path and current goal.
    SetGoalAndPath,

    /// Has no effect on the current goal or path, just requests a pause
    RequestPause,

    /// Set the goal (regardless of `None`), and request a cancel of the current path (when safe)
    CancelAndSetGoal,

    /// Set the goal and path.
    ///
    /// If `Settings.cancelOnGoalInvalidation` is `true`, revalidate the
    /// current goal, and cancel if it's no longer valid, or if the new goal is `None`.
    RevalidateGoalAndPath,

    /// Set the goal and path.
    ///
    /// Cancel the current path if the goals are not equal
    ForceRevalidateGoalAndPath,

    /// Go and ask the next process what to do
    Defer,

    /// Sets the goal and calculates a path, but pauses instead of immediately starting the path.
    SetGoalAndPause,
}

impl PathingCommandType {
    /// The upstream constant name (`"SET_GOAL_AND_PATH"`).
    pub fn name(self) -> &'static str {
        match self {
            PathingCommandType::SetGoalAndPath => "SET_GOAL_AND_PATH",
            PathingCommandType::RequestPause => "REQUEST_PAUSE",
            PathingCommandType::CancelAndSetGoal => "CANCEL_AND_SET_GOAL",
            PathingCommandType::RevalidateGoalAndPath => "REVALIDATE_GOAL_AND_PATH",
            PathingCommandType::ForceRevalidateGoalAndPath => "FORCE_REVALIDATE_GOAL_AND_PATH",
            PathingCommandType::Defer => "DEFER",
            PathingCommandType::SetGoalAndPause => "SET_GOAL_AND_PAUSE",
        }
    }
}

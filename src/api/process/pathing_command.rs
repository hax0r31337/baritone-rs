// Ported from baritone src/api/java/baritone/api/process/PathingCommand.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `baritone.utils.PathingCommandContext`, the subclass that carries a calculation context:
// a command is one when `desired_calc_context` is set (`instanceof PathingCommandContext`).

use std::fmt;
use std::sync::Arc;

use crate::api::pathing::goals::Goal;
use crate::api::process::PathingCommandType;
use crate::pathing::movement::CalculationContext;

#[derive(Clone)]
pub struct PathingCommand {
    /// The target goal, may be `None`.
    pub goal: Option<Arc<dyn Goal>>,

    /// The command type.
    pub command_type: PathingCommandType,

    /// `PathingCommandContext.desiredCalcContext`
    pub desired_calc_context: Option<CalculationContext>,
}

impl PathingCommand {
    /// Create a new [`PathingCommand`].
    pub fn new(goal: Option<Arc<dyn Goal>>, command_type: PathingCommandType) -> Self {
        Self {
            goal,
            command_type,
            desired_calc_context: None,
        }
    }

    /// `new PathingCommandContext(Goal, PathingCommandType, CalculationContext)`
    pub fn with_context(
        goal: Option<Arc<dyn Goal>>,
        command_type: PathingCommandType,
        context: CalculationContext,
    ) -> Self {
        Self {
            goal,
            command_type,
            desired_calc_context: Some(context),
        }
    }
}

impl fmt::Display for PathingCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.goal {
            Some(goal) => write!(f, "{} {}", self.command_type.name(), goal),
            None => write!(f, "{} null", self.command_type.name()),
        }
    }
}

impl fmt::Debug for PathingCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

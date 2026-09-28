// Ported from baritone src/api/java/baritone/api/event/events/PathEvent.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathEvent {
    CalcStarted,
    CalcFinishedNowExecuting,
    CalcFailed,
    NextSegmentCalcStarted,
    NextSegmentCalcFinished,
    ContinuingOntoPlannedNext,
    SplicingOntoNextEarly,
    AtGoal,
    PathFinishedNextStillCalculating,
    NextCalcFailed,
    DiscardNext,
    Canceled,
}

impl PathEvent {
    /// The upstream constant name (`"CALC_STARTED"`).
    pub fn name(self) -> &'static str {
        match self {
            PathEvent::CalcStarted => "CALC_STARTED",
            PathEvent::CalcFinishedNowExecuting => "CALC_FINISHED_NOW_EXECUTING",
            PathEvent::CalcFailed => "CALC_FAILED",
            PathEvent::NextSegmentCalcStarted => "NEXT_SEGMENT_CALC_STARTED",
            PathEvent::NextSegmentCalcFinished => "NEXT_SEGMENT_CALC_FINISHED",
            PathEvent::ContinuingOntoPlannedNext => "CONTINUING_ONTO_PLANNED_NEXT",
            PathEvent::SplicingOntoNextEarly => "SPLICING_ONTO_NEXT_EARLY",
            PathEvent::AtGoal => "AT_GOAL",
            PathEvent::PathFinishedNextStillCalculating => "PATH_FINISHED_NEXT_STILL_CALCULATING",
            PathEvent::NextCalcFailed => "NEXT_CALC_FAILED",
            PathEvent::DiscardNext => "DISCARD_NEXT",
            PathEvent::Canceled => "CANCELED",
        }
    }
}

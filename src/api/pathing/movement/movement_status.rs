// Ported from baritone src/api/java/baritone/api/pathing/movement/MovementStatus.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MovementStatus {
    /// We are preparing the movement to be executed. This is when any blocks obstructing the destination are broken.
    Prepping,

    /// We are waiting for the movement to begin, after [`MovementStatus::Prepping`].
    Waiting,

    /// The movement is currently in progress, after [`MovementStatus::Waiting`]
    Running,

    /// The movement has been completed and we are at our destination
    Success,

    /// There was a change in state between calculation and actual
    /// movement execution, and the movement has now become impossible.
    Unreachable,

    /// Unused
    Failed,

    /// "Unused"
    Canceled,
}

impl MovementStatus {
    /// Whether or not this status indicates a complete movement.
    pub fn is_complete(self) -> bool {
        matches!(
            self,
            MovementStatus::Success
                | MovementStatus::Unreachable
                | MovementStatus::Failed
                | MovementStatus::Canceled
        )
    }

    /// The upstream constant name (`"PREPPING"`).
    pub fn name(self) -> &'static str {
        match self {
            MovementStatus::Prepping => "PREPPING",
            MovementStatus::Waiting => "WAITING",
            MovementStatus::Running => "RUNNING",
            MovementStatus::Success => "SUCCESS",
            MovementStatus::Unreachable => "UNREACHABLE",
            MovementStatus::Failed => "FAILED",
            MovementStatus::Canceled => "CANCELED",
        }
    }
}

// Ported from baritone src/api/java/baritone/api/utils/input/Input.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

/// An enum representing the inputs that control the player's
/// behavior. This includes moving, interacting with blocks, jumping,
/// sneaking, and sprinting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Input {
    /// The move forward input
    MoveForward,

    /// The move back input
    MoveBack,

    /// The move left input
    MoveLeft,

    /// The move right input
    MoveRight,

    /// The attack input
    ClickLeft,

    /// The use item input
    ClickRight,

    /// The jump input
    Jump,

    /// The sneak input
    Sneak,

    /// The sprint input
    Sprint,
}

impl Input {
    /// `Input.values()`
    pub const VALUES: [Input; 9] = [
        Input::MoveForward,
        Input::MoveBack,
        Input::MoveLeft,
        Input::MoveRight,
        Input::ClickLeft,
        Input::ClickRight,
        Input::Jump,
        Input::Sneak,
        Input::Sprint,
    ];

    /// The upstream constant name (`"MOVE_FORWARD"`).
    pub fn name(self) -> &'static str {
        match self {
            Input::MoveForward => "MOVE_FORWARD",
            Input::MoveBack => "MOVE_BACK",
            Input::MoveLeft => "MOVE_LEFT",
            Input::MoveRight => "MOVE_RIGHT",
            Input::ClickLeft => "CLICK_LEFT",
            Input::ClickRight => "CLICK_RIGHT",
            Input::Jump => "JUMP",
            Input::Sneak => "SNEAK",
            Input::Sprint => "SPRINT",
        }
    }
}

// Ported from baritone src/api/java/baritone/api/event/events/TickEvent.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only the type is ported: no kept listener reads the state or the tick count.

/// `TickEvent.Type`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// When guarantees can be made about
    /// the game state and in-game variables.
    In,
    /// No guarantees can be made about the game state.
    /// This probably means we are at the main menu.
    Out,
}

// Ported from baritone src/api/java/baritone/api/event/events/type/EventState.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventState {
    /// Before the dispatching of what the event is targetting
    Pre,

    /// After the dispatching of what the event is targetting
    Post,
}

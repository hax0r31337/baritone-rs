// Ported from baritone src/main/java/baritone/pathing/precompute/Ternary.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ternary {
    Yes,
    Maybe,
    No,
}

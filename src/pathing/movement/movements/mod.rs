//! The `Movement` subclasses. Their constructors (`new`) build the `Movement` that wraps them.

#![allow(clippy::new_ret_no_self)]

pub mod movement_ascend;
pub mod movement_descend;
pub mod movement_diagonal;
pub mod movement_downward;
pub mod movement_fall;
pub mod movement_parkour;
pub mod movement_pillar;
pub mod movement_traverse;

pub use movement_ascend::MovementAscend;
pub use movement_descend::MovementDescend;
pub use movement_diagonal::MovementDiagonal;
pub use movement_downward::MovementDownward;
pub use movement_fall::MovementFall;
pub use movement_parkour::MovementParkour;
pub use movement_pillar::MovementPillar;
pub use movement_traverse::MovementTraverse;

use crate::host::{BlockState, Climbable, SpeedKind};

// Block identity checks (`state.getBlock() == Blocks.X`, `state.is(Blocks.X)`) that several
// movements share; docs/trait-mapping.md lists them.

/// `Blocks.SOUL_SAND`
#[inline]
pub(crate) fn is_soul_sand(state: &BlockState) -> bool {
    state.speed_kind == Some(SpeedKind::SoulSand)
}

/// `Blocks.MAGMA_BLOCK`
#[inline]
pub(crate) fn is_magma(state: &BlockState) -> bool {
    state.hot_floor
}

/// `Blocks.WATER`: a pure water block, not waterlogged.
#[inline]
pub(crate) fn is_water_block(state: &BlockState) -> bool {
    state.liquid_block && state.own_fluid().is_water()
}

/// `Blocks.LADDER || Blocks.VINE` (not the nether vines)
#[inline]
pub(crate) fn is_ladder_or_vine(state: &BlockState) -> bool {
    matches!(state.climbable, Some(Climbable::Ladder | Climbable::Vine))
}

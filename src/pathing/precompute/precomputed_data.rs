// Ported from baritone src/main/java/baritone/pathing/precompute/PrecomputedData.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Sized by the host block state table instead of `Block.BLOCK_STATE_REGISTRY`, with a second
// entry per state on Bedrock for its waterlogged tri-states. The Java `byte[]` is written
// racily by design ("every thread should compute the exact same int"); the port uses relaxed
// atomics for the same effect.

use std::sync::atomic::{AtomicU8, Ordering};

use crate::host::{BlockState, BlockStateTable, FluidState};
use crate::pathing::movement::movement_helper;
use crate::pathing::precompute::Ternary;
use crate::utils::BlockStateInterface;

// byte layout
//
//          7              6             5              4              3              2              1             0
//          |              |             |              |              |              |              |             |
//      unused         canWalkOn       maybe       canWalkThrough    maybe        fullyPassable    maybe       completed

const COMPLETED_MASK: u8 = 1 << 0;
const FULLY_PASSABLE_MAYBE_MASK: u8 = 1 << 1;
const FULLY_PASSABLE_MASK: u8 = 1 << 2;
const CAN_WALK_THROUGH_MAYBE_MASK: u8 = 1 << 3;
const CAN_WALK_THROUGH_MASK: u8 = 1 << 4;
const CAN_WALK_ON_MAYBE_MASK: u8 = 1 << 5;
const CAN_WALK_ON_MASK: u8 = 1 << 6;

/// Entries per state: with the `bedrock` feature a second one for the state with water in the
/// liquid layer ([`BlockState::waterlogged`]).
const SLOTS: usize = edition!(1, 2);

/// Per-state tri-states, filled in on first use with the settings active at that time.
pub struct PrecomputedData {
    data: Box<[AtomicU8]>,
}

impl PrecomputedData {
    /// For states of `table` (the table of the worlds it will be used with).
    pub fn new(table: &BlockStateTable) -> Self {
        Self {
            data: (0..table.len() * SLOTS).map(|_| AtomicU8::new(0)).collect(),
        }
    }

    fn fill_data(&self, slot: usize, state: &BlockState, fluid: FluidState) -> u8 {
        let mut block_data = 0;

        let can_walk_on_state = movement_helper::can_walk_on_block_state(state, fluid);
        match can_walk_on_state {
            Ternary::Yes => block_data |= CAN_WALK_ON_MASK,
            Ternary::Maybe => block_data |= CAN_WALK_ON_MAYBE_MASK,
            Ternary::No => {}
        }

        let can_walk_through_state = movement_helper::can_walk_through_block_state(state, fluid);
        match can_walk_through_state {
            Ternary::Yes => block_data |= CAN_WALK_THROUGH_MASK,
            Ternary::Maybe => block_data |= CAN_WALK_THROUGH_MAYBE_MASK,
            Ternary::No => {}
        }

        let fully_passable_state = movement_helper::fully_passable_block_state(state, fluid);
        match fully_passable_state {
            Ternary::Yes => block_data |= FULLY_PASSABLE_MASK,
            Ternary::Maybe => block_data |= FULLY_PASSABLE_MAYBE_MASK,
            Ternary::No => {}
        }

        block_data |= COMPLETED_MASK;

        // in theory, this is thread "safe" because every thread should compute the exact same int to write?
        self.data[slot].store(block_data, Ordering::Relaxed);
        block_data
    }

    /// The tri-states of `state` at `x, y, z`. The state's entry only depends on whether the
    /// fluid there is its own (the liquid layer only holds water).
    #[inline]
    fn block_data(
        &self,
        bsi: &BlockStateInterface,
        x: i32,
        y: i32,
        z: i32,
        state: &BlockState,
    ) -> u8 {
        let id = state.id as usize;
        let (slot, fluid) = if cfg!(feature = "bedrock") && state.waterlogged.is_some() {
            let fluid = bsi.fluid_in(x, y, z, state);
            let slot = id * SLOTS + state.waterlogged_by(fluid).is_some() as usize;
            (slot, fluid)
        } else {
            (id * SLOTS, state.own_fluid())
        };
        let block_data = self.data[slot].load(Ordering::Relaxed);

        if block_data & COMPLETED_MASK == 0 {
            // we need to fill in the data
            return self.fill_data(slot, state, fluid);
        }
        block_data
    }

    pub fn can_walk_on(
        &self,
        bsi: &BlockStateInterface,
        x: i32,
        y: i32,
        z: i32,
        state: &BlockState,
    ) -> bool {
        let block_data = self.block_data(bsi, x, y, z, state);

        if block_data & CAN_WALK_ON_MAYBE_MASK != 0 {
            movement_helper::can_walk_on_position(bsi, x, y, z, state)
        } else {
            block_data & CAN_WALK_ON_MASK != 0
        }
    }

    pub fn can_walk_through(
        &self,
        bsi: &BlockStateInterface,
        x: i32,
        y: i32,
        z: i32,
        state: &BlockState,
    ) -> bool {
        let block_data = self.block_data(bsi, x, y, z, state);

        if block_data & CAN_WALK_THROUGH_MAYBE_MASK != 0 {
            movement_helper::can_walk_through_position(bsi, x, y, z, state)
        } else {
            block_data & CAN_WALK_THROUGH_MASK != 0
        }
    }

    pub fn fully_passable(
        &self,
        bsi: &BlockStateInterface,
        x: i32,
        y: i32,
        z: i32,
        state: &BlockState,
    ) -> bool {
        let block_data = self.block_data(bsi, x, y, z, state);

        if block_data & FULLY_PASSABLE_MAYBE_MASK != 0 {
            movement_helper::fully_passable_position(bsi, x, y, z, state)
        } else {
            block_data & FULLY_PASSABLE_MASK != 0
        }
    }
}

#[cfg(all(test, feature = "bedrock"))]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::host::world::tests::fluid_table;
    use crate::host::{Chunk, DimensionType, World};

    const STONE: u32 = 1;
    const FENCE: u32 = 2;
    const WATER: u32 = 3;

    #[test]
    fn waterlogged_blocks_use_their_waterlogged_tri_states() {
        let mut world = World::new(
            fluid_table(),
            DimensionType {
                min_y: 0,
                height: 16,
                water_evaporates: false,
            },
        )
        .unwrap();
        world.load_chunk(0, 0, Chunk::new(1)).unwrap();
        for x in 0..8 {
            world.set_block(x, 0, 0, STONE).unwrap();
        }
        // a waterlogged fence under air, a dry one, a waterlogged one under water
        world.set_block(1, 1, 0, FENCE).unwrap();
        world.set_liquid(1, 1, 0, WATER).unwrap();
        world.set_block(3, 1, 0, FENCE).unwrap();
        world.set_block(5, 1, 0, FENCE).unwrap();
        world.set_liquid(5, 1, 0, WATER).unwrap();
        world.set_block(5, 2, 0, WATER).unwrap();

        let bsi = BlockStateInterface::new(Arc::new(world));
        let pd = PrecomputedData::new(bsi.table());
        let fence = bsi.table().get(FENCE);
        // twice, so the second round reads the filled entries
        for _ in 0..2 {
            // still water you can stand in: MAYBE, and the position check says yes
            assert!(pd.can_walk_through(&bsi, 1, 1, 0, fence));
            assert!(!pd.can_walk_through(&bsi, 3, 1, 0, fence));
            // water under water can be walked on
            assert!(pd.can_walk_on(&bsi, 5, 1, 0, fence));
            assert!(!pd.can_walk_on(&bsi, 3, 1, 0, fence));
            assert!(!pd.fully_passable(&bsi, 1, 1, 0, fence));
        }
    }
}

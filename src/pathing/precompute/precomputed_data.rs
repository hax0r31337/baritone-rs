// Ported from baritone src/main/java/baritone/pathing/precompute/PrecomputedData.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Sized by the host block state table instead of `Block.BLOCK_STATE_REGISTRY`. The Java
// `byte[]` is written racily by design ("every thread should compute the exact same int");
// the port uses relaxed atomics for the same effect.

use std::sync::atomic::{AtomicU8, Ordering};

use crate::host::{BlockState, BlockStateTable};
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

/// Per-state tri-states, filled in on first use with the settings active at that time.
pub struct PrecomputedData {
    data: Box<[AtomicU8]>,
}

impl PrecomputedData {
    /// For states of `table` (the table of the worlds it will be used with).
    pub fn new(table: &BlockStateTable) -> Self {
        Self {
            data: (0..table.len()).map(|_| AtomicU8::new(0)).collect(),
        }
    }

    fn fill_data(&self, id: usize, state: &BlockState) -> u8 {
        let mut block_data = 0;

        let can_walk_on_state = movement_helper::can_walk_on_block_state(state);
        match can_walk_on_state {
            Ternary::Yes => block_data |= CAN_WALK_ON_MASK,
            Ternary::Maybe => block_data |= CAN_WALK_ON_MAYBE_MASK,
            Ternary::No => {}
        }

        let can_walk_through_state = movement_helper::can_walk_through_block_state(state);
        match can_walk_through_state {
            Ternary::Yes => block_data |= CAN_WALK_THROUGH_MASK,
            Ternary::Maybe => block_data |= CAN_WALK_THROUGH_MAYBE_MASK,
            Ternary::No => {}
        }

        let fully_passable_state = movement_helper::fully_passable_block_state(state);
        match fully_passable_state {
            Ternary::Yes => block_data |= FULLY_PASSABLE_MASK,
            Ternary::Maybe => block_data |= FULLY_PASSABLE_MAYBE_MASK,
            Ternary::No => {}
        }

        block_data |= COMPLETED_MASK;

        // in theory, this is thread "safe" because every thread should compute the exact same int to write?
        self.data[id].store(block_data, Ordering::Relaxed);
        block_data
    }

    #[inline]
    fn block_data(&self, state: &BlockState) -> u8 {
        let id = state.id as usize;
        let block_data = self.data[id].load(Ordering::Relaxed);

        if block_data & COMPLETED_MASK == 0 {
            // we need to fill in the data
            return self.fill_data(id, state);
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
        let block_data = self.block_data(state);

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
        let block_data = self.block_data(state);

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
        let block_data = self.block_data(state);

        if block_data & FULLY_PASSABLE_MAYBE_MASK != 0 {
            movement_helper::fully_passable_position(bsi, x, y, z, state)
        } else {
            block_data & FULLY_PASSABLE_MASK != 0
        }
    }
}

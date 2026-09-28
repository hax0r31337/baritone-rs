// Ported from baritone src/main/java/baritone/pathing/movement/MovementHelper.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Phase 2 ports the block checks that need only a BlockStateInterface. Missing until phase 3:
// the CalculationContext overloads (canWalkThrough, canWalkOn, fullyPassable),
// canUseFrostWalker(context), mustBeSolidToWalkOn, getMiningDurationTicks. Missing until phase 4:
// the IPlayerContext overloads, isDoorPassable, isGatePassable, isHorizontalBlockPassable,
// canUseFrostWalker(ctx), switchToBestToolFor, the movement input helpers, attemptToPlaceABlock,
// steppingOnBlocks.
//
// Block identity checks read host traits; docs/trait-mapping.md lists which trait replaces each
// one. The three `*BlockState` functions start from the tri-states the host computed with
// upstream's default settings and apply the settings that differ.

use crate::api::utils::BetterBlockPos;
use crate::host::{BlockState, Climbable, Fluid, SlabType};
use crate::pathing::precompute::Ternary;
use crate::settings::settings;
use crate::utils::BlockStateInterface;

pub fn avoid_breaking(
    bsi: &BlockStateInterface,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    if !bsi.world_border.can_place_at(x, z) {
        return true;
    }
    settings().blocks_to_disallow_breaking.contains(&state.name)
        || state.avoid_breaking // ice becomes water, and water can mess up the path; infested: obvious reasons
        // call context.get directly with x,y,z. no need to make 5 new BlockPos for no reason
        || avoid_adjacent_breaking(bsi, x, y.wrapping_add(1), z, true)
        || avoid_adjacent_breaking(bsi, x.wrapping_add(1), y, z, false)
        || avoid_adjacent_breaking(bsi, x.wrapping_sub(1), y, z, false)
        || avoid_adjacent_breaking(bsi, x, y, z.wrapping_add(1), false)
        || avoid_adjacent_breaking(bsi, x, y, z.wrapping_sub(1), false)
}

pub fn avoid_adjacent_breaking(
    bsi: &BlockStateInterface,
    x: i32,
    y: i32,
    z: i32,
    directly_above: bool,
) -> bool {
    // returns true if you should avoid breaking a block that's adjacent to this one (e.g. lava that will start flowing if you give it a path)
    // this is only called for north, south, east, west, and up. this is NOT called for down.
    // we assume that it's ALWAYS okay to break the block thats ABOVE liquid
    let state = bsi.get0(x, y, z);
    if !directly_above // it is fine to mine a block that has a falling block directly above, this (the cost of breaking the stacked fallings) is included in cost calculations
        // therefore if directlyAbove is true, we will actually ignore if this is falling
        && state.falls // obviously, this check is only valid for falling blocks
        && settings().avoid_updating_falling_blocks // and if the setting is enabled
        && falling_block_is_free(bsi.get0(x, y.wrapping_sub(1), z))
    {
        // and if it would fall (i.e. it's unsupported)
        return true; // dont break a block that is adjacent to unsupported gravel because it can cause really weird stuff
    }
    // only pure liquids for now
    // waterlogged blocks can have closed bottom sides and such
    if state.liquid_block {
        if directly_above || settings().strict_liquid_check {
            return true;
        }
        // LiquidBlock.LEVEL == 0
        if state.fluid_source {
            return true; // source blocks like to flow horizontally
        }
        // everything else will prefer flowing down
        return !bsi.get0(x, y.wrapping_sub(1), z).liquid_block; // assume everything is in a static state
    }
    state.fluid != Fluid::Empty
}

/// `FallingBlock.isFree(BlockState)`, a Minecraft method: a falling block would fall into this.
pub fn falling_block_is_free(state: &BlockState) -> bool {
    state.air || state.fire || state.liquid || state.replaceable
}

/// `canWalkThrough(BlockStateInterface, int, int, int)`
pub fn can_walk_through_bsi(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
    can_walk_through_bsi_state(bsi, x, y, z, bsi.get0(x, y, z))
}

/// `canWalkThrough(BlockStateInterface, int, int, int, BlockState)`
pub fn can_walk_through_bsi_state(
    bsi: &BlockStateInterface,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    let can_walk_through = can_walk_through_block_state(state);
    if can_walk_through == Ternary::Yes {
        return true;
    }
    if can_walk_through == Ternary::No {
        return false;
    }
    can_walk_through_position(bsi, x, y, z, state)
}

pub fn can_walk_through_block_state(state: &BlockState) -> Ternary {
    // Upstream returns YES for air first, then NO for a list of blocks, then NO for
    // blocksToAvoid. The host's tri-state has everything but blocksToAvoid.
    if !state.air && settings().blocks_to_avoid.contains(&state.name) {
        return Ternary::No;
    }
    state.can_walk_through
}

pub fn can_walk_through_position(
    bsi: &BlockStateInterface,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    if state.carpet {
        return can_walk_on_bsi(bsi, x, y.wrapping_sub(1), z);
    }

    if state.snow_layers > 0 {
        // if they're cached as a top block, we don't know their metadata
        // default to true (mostly because it would otherwise make long distance pathing through snowy biomes impossible)
        if !bsi.world_contains_loaded_chunk(x, z) {
            return true;
        }
        // the check in BlockSnow.isPassable is layers < 5
        // while actually, we want < 3 because 3 or greater makes it impassable in a 2 high ceiling
        if state.snow_layers >= 3 {
            return false;
        }
        // ok, it's low enough we could walk through it, but is it supported?
        return can_walk_on_bsi(bsi, x, y.wrapping_sub(1), z);
    }

    if state.fluid != Fluid::Empty {
        if is_flowing(x, y, z, state, bsi) {
            return false;
        }
        // Everything after this point has to be a special case as it relies on the water not being flowing, which means a special case is needed.
        if settings().assume_walk_on_water {
            return false;
        }

        let up = bsi.get0(x, y.wrapping_add(1), z);
        if up.fluid != Fluid::Empty || up.lily_pad {
            return false;
        }
        return state.fluid == Fluid::Water;
    }

    state.pathfindable_land
}

pub fn fully_passable_block_state(state: &BlockState) -> Ternary {
    // no settings involved, the host's tri-state is upstream's
    state.fully_passable
}

/// params retained for backwards compatibility
pub fn fully_passable_position(
    _bsi: &BlockStateInterface,
    _x: i32,
    _y: i32,
    _z: i32,
    state: &BlockState,
) -> bool {
    state.pathfindable_land
}

pub fn is_replaceable(
    x: i32,
    _y: i32,
    z: i32,
    state: &BlockState,
    bsi: &BlockStateInterface,
) -> bool {
    // for MovementTraverse and MovementAscend
    // block double plant defaults to true when the block doesn't match, so don't need to check that case
    // all other overrides just return true or false
    // the only case to deal with is snow
    /*
     *  public boolean isReplaceable(IBlockAccess worldIn, BlockPos pos)
     *     {
     *         return ((Integer)worldIn.getBlockState(pos).getValue(LAYERS)).intValue() == 1;
     *     }
     */
    if state.air {
        // early return for common cases hehe
        return true;
    }
    if state.snow_layers > 0 {
        // as before, default to true (mostly because it would otherwise make long distance pathing through snowy biomes impossible)
        if !bsi.world_contains_loaded_chunk(x, z) {
            return true;
        }
        return state.snow_layers == 1;
    }
    // upstream returns true for LARGE_FERN and TALL_GRASS here; `replaceable` already is
    state.replaceable
}

pub fn avoid_walking_into(state: &BlockState) -> bool {
    state.fluid != Fluid::Empty
        || (state.hot_floor && !settings().allow_walk_on_magma_blocks)
        || state.avoid_walking_into
}

/// Can I walk on this block without anything weird happening like me falling
/// through? Includes water because we know that we automatically jump on
/// water
///
/// If changing something in this function remember to also change it in precomputed data
///
/// `canWalkOn(BlockStateInterface, int, int, int, BlockState)`
pub fn can_walk_on_bsi_state(
    bsi: &BlockStateInterface,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    let can_walk_on = can_walk_on_block_state(state);
    if can_walk_on == Ternary::Yes {
        return true;
    }
    if can_walk_on == Ternary::No {
        return false;
    }
    can_walk_on_position(bsi, x, y, z, state)
}

pub fn can_walk_on_block_state(state: &BlockState) -> Ternary {
    let settings = settings();
    // The host's tri-state has allowWalkOnMagmaBlocks, allowVines and assumeWalkOnLava off and
    // allowWalkOnBottomSlab on. Upstream's checks, in order, where these settings matter:
    //   isBlockNormalCube && (not magma || allowWalkOnMagmaBlocks) && ... -> YES
    if state.hot_floor && state.normal_cube && settings.allow_walk_on_magma_blocks {
        return Ternary::Yes;
    }
    //   LADDER || (isClimbable && allowVines) -> YES
    if matches!(
        state.climbable,
        Some(Climbable::Vine | Climbable::NetherVine)
    ) && settings.allow_vines
    {
        return Ternary::Yes;
    }
    //   isWater -> MAYBE, then isLava && assumeWalkOnLava -> MAYBE (the host said NO)
    if is_lava(state) && settings.assume_walk_on_lava && state.can_walk_on == Ternary::No {
        return Ternary::Maybe;
    }
    //   SlabBlock: !allowWalkOnBottomSlab && BOTTOM -> NO (a waterlogged slab stopped at isWater)
    if is_bottom_slab(state) && state.fluid == Fluid::Empty && !settings.allow_walk_on_bottom_slab {
        return Ternary::No;
    }
    state.can_walk_on
}

pub fn can_walk_on_position(
    bsi: &BlockStateInterface,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    if is_water(state) {
        // since this is called literally millions of times per second, the benefit of not allocating millions of useless "pos.up()"
        // BlockPos s that we'd just garbage collect immediately is actually noticeable. I don't even think its a decrease in readability
        let up_state = bsi.get0(x, y.wrapping_add(1), z);
        if up_state.lily_pad || up_state.carpet {
            return true;
        }
        // Fluids.FLOWING_WATER
        if is_flowing(x, y, z, state, bsi)
            || (up_state.fluid == Fluid::Water && !up_state.fluid_source)
        {
            // the only scenario in which we can walk on flowing water is if it's under still water with jesus off
            return is_water(up_state) && !settings().assume_walk_on_water;
        }
        // if assumeWalkOnWater is on, we can only walk on water if there isn't water above it
        // if assumeWalkOnWater is off, we can only walk on water if there is water above it
        return is_water(up_state) ^ settings().assume_walk_on_water;
    }

    if is_lava(state) && !is_flowing(x, y, z, state, bsi) && settings().assume_walk_on_lava {
        // if we get here it means that assumeWalkOnLava must be true, so put it last
        return true;
    }

    false // If we don't recognise it then we want to just return false to be safe.
}

/// `canWalkOn(BlockStateInterface, int, int, int)`
pub fn can_walk_on_bsi(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
    can_walk_on_bsi_state(bsi, x, y, z, bsi.get0(x, y, z))
}

pub fn can_place_against(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
    can_place_against_state(bsi, x, y, z, bsi.get0(x, y, z))
}

/// `canPlaceAgainst(BlockStateInterface, BlockPos)`
pub fn can_place_against_pos(bsi: &BlockStateInterface, pos: BetterBlockPos) -> bool {
    can_place_against(bsi, pos.x, pos.y, pos.z)
}

/// `canPlaceAgainst(BlockStateInterface, int, int, int, BlockState)`
pub fn can_place_against_state(
    bsi: &BlockStateInterface,
    x: i32,
    _y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    if !bsi.world_border.can_place_at(x, z) {
        return false;
    }
    // can we look at the center of a side face of this block and likely be able to place?
    // (thats how this check is used)
    // therefore dont include weird things that we technically could place against (like carpet) but practically can't
    state.can_place_against
}

/// Can we climb up this block by pressing space while inside it?
/// Also doubles as "If I start a movement on this, can weird things happen?"
/// because movements can end/start on these blocks despite them not being canWalkOn.
///
/// Upstream takes the `Block`.
pub fn is_climbable(state: &BlockState) -> bool {
    matches!(
        state.climbable,
        Some(Climbable::Ladder | Climbable::Vine | Climbable::NetherVine)
    )
}

pub fn is_bottom_slab(state: &BlockState) -> bool {
    state.slab == Some(SlabType::Bottom)
}

/// Returns whether or not the specified block is
/// water, regardless of whether or not it is flowing.
pub fn is_water(state: &BlockState) -> bool {
    state.fluid == Fluid::Water
}

pub fn is_lava(state: &BlockState) -> bool {
    state.fluid == Fluid::Lava
}

pub fn is_liquid(block_state: &BlockState) -> bool {
    block_state.fluid != Fluid::Empty
}

pub fn possibly_flowing(state: &BlockState) -> bool {
    // FlowingFluid: water and lava, not the empty fluid
    state.fluid != Fluid::Empty && state.fluid_amount != 8
}

pub fn is_flowing(x: i32, y: i32, z: i32, state: &BlockState, bsi: &BlockStateInterface) -> bool {
    if state.fluid == Fluid::Empty {
        return false;
    }
    if state.fluid_amount != 8 {
        return true;
    }
    possibly_flowing(bsi.get0(x.wrapping_add(1), y, z))
        || possibly_flowing(bsi.get0(x.wrapping_sub(1), y, z))
        || possibly_flowing(bsi.get0(x, y, z.wrapping_add(1)))
        || possibly_flowing(bsi.get0(x, y, z.wrapping_sub(1)))
}

pub fn is_block_normal_cube(state: &BlockState) -> bool {
    state.normal_cube
}

/// Upstream takes the `Block`: air, `Blocks.LAVA`, `Blocks.WATER`.
pub fn is_transparent(b: &BlockState) -> bool {
    b.air || b.liquid_block
}

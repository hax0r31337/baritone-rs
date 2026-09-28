// Ported from baritone src/main/java/baritone/pathing/movement/MovementHelper.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Block identity checks read host traits; docs/trait-mapping.md lists which trait replaces each
// one. The three `*BlockState` functions start from the tri-states the host computed with
// upstream's default settings and apply the settings that differ.
//
// `attemptToPlaceABlock` takes the `Baritone` (upstream: `IBaritone`), which holds the player
// context. `isHorizontalBlockPassable` takes the open state instead of the property to read it
// from.

use std::sync::Arc;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::helper::log_debug;
use crate::api::utils::input::Input;
use crate::api::utils::rotation_utils::{self, DEG_TO_RAD_F};
use crate::api::utils::{BetterBlockPos, IPlayerContext, Rotation, ray_trace_utils, vec_utils};
use crate::behavior::InventoryBehavior;
use crate::host::{BlockState, Climbable, Fluid, Half, Openable, SlabType};
use crate::mc::{Axis, HitResultType, Vec3, mth};
use crate::pathing::movement::movement::HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP;
use crate::pathing::movement::movement_state::MovementTarget;
use crate::pathing::movement::{CalculationContext, MovementOption, MovementState};
use crate::pathing::precompute::Ternary;
use crate::settings::settings;
use crate::utils::{BlockStateInterface, ToolSet};

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

/// `canWalkThrough(IPlayerContext, BetterBlockPos)`
pub fn can_walk_through_ctx(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> bool {
    can_walk_through_bsi(&BlockStateInterface::from_ctx(ctx), pos.x, pos.y, pos.z)
}

/// `canWalkThrough(BlockStateInterface, int, int, int)`
pub fn can_walk_through_bsi(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
    can_walk_through_bsi_state(bsi, x, y, z, bsi.get0(x, y, z))
}

/// `canWalkThrough(CalculationContext, int, int, int, BlockState)`
#[inline]
pub fn can_walk_through_state(
    context: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    context
        .precomputed_data
        .can_walk_through(&context.bsi, x, y, z, state)
}

/// `canWalkThrough(CalculationContext, int, int, int)`
#[inline]
pub fn can_walk_through(context: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    context
        .precomputed_data
        .can_walk_through(&context.bsi, x, y, z, context.get(x, y, z))
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

/// canWalkThrough but also won't impede movement at all. so not including doors or fence gates (we'd have to right click),
/// not including water, and not including ladders or vines or cobwebs (they slow us down)
///
/// `fullyPassable(CalculationContext, int, int, int)`
#[inline]
pub fn fully_passable(context: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    fully_passable_state(context, x, y, z, context.get(x, y, z))
}

/// `fullyPassable(CalculationContext, int, int, int, BlockState)`
#[inline]
pub fn fully_passable_state(
    context: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    context
        .precomputed_data
        .fully_passable(&context.bsi, x, y, z, state)
}

/// `fullyPassable(IPlayerContext, BlockPos)`
pub fn fully_passable_ctx(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> bool {
    let state = ctx.world().get_block_state(pos);
    let fully_passable = fully_passable_block_state(state);
    if fully_passable == Ternary::Yes {
        return true;
    }
    if fully_passable == Ternary::No {
        return false;
    }
    state.pathfindable_land
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

pub fn is_door_passable(
    ctx: &dyn IPlayerContext,
    door_pos: BetterBlockPos,
    player_pos: BetterBlockPos,
) -> bool {
    if player_pos == door_pos {
        return false;
    }

    let state = BlockStateInterface::get(ctx, door_pos);
    if state.openable != Some(Openable::Door) {
        return true;
    }

    is_horizontal_block_passable(door_pos, state, player_pos, state.open)
}

pub fn is_gate_passable(
    ctx: &dyn IPlayerContext,
    gate_pos: BetterBlockPos,
    player_pos: BetterBlockPos,
) -> bool {
    if player_pos == gate_pos {
        return false;
    }

    let state = BlockStateInterface::get(ctx, gate_pos);
    if state.openable != Some(Openable::FenceGate) {
        return true;
    }

    state.open
}

/// Upstream takes the property to read the open state from (`propertyOpen`); the port takes the
/// open state.
pub fn is_horizontal_block_passable(
    block_pos: BetterBlockPos,
    block_state: &BlockState,
    player_pos: BetterBlockPos,
    open: bool,
) -> bool {
    if player_pos == block_pos {
        return false;
    }

    // getValue(HorizontalDirectionalBlock.FACING) throws for a state without it
    let facing = block_state
        .facing
        .expect("IllegalArgumentException: no facing")
        .get_axis();

    let player_facing = if player_pos.north() == block_pos || player_pos.south() == block_pos {
        Axis::Z
    } else if player_pos.east() == block_pos || player_pos.west() == block_pos {
        Axis::X
    } else {
        return true;
    };

    (facing == player_facing) == open
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

/// `canWalkOn(CalculationContext, int, int, int, BlockState)`
#[inline]
pub fn can_walk_on_state(
    context: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    context
        .precomputed_data
        .can_walk_on(&context.bsi, x, y, z, state)
}

/// `canWalkOn(CalculationContext, int, int, int)`
#[inline]
pub fn can_walk_on(context: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    can_walk_on_state(context, x, y, z, context.get(x, y, z))
}

/// `canWalkOn(IPlayerContext, BetterBlockPos, BlockState)`
pub fn can_walk_on_ctx_state(
    ctx: &dyn IPlayerContext,
    pos: BetterBlockPos,
    state: &BlockState,
) -> bool {
    can_walk_on_bsi_state(
        &BlockStateInterface::from_ctx(ctx),
        pos.x,
        pos.y,
        pos.z,
        state,
    )
}

/// `canWalkOn(IPlayerContext, BlockPos)` and `canWalkOn(IPlayerContext, BetterBlockPos)`
pub fn can_walk_on_ctx(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> bool {
    can_walk_on_bsi(&BlockStateInterface::from_ctx(ctx), pos.x, pos.y, pos.z)
}

/// `canWalkOn(BlockStateInterface, int, int, int)`
pub fn can_walk_on_bsi(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
    can_walk_on_bsi_state(bsi, x, y, z, bsi.get0(x, y, z))
}

/// `canUseFrostWalker(CalculationContext, BlockState)`
pub fn can_use_frost_walker(context: &CalculationContext, state: &BlockState) -> bool {
    // state == FrostedIceBlock.meltsInto() (the default water state, a source) && LEVEL == 0
    context.frost_walker != 0
        && state.liquid_block
        && state.fluid == Fluid::Water
        && state.fluid_source
}

/// `canUseFrostWalker(IPlayerContext, BlockPos)`: the host's `frost_walker` is the level of a
/// Frost Walker enchantment on the equipment, 0 without.
pub fn can_use_frost_walker_ctx(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> bool {
    let has_frost_walker = ctx.player().frost_walker != 0;
    let state = BlockStateInterface::get(ctx, pos);
    // state == FrostedIceBlock.meltsInto() (the default water state, a source) && LEVEL == 0
    has_frost_walker && state.liquid_block && state.fluid == Fluid::Water && state.fluid_source
}

/// If movements make us stand/walk on this block, will it have a top to walk on?
pub fn must_be_solid_to_walk_on(
    context: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
) -> bool {
    if is_climbable(state) {
        return false;
    }
    if state.fluid != Fluid::Empty {
        // used for frostwalker so only includes blocks where we are still on ground when leaving them to any side
        if let Some(slab) = state.slab {
            if slab != SlabType::Bottom {
                return true;
            }
        } else if let Some(stairs) = state.stairs {
            if stairs.half == Half::Top {
                return true;
            }
            // SHAPE is INNER_LEFT or INNER_RIGHT
            if stairs.inner_corner {
                return true;
            }
        } else if state.openable == Some(Openable::TrapDoor) {
            if !state.open && state.half == Half::Top {
                return true;
            }
        } else if state.climbable == Some(Climbable::Scaffolding) || state.leaves {
            return true;
        }
        if context.assume_walk_on_water {
            return false;
        }
        let block_above = context.get_block(x, y.wrapping_add(1), z);
        if block_above.liquid_block {
            return false;
        }
    }
    true
}

pub fn can_place_against(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
    can_place_against_state(bsi, x, y, z, bsi.get0(x, y, z))
}

/// `canPlaceAgainst(BlockStateInterface, BlockPos)`
pub fn can_place_against_pos(bsi: &BlockStateInterface, pos: BetterBlockPos) -> bool {
    can_place_against(bsi, pos.x, pos.y, pos.z)
}

/// `canPlaceAgainst(IPlayerContext, BlockPos)`
pub fn can_place_against_ctx(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> bool {
    can_place_against_pos(&BlockStateInterface::from_ctx(ctx), pos)
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

/// `getMiningDurationTicks(CalculationContext, int, int, int, boolean)`
pub fn get_mining_duration_ticks(
    context: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    include_falling: bool,
) -> f64 {
    get_mining_duration_ticks_state(context, x, y, z, context.get(x, y, z), include_falling)
}

/// `getMiningDurationTicks(CalculationContext, int, int, int, BlockState, boolean)`
pub fn get_mining_duration_ticks_state(
    context: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    state: &BlockState,
    include_falling: bool,
) -> f64 {
    if !can_walk_through_state(context, x, y, z, state) {
        if state.fluid != Fluid::Empty {
            return COST_INF;
        }
        let mult = context.break_cost_multiplier_at(x, y, z, state);
        if mult >= COST_INF {
            return COST_INF;
        }
        if avoid_breaking(&context.bsi, x, y, z, state) {
            return COST_INF;
        }
        let str_vs_block = context.tool_set.get_str_vs_block(state);
        if str_vs_block <= 0.0 {
            return COST_INF;
        }
        let mut result = 1.0 / str_vs_block;
        result += context.break_block_additional_cost;
        result *= mult;
        if include_falling {
            let above = context.get(x, y.wrapping_add(1), z);
            if above.falls {
                result +=
                    get_mining_duration_ticks_state(context, x, y.wrapping_add(1), z, above, true);
            }
        }
        return result;
    }
    0.0 // we won't actually mine it, so don't check fallings above
}

pub fn is_bottom_slab(state: &BlockState) -> bool {
    state.slab == Some(SlabType::Bottom)
}

/// AutoTool for a specific block
///
/// `switchToBestToolFor(IPlayerContext, BlockState)`: `b` is the blockstate to mine.
pub fn switch_to_best_tool_for(ctx: &mut dyn IPlayerContext, b: &BlockState) {
    let ts = ToolSet::new(
        Arc::new(ctx.player().clone()),
        Arc::clone(ctx.world().table()),
    );
    switch_to_best_tool_for_tool_set(ctx, b, &ts, settings().prefer_silk_touch);
}

/// AutoTool for a specific block with precomputed ToolSet data
///
/// `switchToBestToolFor(IPlayerContext, BlockState, ToolSet, boolean)`: `b` is the blockstate
/// to mine, `ts` the previously calculated ToolSet.
pub fn switch_to_best_tool_for_tool_set(
    ctx: &mut dyn IPlayerContext,
    b: &BlockState,
    ts: &ToolSet,
    prefer_silk_touch: bool,
) {
    let settings = settings();
    if settings.auto_tool && !settings.assume_external_auto_tool {
        let slot = ts.get_best_slot(b, prefer_silk_touch);
        ctx.player_mut().inventory.set_selected_slot(slot);
    }
}

pub fn move_towards(ctx: &dyn IPlayerContext, state: &mut MovementState, pos: BetterBlockPos) {
    state
        .set_target(MovementTarget::new(
            rotation_utils::calc_rotation_from_vec3d(
                ctx.player_head(),
                vec_utils::get_block_pos_center(pos),
                ctx.player_rotations(),
            )
            .with_pitch(ctx.player_rotations().get_pitch()),
            false,
        ))
        .set_input(Input::MoveForward, true);
}

/// `moveTowardsWithoutRotation(IPlayerContext, MovementState, float)`
pub fn move_towards_without_rotation(
    ctx: &dyn IPlayerContext,
    state: &mut MovementState,
    ideal_yaw: f32,
) {
    let yaw = ctx.player_rotations().get_yaw();
    let options = MovementOption::get_options(
        mth::sin((yaw * DEG_TO_RAD_F) as f64),
        mth::cos((yaw * DEG_TO_RAD_F) as f64),
        settings().allow_sprint,
    );
    let ideal_x = mth::sin((ideal_yaw * DEG_TO_RAD_F) as f64);
    let ideal_z = mth::cos((ideal_yaw * DEG_TO_RAD_F) as f64);
    // Stream.min keeps the first of equal elements; Comparator.comparing boxes to Float
    let mut selection: Option<(MovementOption, f32)> = None;
    for option in options {
        let key = option.distance_to_sq(ideal_x, ideal_z);
        if selection.is_none_or(|(_, best)| key.total_cmp(&best).is_lt()) {
            selection = Some((option, key));
        }
    }
    if let Some((selection, _)) = selection {
        selection.set_inputs(state);
    }
}

/// `moveTowardsWithoutRotation(IPlayerContext, MovementState, BlockPos)`
pub fn move_towards_without_rotation_pos(
    ctx: &dyn IPlayerContext,
    state: &mut MovementState,
    dest: BetterBlockPos,
) {
    let ideal_yaw = rotation_utils::calc_rotation_from_vec3d(
        ctx.player_head(),
        vec_utils::get_block_pos_center(dest),
        ctx.player_rotations(),
    )
    .get_yaw();
    move_towards_without_rotation(ctx, state, ideal_yaw);
}

pub fn move_towards_with_slight_rotation(
    ctx: &dyn IPlayerContext,
    state: &mut MovementState,
    dest: BetterBlockPos,
) {
    let ideal_yaw = rotation_utils::calc_rotation_from_vec3d(
        ctx.player_head(),
        vec_utils::get_block_pos_center(dest),
        ctx.player_rotations(),
    )
    .get_yaw();
    let distance =
        Rotation::yaw_distance_from_offset(ctx.player_rotations().get_yaw(), ideal_yaw) % 45.0f32;
    let new_yaw = if distance > 0.0 {
        if distance > 22.5 {
            distance - 45.0
        } else {
            distance
        }
    } else if distance < -22.5 {
        distance + 45.0
    } else {
        distance
    };
    state.set_target(MovementTarget::new(
        Rotation::new(
            ctx.player_rotations().get_yaw() - new_yaw,
            ctx.player_rotations().get_pitch(),
        ),
        true,
    ));
    move_towards_without_rotation(ctx, state, ideal_yaw);
}

/// Returns whether or not the specified block is
/// water, regardless of whether or not it is flowing.
pub fn is_water(state: &BlockState) -> bool {
    state.fluid == Fluid::Water
}

/// Returns whether or not the block at the specified pos is
/// water, regardless of whether or not it is flowing.
///
/// `isWater(IPlayerContext, BlockPos)`
pub fn is_water_ctx(ctx: &dyn IPlayerContext, bp: BetterBlockPos) -> bool {
    is_water(BlockStateInterface::get(ctx, bp))
}

/// Returns whether or not the specified pos has a liquid
///
/// `isLiquid(IPlayerContext, BlockPos)`
pub fn is_liquid_ctx(ctx: &dyn IPlayerContext, p: BetterBlockPos) -> bool {
    is_liquid(BlockStateInterface::get(ctx, p))
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

pub fn attempt_to_place_a_block(
    state: &mut MovementState,
    baritone: &mut Baritone,
    place_at: BetterBlockPos,
    prefer_down: bool,
    would_sneak: bool,
) -> PlaceResult {
    let ctx = &baritone.player_context;
    let aim = baritone.look_behavior.get_aim_processor();
    let direct = rotation_utils::reachable_sneak(ctx, aim, place_at, would_sneak); // we assume that if there is a block there, it must be replacable
    let mut found = false;
    if let Some(direct) = direct {
        state.set_target(MovementTarget::new(direct, true));
        found = true;
    }
    for direction in HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP {
        let against1 = place_at.relative(direction);
        if can_place_against_ctx(&baritone.player_context, against1) {
            if !InventoryBehavior::select_throwaway_for_location(
                baritone, false, place_at.x, place_at.y, place_at.z,
            ) {
                // get ready to place a throwaway block
                log_debug("bb pls get me some blocks. dirt, netherrack, cobble");
                state.set_status(MovementStatus::Unreachable);
                return PlaceResult::NoOption;
            }
            let ctx = &baritone.player_context;
            let face_x = (place_at.x.wrapping_add(against1.x) as f64 + 1.0) * 0.5;
            let face_y = (place_at.y.wrapping_add(against1.y) as f64 + 0.5) * 0.5;
            let face_z = (place_at.z.wrapping_add(against1.z) as f64 + 1.0) * 0.5;
            let place = rotation_utils::calc_rotation_from_vec3d(
                if would_sneak {
                    ray_trace_utils::infer_sneaking_eye_position(ctx.player())
                } else {
                    ctx.player_head()
                },
                Vec3::new(face_x, face_y, face_z),
                ctx.player_rotations(),
            );
            let actual = baritone
                .look_behavior
                .get_aim_processor()
                .peek_rotation(ctx, place);
            let res = ray_trace_utils::ray_trace_towards_sneak(
                ctx.player(),
                ctx.world(),
                actual,
                ctx.player_controller_ref().get_block_reach_distance(),
                would_sneak,
            );
            if res.get_type() == HitResultType::Block
                && res.get_block_pos() == against1
                && res.get_block_pos().relative(res.get_direction()) == place_at
            {
                state.set_target(MovementTarget::new(place, true));
                found = true;

                if !prefer_down {
                    // if preferDown is true, we want the last option
                    // if preferDown is false, we want the first
                    break;
                }
            }
        }
    }
    let ctx = &baritone.player_context;
    if let Some(selected_block) = ctx.get_selected_block() {
        let side = ctx.object_mouse_over().get_direction();
        // only way for selectedBlock.equals(placeAt) to be true is if it's replaceable
        if selected_block == place_at
            || (can_place_against_ctx(ctx, selected_block)
                && selected_block.relative(side) == place_at)
        {
            if would_sneak {
                state.set_input(Input::Sneak, true);
            }
            InventoryBehavior::select_throwaway_for_location(
                baritone, true, place_at.x, place_at.y, place_at.z,
            );
            return PlaceResult::ReadyToPlace;
        }
    }
    if found {
        if would_sneak {
            state.set_input(Input::Sneak, true);
        }
        InventoryBehavior::select_throwaway_for_location(
            baritone, true, place_at.x, place_at.y, place_at.z,
        );
        return PlaceResult::Attempting;
    }
    PlaceResult::NoOption
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceResult {
    ReadyToPlace,
    Attempting,
    NoOption,
}

/// Upstream takes the `Block`: air, `Blocks.LAVA`, `Blocks.WATER`.
pub fn is_transparent(b: &BlockState) -> bool {
    b.air || b.liquid_block
}

pub fn stepping_on_blocks(ctx: &dyn IPlayerContext) -> Vec<BetterBlockPos> {
    let player = ctx.player();
    let mut blocks = Vec::new();
    let block_position = player.block_position();
    let corner = Vec3::new(
        block_position.x as f64,
        block_position.y as f64,
        block_position.z as f64,
    );
    for x in -1i8..=1 {
        for z in -1i8..=1 {
            if player.bounding_box.intersects_vec(
                corner.add(x as f64, 0.0, z as f64),
                corner.add((x + 1) as f64, 1.0, (z + 1) as f64),
            ) {
                blocks.push(BetterBlockPos::new(
                    block_position.x.wrapping_add(x as i32),
                    block_position.y.wrapping_sub(1),
                    block_position.z.wrapping_add(z as i32),
                ));
            }
        }
    }
    blocks
}

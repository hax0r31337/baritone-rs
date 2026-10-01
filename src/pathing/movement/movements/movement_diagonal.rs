// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementDiagonal.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::java::max_f64;
use crate::mc::Direction;
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::{is_magma, is_soul_sand, is_water_block};
use crate::pathing::movement::{CalculationContext, Movement, MovementState};
use crate::settings::settings;
use crate::utils::BlockStateInterface;
use crate::utils::pathing::MutableMoveResult;

/// `Math.sqrt(2)`
fn sqrt_2() -> f64 {
    2f64.sqrt()
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovementDiagonal;

impl MovementDiagonal {
    /// `MovementDiagonal(IBaritone, BetterBlockPos, Direction, Direction, int)`
    pub fn new(start: BetterBlockPos, dir1: Direction, dir2: Direction, dy: i32) -> Movement {
        Self::from_dirs(start, start.relative(dir1), start.relative(dir2), dir2, dy)
        // super(start, start.offset(dir1).offset(dir2), new BlockPos[]{start.offset(dir1), start.offset(dir1).up(), start.offset(dir2), start.offset(dir2).up(), start.offset(dir1).offset(dir2), start.offset(dir1).offset(dir2).up()}, new BlockPos[]{start.offset(dir1).offset(dir2).down()});
    }

    fn from_dirs(
        start: BetterBlockPos,
        dir1: BetterBlockPos,
        dir2: BetterBlockPos,
        drr2: Direction,
        dy: i32,
    ) -> Movement {
        Self::from_end(start, dir1.relative(drr2).above_n(dy), dir1, dir2)
    }

    fn from_end(
        start: BetterBlockPos,
        end: BetterBlockPos,
        dir1: BetterBlockPos,
        dir2: BetterBlockPos,
    ) -> Movement {
        Movement::new(
            start,
            end,
            Box::new([dir1, dir1.above(), dir2, dir2.above(), end, end.above()]),
            None,
            MovementKind::Diagonal(MovementDiagonal),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        let mut result = MutableMoveResult::new();
        Self::cost(
            context,
            m.src.x,
            m.src.y,
            m.src.z,
            m.dest.x,
            m.dest.z,
            &mut result,
        );
        if result.y != m.dest.y {
            return COST_INF; // doesn't apply to us, this position is incorrect
        }
        result.cost
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        let (src, dest) = (m.src, m.dest);
        let diag_a = BetterBlockPos::new(src.x, src.y, dest.z);
        let diag_b = BetterBlockPos::new(dest.x, src.y, src.z);
        if dest.y < src.y {
            return FxHashSet::from_iter([
                src,
                dest.above(),
                diag_a,
                diag_b,
                dest,
                diag_a.below(),
                diag_b.below(),
            ]);
        }
        if dest.y > src.y {
            return FxHashSet::from_iter([
                src,
                src.above(),
                diag_a,
                diag_b,
                dest,
                diag_a.above(),
                diag_b.above(),
            ]);
        }
        FxHashSet::from_iter([src, dest, diag_a, diag_b])
    }

    pub fn cost(
        context: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        dest_x: i32,
        dest_z: i32,
        res: &mut MutableMoveResult,
    ) {
        let costs = &*context.costs;
        if !mh::can_walk_through(context, dest_x, y + 1, dest_z) {
            return;
        }
        let dest_into = context.get(dest_x, y, dest_z);
        let from_down;
        let mut ascend = false;
        let dest_walk_on;
        let mut descend = false;
        let mut frost_walker = false;
        let mut sneaking = false;
        if !mh::can_walk_through_state(context, dest_x, y, dest_z, dest_into) {
            ascend = true;
            if !context.allow_diagonal_ascend
                || !mh::can_walk_through(context, x, y + 2, z)
                || !mh::can_walk_on_state(context, dest_x, y, dest_z, dest_into)
                || !mh::can_walk_through(context, dest_x, y + 2, dest_z)
            {
                return;
            }
            dest_walk_on = dest_into;
            from_down = context.get(x, y - 1, z);
        } else {
            dest_walk_on = context.get(dest_x, y - 1, dest_z);
            from_down = context.get(x, y - 1, z);
            let standing_on_a_block = mh::must_be_solid_to_walk_on(context, x, y - 1, z, from_down);
            frost_walker = standing_on_a_block && mh::can_use_frost_walker(context, dest_walk_on);
            if !frost_walker && !mh::can_walk_on_state(context, dest_x, y - 1, dest_z, dest_walk_on)
            {
                descend = true;
                if !context.allow_diagonal_descend
                    || !mh::can_walk_on(context, dest_x, y - 2, dest_z)
                    || !mh::can_walk_through_state(context, dest_x, y - 1, dest_z, dest_walk_on)
                {
                    return;
                }
            }
            frost_walker &= !context.assume_walk_on_water; // do this after checking for descends because jesus can't prevent the water from freezing, it just prevents us from relying on the water freezing
        }
        let mut multiplier = costs.walk_one_block_cost;
        // For either possible soul sand, that affects half of our walking
        if is_soul_sand(dest_walk_on) {
            multiplier += (costs.walk_one_over_soul_sand_cost - costs.walk_one_block_cost) / 2.0;
        } else if context.allow_walk_on_magma_blocks && is_magma(dest_walk_on) {
            multiplier += (costs.sneak_one_block_cost - costs.walk_one_block_cost) / 2.0;
            sneaking = true;
        } else if frost_walker {
            // frostwalker lets us walk on water without the penalty
        } else if is_water_block(dest_walk_on) {
            multiplier += context.walk_on_water_one_penalty * sqrt_2();
        }
        let from_down_block = from_down;
        if mh::is_climbable(from_down_block) {
            return;
        }
        if is_soul_sand(from_down_block) {
            multiplier += (costs.walk_one_over_soul_sand_cost - costs.walk_one_block_cost) / 2.0;
        } else if context.allow_walk_on_magma_blocks && is_magma(from_down_block) {
            multiplier += (costs.sneak_one_block_cost - costs.walk_one_block_cost) / 2.0;
            sneaking = true;
        }
        let cutting_over1 = context.get(x, y - 1, dest_z);
        if (!context.allow_walk_on_magma_blocks && is_magma(cutting_over1))
            || context.fluid_in(x, y - 1, dest_z, cutting_over1).is_lava()
        {
            return;
        }
        let cutting_over2 = context.get(dest_x, y - 1, z);
        // upstream tests cuttingOver1 for magma again here; kept
        if (!context.allow_walk_on_magma_blocks && is_magma(cutting_over1))
            || context.fluid_in(dest_x, y - 1, z, cutting_over2).is_lava()
        {
            return;
        }
        let mut water = false;
        let start_state = context.get(x, y, z);
        let start_in = start_state;
        if context.fluid_in(x, y, z, start_state).is_water()
            || context.fluid_in(dest_x, y, dest_z, dest_into).is_water()
        {
            if ascend {
                return;
            }
            // Ignore previous multiplier
            // Whatever we were walking on (possibly soul sand) doesn't matter as we're actually floating on water
            // Not even touching the blocks below
            multiplier = context.water_walk_speed;
            water = true;
        }
        let pb0 = context.get(x, y, dest_z);
        let pb0_fluid = context.fluid_in(x, y, dest_z, pb0);
        let pb2 = context.get(dest_x, y, z);
        let pb2_fluid = context.fluid_in(dest_x, y, z, pb2);
        if ascend {
            let a_top = mh::can_walk_through(context, x, y + 2, dest_z);
            let a_mid = mh::can_walk_through(context, x, y + 1, dest_z);
            let a_low = mh::can_walk_through_state(context, x, y, dest_z, pb0);
            let b_top = mh::can_walk_through(context, dest_x, y + 2, z);
            let b_mid = mh::can_walk_through(context, dest_x, y + 1, z);
            let b_low = mh::can_walk_through_state(context, dest_x, y, z, pb2);
            if (!(a_top && a_mid && a_low) && !(b_top && b_mid && b_low)) // no option
                || mh::avoid_walking_into(pb0, pb0_fluid) // bad
                || mh::avoid_walking_into(pb2, pb2_fluid) // bad
                || (a_top && a_mid && mh::can_walk_on_state(context, x, y, dest_z, pb0)) // we could just ascend
                || (b_top && b_mid && mh::can_walk_on_state(context, dest_x, y, z, pb2)) // we could just ascend
                || (!a_top && a_mid && a_low) // head bonk A
                || (!b_top && b_mid && b_low)
            {
                // head bonk B
                return;
            }
            res.cost = multiplier * sqrt_2() + costs.jump_one_block_cost;
            res.x = dest_x;
            res.z = dest_z;
            res.y = y + 1;
            return;
        }
        let mut option_a = mh::get_mining_duration_ticks_state(context, x, y, dest_z, pb0, false);
        let mut option_b = mh::get_mining_duration_ticks_state(context, dest_x, y, z, pb2, false);
        if option_a != 0.0 && option_b != 0.0 {
            // check these one at a time -- if pb0 and pb2 were nonzero, we already know that (optionA != 0 && optionB != 0)
            // so no need to check pb1 as well, might as well return early here
            return;
        }
        let pb1 = context.get(x, y + 1, dest_z);
        option_a += mh::get_mining_duration_ticks_state(context, x, y + 1, dest_z, pb1, true);
        if option_a != 0.0 && option_b != 0.0 {
            // same deal, if pb1 makes optionA nonzero and option B already was nonzero, pb3 can't affect the result
            return;
        }
        let pb3 = context.get(dest_x, y + 1, z);
        if option_a == 0.0
            && ((mh::avoid_walking_into(pb2, pb2_fluid) && !is_water_block(pb2))
                || mh::avoid_walking_into(pb3, context.fluid_in(dest_x, y + 1, z, pb3)))
        {
            // at this point we're done calculating optionA, so we can check if it's actually possible to edge around in that direction
            return;
        }
        option_b += mh::get_mining_duration_ticks_state(context, dest_x, y + 1, z, pb3, true);
        if option_a != 0.0 && option_b != 0.0 {
            // and finally, if the cost is nonzero for both ways to approach this diagonal, it's not possible
            return;
        }
        if option_b == 0.0
            && ((mh::avoid_walking_into(pb0, pb0_fluid) && !is_water_block(pb0))
                || mh::avoid_walking_into(pb1, context.fluid_in(x, y + 1, dest_z, pb1)))
        {
            // and now that option B is fully calculated, see if we can edge around that way
            return;
        }
        if option_a != 0.0 || option_b != 0.0 {
            multiplier *= sqrt_2() - 0.001; // TODO tune
            if mh::is_climbable(start_in) {
                // edging around doesn't work if doing so would climb a ladder or vine instead of moving sideways
                return;
            }
        } else {
            // only can sprint if not edging around
            if context.can_sprint && !water && !sneaking {
                // If we aren't edging around anything, and we aren't in water
                // We can sprint =D
                // Don't check for soul sand, since we can sprint on that too
                multiplier *= costs.sprint_multiplier;
            }
        }
        res.cost = multiplier * sqrt_2();
        if descend {
            res.cost += max_f64(costs.fall_n_blocks_cost[1], costs.center_after_fall_cost);
            res.y = y - 1;
        } else {
            res.y = y;
        }
        res.x = dest_x;
        res.z = dest_z;
    }

    pub(crate) fn safe_to_cancel(m: &Movement, baritone: &Baritone) -> bool {
        //too simple. backfill does not work after cornering with this
        //return context.precomputedData.canWalkOn(ctx, ctx.playerFeet().down());
        let ctx = &baritone.player_context;
        let player = ctx.player();
        let offset = 0.25;
        let x = player.position.x;
        let y = player.position.y - 1.0;
        let z = player.position.z;
        let (src, dest) = (m.src, m.dest);
        //standard
        if ctx.player_feet() == src {
            return true;
        }
        //both corners are walkable
        if mh::can_walk_on_ctx(ctx, BetterBlockPos::new(src.x, src.y - 1, dest.z))
            && mh::can_walk_on_ctx(ctx, BetterBlockPos::new(dest.x, src.y - 1, src.z))
        {
            return true;
        }
        //we are in a likely unwalkable corner, check for a supporting block
        if ctx.player_feet() == BetterBlockPos::new(src.x, src.y, dest.z)
            || ctx.player_feet() == BetterBlockPos::new(dest.x, src.y, src.z)
        {
            return mh::can_walk_on_ctx(ctx, BetterBlockPos::from_f64(x + offset, y, z + offset))
                || mh::can_walk_on_ctx(ctx, BetterBlockPos::from_f64(x + offset, y, z - offset))
                || mh::can_walk_on_ctx(ctx, BetterBlockPos::from_f64(x - offset, y, z + offset))
                || mh::can_walk_on_ctx(ctx, BetterBlockPos::from_f64(x - offset, y, z - offset));
        }
        true
    }

    pub(crate) fn update_state(
        m: &mut Movement,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) {
        m.update_state_default(baritone, state);
        if state.get_status() != MovementStatus::Running {
            return;
        }

        let (src, dest) = (m.src, m.dest);
        if baritone.player_context.player_feet() == dest {
            state.set_status(MovementStatus::Success);
            return;
        } else if !m.player_in_valid_position(baritone)
            && !{
                let ctx = &baritone.player_context;
                mh::is_liquid_ctx(ctx, src)
                    && m.get_valid_positions().contains(&ctx.player_feet().above())
            }
        {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        let ctx = &baritone.player_context;
        if dest.y > src.y
            && ctx.player().position.y < src.y as f64 + 0.1
            && ctx.player().horizontal_collision
        {
            state.set_input(Input::Jump, true);
        }
        if Self::sprint(m, ctx) {
            state.set_input(Input::Sprint, true);
        }
        state.set_input(
            Input::Sneak,
            settings().allow_walk_on_magma_blocks
                && mh::stepping_on_blocks(ctx)
                    .into_iter()
                    .any(|block| is_magma(ctx.world().get_block_state(block))),
        );
        mh::move_towards(ctx, state, dest);
    }

    fn sprint(m: &Movement, ctx: &dyn IPlayerContext) -> bool {
        if mh::is_liquid_ctx(ctx, ctx.player_feet()) && !settings().sprint_in_water {
            return false;
        }
        for i in 0..4 {
            if !mh::can_walk_through_ctx(ctx, m.positions_to_break[i]) {
                return false;
            }
        }
        true
    }

    pub(crate) fn prepared(
        _m: &mut Movement,
        _baritone: &mut Baritone,
        _state: &mut MovementState,
    ) -> bool {
        true
    }

    pub(crate) fn to_break(m: &mut Movement, bsi: &BlockStateInterface) -> Vec<BetterBlockPos> {
        if let Some(to_break_cached) = &m.to_break_cached {
            return to_break_cached.clone();
        }
        let mut result = Vec::new();
        for i in 4..6 {
            let pos = m.positions_to_break[i];
            if !mh::can_walk_through_bsi(bsi, pos.x, pos.y, pos.z) {
                result.push(pos);
            }
        }
        m.to_break_cached = Some(result.clone());
        result
    }

    pub(crate) fn to_walk_into(m: &mut Movement, bsi: &BlockStateInterface) -> Vec<BetterBlockPos> {
        if m.to_walk_into_cached.is_none() {
            m.to_walk_into_cached = Some(Vec::new());
        }
        let mut result = Vec::new();
        for i in 0..4 {
            let pos = m.positions_to_break[i];
            if !mh::can_walk_through_bsi(bsi, pos.x, pos.y, pos.z) {
                result.push(pos);
            }
        }
        m.to_walk_into_cached = Some(result.clone());
        result
    }
}

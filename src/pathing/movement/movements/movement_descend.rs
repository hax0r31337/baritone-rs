// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementDescend.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: reset, updateState, safeMode, skipToAscend.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::host::BlockState;
use crate::java::max_f64;
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::{is_ladder_or_vine, is_soul_sand};
use crate::pathing::movement::{CalculationContext, Movement};
use crate::utils::pathing::MutableMoveResult;

#[derive(Clone, Debug, PartialEq)]
pub struct MovementDescend {
    pub num_ticks: i32,
    pub force_safe_mode: bool,
}

impl MovementDescend {
    pub fn new(start: BetterBlockPos, end: BetterBlockPos) -> Movement {
        Movement::new(
            start,
            end,
            Box::new([end.above_n(2), end.above(), end]),
            Some(end.below()),
            MovementKind::Descend(MovementDescend {
                num_ticks: 0,
                force_safe_mode: false,
            }),
        )
    }

    /// Called by PathExecutor if needing safeMode can only be detected with knowledge about the next movement
    pub fn force_safe_mode(&mut self) {
        self.force_safe_mode = true;
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
            return COST_INF; // doesn't apply to us, this position is a fall not a descend
        }
        result.cost
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        FxHashSet::from_iter([m.src, m.dest.above(), m.dest])
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
        let mut total_cost = 0.0;
        let dest_down = context.get(dest_x, y - 1, dest_z);
        total_cost +=
            mh::get_mining_duration_ticks_state(context, dest_x, y - 1, dest_z, dest_down, false);
        if total_cost >= COST_INF {
            return;
        }
        total_cost += mh::get_mining_duration_ticks(context, dest_x, y, dest_z, false);
        if total_cost >= COST_INF {
            return;
        }
        total_cost += mh::get_mining_duration_ticks(context, dest_x, y + 1, dest_z, true); // only the top block in the 3 we need to mine needs to consider the falling blocks above
        if total_cost >= COST_INF {
            return;
        }

        let from_down = context.get(x, y - 1, z);
        if mh::is_climbable(from_down) {
            return;
        }

        // A
        //SA
        // A
        // B
        // C
        // D
        //if S is where you start, B needs to be air for a movementfall
        //A is plausibly breakable by either descend or fall
        //C, D, etc determine the length of the fall

        let below = context.get(dest_x, y - 2, dest_z);
        if !mh::can_walk_on_state(context, dest_x, y - 2, dest_z, below) {
            Self::dynamic_fall_cost(context, x, y, z, dest_x, dest_z, total_cost, below, res);
            return;
        }

        if is_ladder_or_vine(dest_down) {
            return;
        }
        if mh::can_use_frost_walker(context, dest_down) {
            // no need to check assumeWalkOnWater
            return; // the water will freeze when we try to walk into it
        }

        // we walk half the block plus 0.3 to get to the edge, then we walk the other 0.2 while simultaneously falling (math.max because of how it's in parallel)
        let mut walk = costs.walk_off_block_cost;
        if is_soul_sand(from_down) {
            // use this ratio to apply the soul sand speed penalty to our 0.8 block distance
            walk *= costs.walk_one_over_soul_sand_cost / costs.walk_one_block_cost;
        }
        total_cost += walk + max_f64(costs.fall_n_blocks_cost[1], costs.center_after_fall_cost);
        res.x = dest_x;
        res.y = y - 1;
        res.z = dest_z;
        res.cost = total_cost;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dynamic_fall_cost(
        context: &CalculationContext,
        _x: i32,
        y: i32,
        _z: i32,
        dest_x: i32,
        dest_z: i32,
        front_break: f64,
        below: &BlockState,
        res: &mut MutableMoveResult,
    ) -> bool {
        let costs = &*context.costs;
        if front_break != 0.0 && context.get(dest_x, y + 2, dest_z).falls {
            // if frontBreak is 0 we can actually get through this without updating the falling block and making it actually fall
            // but if frontBreak is nonzero, we're breaking blocks in front, so don't let anything fall through this column,
            // and potentially replace the water we're going to fall into
            return false;
        }
        if !mh::can_walk_through_state(context, dest_x, y - 2, dest_z, below) {
            return false;
        }
        let mut cost_so_far = 0.0;
        let mut effective_start_height = y;
        let mut fall_height: i32 = 3;
        loop {
            let new_y = y - fall_height;
            if new_y < context.world.dimension().min_y {
                // when pathing in the end, where you could plausibly fall into the void
                // this check prevents it from getting the block at y=(below whatever the minimum height is) and crashing
                return false;
            }
            let reached_minimum = fall_height >= context.min_fall_height;
            let onto_block = context.get(dest_x, new_y, dest_z);
            let unprotected_fall_height = fall_height - (y - effective_start_height); // equal to fallHeight - y + effectiveFallHeight, which is equal to -newY + effectiveFallHeight, which is equal to effectiveFallHeight - newY
            let tentative_cost = costs.walk_off_block_cost
                + fall_n_blocks_cost(costs, unprotected_fall_height)
                + front_break
                + cost_so_far;
            if reached_minimum && mh::is_water(onto_block) {
                if !mh::can_walk_through_state(context, dest_x, new_y, dest_z, onto_block) {
                    return false;
                }
                if context.assume_walk_on_water {
                    return false; // TODO fix
                }
                if mh::is_flowing(dest_x, new_y, dest_z, onto_block, &context.bsi) {
                    return false; // TODO flowing check required here?
                }
                if !mh::can_walk_on(context, dest_x, new_y - 1, dest_z) {
                    // we could punch right through the water into something else
                    return false;
                }
                // found a fall into water
                res.x = dest_x;
                res.y = new_y;
                res.z = dest_z;
                res.cost = tentative_cost; // TODO incorporate water swim up cost?
                return false;
            }
            if reached_minimum && context.allow_fall_into_lava && mh::is_lava(onto_block) {
                // found a fall into lava
                res.x = dest_x;
                res.y = new_y;
                res.z = dest_z;
                res.cost = tentative_cost;
                return false;
            }
            if unprotected_fall_height <= 11 && mh::is_climbable(onto_block) {
                // if fall height is greater than or equal to 11, we don't actually grab on to vines or ladders. the more you know
                // this effectively "resets" our falling speed
                cost_so_far += fall_n_blocks_cost(costs, unprotected_fall_height - 1); // we fall until the top of this block (not including this block)
                cost_so_far += costs.ladder_down_one_cost;
                effective_start_height = new_y;
                fall_height += 1;
                continue;
            }
            if mh::can_walk_through_state(context, dest_x, new_y, dest_z, onto_block) {
                fall_height += 1;
                continue;
            }
            if !mh::can_walk_on_state(context, dest_x, new_y, dest_z, onto_block) {
                return false;
            }
            if mh::is_bottom_slab(onto_block) {
                return false; // falling onto a half slab is really glitchy, and can cause more fall damage than we'd expect
            }
            if reached_minimum && unprotected_fall_height <= context.max_fall_height_no_water + 1 {
                // fallHeight = 4 means onto.up() is 3 blocks down, which is the max
                res.x = dest_x;
                res.y = new_y + 1;
                res.z = dest_z;
                res.cost = tentative_cost;
                return false;
            }
            if reached_minimum
                && context.has_water_bucket
                && unprotected_fall_height <= context.max_fall_height_bucket + 1
            {
                res.x = dest_x;
                res.y = new_y + 1; // this is the block we're falling onto, so dest is +1
                res.z = dest_z;
                res.cost = tentative_cost + context.place_bucket_cost();
                return true;
            } else {
                return false;
            }
        }
    }
}

/// `FALL_N_BLOCKS_COST[n]`; out of range throws `ArrayIndexOutOfBoundsException` upstream.
#[inline]
fn fall_n_blocks_cost(costs: &crate::api::pathing::movement::ActionCosts, n: i32) -> f64 {
    costs.fall_n_blocks_cost[usize::try_from(n).expect("ArrayIndexOutOfBoundsException")]
}

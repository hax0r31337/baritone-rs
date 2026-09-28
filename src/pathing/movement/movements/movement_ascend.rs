// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementAscend.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: reset, updateState, headBonk, safeToCancel.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::java::max_f64;
use crate::pathing::movement::movement::{
    HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP, MovementKind,
};
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::{is_magma, is_soul_sand};
use crate::pathing::movement::{CalculationContext, Movement};

#[derive(Clone, Debug, PartialEq)]
pub struct MovementAscend {
    pub ticks_without_placement: i32,
}

impl MovementAscend {
    pub fn new(src: BetterBlockPos, dest: BetterBlockPos) -> Movement {
        Movement::new(
            src,
            dest,
            Box::new([dest, src.above_n(2), dest.above()]),
            Some(dest.below()),
            MovementKind::Ascend(MovementAscend {
                ticks_without_placement: 0,
            }),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        Self::cost(context, m.src.x, m.src.y, m.src.z, m.dest.x, m.dest.z)
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        let prior = m.src.subtract(m.get_direction()).above(); // sometimes we back up to place the block, also sprint ascends, also skip descend to straight ascend
        FxHashSet::from_iter([m.src, m.src.above(), m.dest, prior, prior.above()])
    }

    pub fn cost(
        context: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        dest_x: i32,
        dest_z: i32,
    ) -> f64 {
        let costs = &*context.costs;
        let to_place = context.get(dest_x, y, dest_z);
        let mut additional_placement_cost = 0.0;
        if !mh::can_walk_on_state(context, dest_x, y, dest_z, to_place) {
            additional_placement_cost = context.cost_of_placing_at(dest_x, y, dest_z, to_place);
            if additional_placement_cost >= COST_INF {
                return COST_INF;
            }
            if !mh::is_replaceable(dest_x, y, dest_z, to_place, &context.bsi) {
                return COST_INF;
            }
            let mut found_place_option = false;
            for dir in HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP {
                let against_x = dest_x + dir.get_step_x();
                let against_y = y + dir.get_step_y();
                let against_z = dest_z + dir.get_step_z();
                if against_x == x && against_z == z {
                    // we might be able to backplace now, but it doesn't matter because it will have been broken by the time we'd need to use it
                    continue;
                }
                if mh::can_place_against(&context.bsi, against_x, against_y, against_z) {
                    found_place_option = true;
                    break;
                }
            }
            if !found_place_option {
                // didn't find a valid place =(
                return COST_INF;
            }
        }
        let src_up2 = context.get(x, y + 2, z); // used lower down anyway
        if context.get(x, y + 3, z).falls
            && (mh::can_walk_through(context, x, y + 1, z) || !src_up2.falls)
        {
            //it would fall on us and possibly suffocate us
            // HOWEVER, we assume that we're standing in the start position
            // that means that src and src.up(1) are both air
            // maybe they aren't now, but they will be by the time this starts
            // if the lower one is can't walk through and the upper one is falling, that means that by standing on src
            // (the presupposition of this Movement)
            // we have necessarily already cleared the entire FallingBlock stack
            // on top of our head

            // as in, if we have a block, then two FallingBlocks on top of it
            // and that block is x, y+1, z, and we'd have to clear it to even start this movement
            // we don't need to worry about those FallingBlocks because we've already cleared them
            return COST_INF;
            // you may think we only need to check srcUp2, not srcUp
            // however, in the scenario where glitchy world gen where unsupported sand / gravel generates
            // it's possible srcUp is AIR from the start, and srcUp2 is falling
            // and in that scenario, when we arrive and break srcUp2, that lets srcUp3 fall on us and suffocate us
        }
        let src_down = context.get(x, y - 1, z);
        if mh::is_climbable(src_down) {
            return COST_INF;
        }
        // we can jump from soul sand, but not from a bottom slab
        let jumping_from_bottom_slab = mh::is_bottom_slab(src_down);
        let jumping_to_bottom_slab = mh::is_bottom_slab(to_place);
        if jumping_from_bottom_slab && !jumping_to_bottom_slab {
            return COST_INF; // the only thing we can ascend onto from a bottom slab is another bottom slab
        }
        let mut walk;
        if jumping_to_bottom_slab {
            if jumping_from_bottom_slab {
                walk = max_f64(costs.jump_one_block_cost, costs.walk_one_block_cost); // we hit space immediately on entering this action
                walk += context.jump_penalty;
            } else {
                walk = costs.walk_one_block_cost; // we don't hit space we just walk into the slab
            }
        } else {
            // jumpingFromBottomSlab must be false
            if is_soul_sand(to_place) {
                walk = costs.walk_one_over_soul_sand_cost;
            } else if is_magma(to_place) {
                walk = costs.sneak_one_block_cost;
            } else {
                walk = max_f64(costs.jump_one_block_cost, costs.walk_one_block_cost);
            }
            walk += context.jump_penalty;
        }

        let mut total_cost = walk + additional_placement_cost;
        // start with srcUp2 since we already have its state
        // includeFalling isn't needed because of the falling check above -- if srcUp3 is falling we will have already exited with COST_INF if we'd actually have to break it
        total_cost += mh::get_mining_duration_ticks_state(context, x, y + 2, z, src_up2, false);
        if total_cost >= COST_INF {
            return COST_INF;
        }
        total_cost += mh::get_mining_duration_ticks(context, dest_x, y + 1, dest_z, false);
        if total_cost >= COST_INF {
            return COST_INF;
        }
        total_cost += mh::get_mining_duration_ticks(context, dest_x, y + 2, dest_z, true);
        total_cost
    }
}

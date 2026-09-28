// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementTraverse.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: reset, updateState, safeToCancel, prepared.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::host::{Fluid, SlabType};
use crate::pathing::movement::movement::{
    HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP, MovementKind,
};
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::{is_magma, is_soul_sand, is_water_block};
use crate::pathing::movement::{CalculationContext, Movement};

#[derive(Clone, Debug, PartialEq)]
pub struct MovementTraverse {
    /// Did we have to place a bridge block or was it always there
    pub was_the_bridge_block_always_there: bool,
}

impl MovementTraverse {
    pub fn new(from: BetterBlockPos, to: BetterBlockPos) -> Movement {
        Movement::new(
            from,
            to,
            Box::new([to.above(), to]),
            Some(to.below()),
            MovementKind::Traverse(MovementTraverse {
                was_the_bridge_block_always_there: true,
            }),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        Self::cost(context, m.src.x, m.src.y, m.src.z, m.dest.x, m.dest.z)
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        FxHashSet::from_iter([m.src, m.dest]) // src.above means that we don't get caught in an infinite loop in water
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
        let pb0 = context.get(dest_x, y + 1, dest_z);
        let pb1 = context.get(dest_x, y, dest_z);
        let dest_on = context.get(dest_x, y - 1, dest_z);
        let src_down = context.get(x, y - 1, z);
        let src_down_block = src_down;
        let standing_on_a_block = mh::must_be_solid_to_walk_on(context, x, y - 1, z, src_down);
        let frost_walker = standing_on_a_block
            && !context.assume_walk_on_water
            && mh::can_use_frost_walker(context, dest_on);
        if frost_walker || mh::can_walk_on_state(context, dest_x, y - 1, dest_z, dest_on) {
            //this is a walk, not a bridge
            let mut wc = costs.walk_one_block_cost;
            let mut water = false;
            let mut sneaking = false;
            if mh::is_water(pb0) || mh::is_water(pb1) {
                wc = context.water_walk_speed;
                water = true;
            } else {
                if is_soul_sand(dest_on) {
                    wc += (costs.walk_one_over_soul_sand_cost - costs.walk_one_block_cost) / 2.0;
                } else if frost_walker {
                    // with frostwalker we can walk on water without the penalty, if we are sure we won't be using jesus
                } else if is_water_block(dest_on) {
                    wc += context.walk_on_water_one_penalty;
                }
                if is_soul_sand(src_down_block) {
                    wc += (costs.walk_one_over_soul_sand_cost - costs.walk_one_block_cost) / 2.0;
                } else if context.allow_walk_on_magma_blocks && is_magma(src_down_block) {
                    sneaking = true;
                    wc += (costs.sneak_one_block_cost - costs.walk_one_block_cost) / 2.0;
                }
            }
            let mut hardness1 =
                mh::get_mining_duration_ticks_state(context, dest_x, y, dest_z, pb1, false);
            if hardness1 >= COST_INF {
                return COST_INF;
            }
            let mut hardness2 =
                mh::get_mining_duration_ticks_state(context, dest_x, y + 1, dest_z, pb0, true); // only include falling on the upper block to break
            if hardness1 == 0.0 && hardness2 == 0.0 {
                if !water && !sneaking && context.can_sprint {
                    // If there's nothing in the way, and this isn't water, and we aren't sneak placing
                    // We can sprint =D
                    // Don't check for soul sand, since we can sprint on that too
                    wc *= costs.sprint_multiplier;
                }
                return wc;
            }
            if mh::is_climbable(src_down_block) {
                hardness1 *= 5.0;
                hardness2 *= 5.0;
            }
            wc + hardness1 + hardness2
        } else {
            //this is a bridge, so we need to place a block
            if mh::is_climbable(src_down_block) {
                return COST_INF;
            }
            if mh::is_replaceable(dest_x, y - 1, dest_z, dest_on, &context.bsi) {
                let through_water = mh::is_water(pb0) || mh::is_water(pb1);
                if mh::is_water(dest_on) && through_water {
                    // this happens when assume walk on water is true and this is a traverse in water, which isn't allowed
                    return COST_INF;
                }
                let place_cost = context.cost_of_placing_at(dest_x, y - 1, dest_z, dest_on);
                if place_cost >= COST_INF {
                    return COST_INF;
                }
                let hardness1 =
                    mh::get_mining_duration_ticks_state(context, dest_x, y, dest_z, pb1, false);
                if hardness1 >= COST_INF {
                    return COST_INF;
                }
                let hardness2 =
                    mh::get_mining_duration_ticks_state(context, dest_x, y + 1, dest_z, pb0, true); // only include falling on the upper block to break
                let mut wc = if through_water {
                    context.water_walk_speed
                } else {
                    costs.walk_one_block_cost
                };
                for dir in HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP {
                    let against_x = dest_x + dir.get_step_x();
                    let against_y = y - 1 + dir.get_step_y();
                    let against_z = dest_z + dir.get_step_z();
                    if against_x == x && against_z == z {
                        // this would be a backplace
                        continue;
                    }
                    if mh::can_place_against(&context.bsi, against_x, against_y, against_z) {
                        // found a side place option
                        return wc + place_cost + hardness1 + hardness2;
                    }
                }
                // now that we've checked all possible directions to side place, we actually need to backplace
                if is_soul_sand(src_down_block)
                    || src_down_block
                        .slab
                        .is_some_and(|slab| slab != SlabType::Double)
                {
                    return COST_INF; // can't sneak and backplace against soul sand or half slabs (regardless of whether it's top half or bottom half) =/
                }
                if !standing_on_a_block {
                    // standing on water / swimming
                    return COST_INF; // this is obviously impossible
                }
                let block_src = context.get_block(x, y, z);
                if (block_src.lily_pad || block_src.carpet) && src_down.fluid != Fluid::Empty {
                    return COST_INF; // we can stand on these but can't place against them
                }
                wc *= costs.sneak_one_block_cost / costs.walk_one_block_cost; //since we are sneak backplacing, we are sneaking lol
                return wc + place_cost + hardness1 + hardness2;
            }
            COST_INF
        }
    }
}

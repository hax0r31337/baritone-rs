// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementPillar.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: hasAgainst, getAgainst, updateState, prepared.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::host::{Fluid, Openable, SlabType};
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::{CalculationContext, Movement};

#[derive(Clone, Debug, PartialEq)]
pub struct MovementPillar;

impl MovementPillar {
    pub fn new(start: BetterBlockPos, end: BetterBlockPos) -> Movement {
        Movement::new(
            start,
            end,
            Box::new([start.above_n(2)]),
            Some(start),
            MovementKind::Pillar(MovementPillar),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        Self::cost(context, m.src.x, m.src.y, m.src.z)
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        FxHashSet::from_iter([m.src, m.dest])
    }

    pub fn cost(context: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
        let costs = &*context.costs;
        let from_state = context.get(x, y, z);
        let from = from_state;
        let ladder = mh::is_climbable(from);
        let from_down = context.get(x, y - 1, z);
        if !ladder {
            if mh::is_climbable(from_down) {
                return COST_INF; // can't pillar from a ladder or vine onto something that isn't also climbable
            }
            if from_down.slab == Some(SlabType::Bottom) {
                return COST_INF; // can't pillar up from a bottom slab onto a non ladder
            }
        }
        let to_break = context.get(x, y + 2, z);
        let to_break_block = to_break;
        if to_break_block.openable == Some(Openable::FenceGate) {
            // see issue #172
            return COST_INF;
        }
        let mut src_up = None;
        if mh::is_water(to_break) && mh::is_water(from_state) {
            // TODO should this also be allowed if toBreakBlock is air?
            let up = context.get(x, y + 1, z);
            src_up = Some(up);
            if mh::is_water(up) {
                return costs.ladder_up_one_cost; // allow ascending pillars of water, but only if we're already in one
            }
        }
        let mut place_cost = 0.0;
        if !ladder {
            // we need to place a block where we started to jump on it
            place_cost = context.cost_of_placing_at(x, y, z, from_state);
            if place_cost >= COST_INF {
                return COST_INF;
            }
            if from_down.air {
                place_cost += 0.1; // slightly (1/200th of a second) penalize pillaring on what's currently air
            }
        }
        if (mh::is_liquid(from_state)
            && !mh::can_place_against_state(&context.bsi, x, y - 1, z, from_down))
            || (mh::is_liquid(from_down) && context.assume_walk_on_water)
        {
            // otherwise, if we're standing in water, we cannot pillar
            // if we're standing on water and assumeWalkOnWater is true, we cannot pillar
            // if we're standing on water and assumeWalkOnWater is false, we must have ascended to here, or sneak backplaced, so it is possible to pillar again
            return COST_INF;
        }
        if (from.lily_pad || from.carpet) && from_down.fluid != Fluid::Empty {
            // to ascend here we'd have to break the block we are standing on
            return COST_INF;
        }
        let mut hardness =
            mh::get_mining_duration_ticks_state(context, x, y + 2, z, to_break, true);
        if hardness >= COST_INF {
            return COST_INF;
        }
        if hardness != 0.0 {
            if mh::is_climbable(to_break_block) {
                hardness = 0.0; // we won't actually need to break the ladder / vine because we're going to use it
            } else {
                let check = context.get(x, y + 3, z); // the block on top of the one we're going to break, could it fall on us?
                if check.falls {
                    // see MovementAscend's identical check for breaking a falling block above our head
                    let src_up = *src_up.get_or_insert_with(|| context.get(x, y + 1, z));
                    if !to_break_block.falls || !src_up.falls {
                        return COST_INF;
                    }
                }
                // this is commented because it may have had a purpose, but it's very unclear what it was. it's from the minebot era.
                //if (!MovementHelper.canWalkOn(context, chkPos, check) || MovementHelper.canWalkThrough(context, chkPos, check)) {//if the block above where we want to break is not a full block, don't do it
                // TODO why does canWalkThrough mean this action is COST_INF?
                // FallingBlock makes sense, and !canWalkOn deals with weird cases like if it were lava
                // but I don't understand why canWalkThrough makes it impossible
                //    return COST_INF;
                //}
            }
        }
        if ladder {
            costs.ladder_up_one_cost + hardness * 5.0
        } else {
            costs.jump_one_block_cost + place_cost + context.jump_penalty + hardness
        }
    }
}

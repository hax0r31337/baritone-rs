// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementPillar.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext, rotation_utils, vec_utils};
use crate::behavior::InventoryBehavior;
use crate::host::{Fluid, Openable, SlabType};
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movement_state::MovementTarget;
use crate::pathing::movement::{CalculationContext, Movement, MovementState};
use crate::utils::BlockStateInterface;

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
        let position_to_place = m.position_to_place.expect("positionToPlace");
        let ctx = &baritone.player_context;
        if ctx.player_feet().y < src.y {
            state.set_status(MovementStatus::Unreachable);
            return;
        }

        let from_down = BlockStateInterface::get(ctx, src);
        if mh::is_water(from_down) && mh::is_water_ctx(ctx, dest) {
            // stay centered while swimming up a water column
            state.set_target(MovementTarget::new(
                rotation_utils::calc_rotation_from_vec3d(
                    ctx.player_head(),
                    vec_utils::get_block_pos_center(dest),
                    ctx.player_rotations(),
                ),
                false,
            ));
            let dest_center = vec_utils::get_block_pos_center(dest);
            let position = ctx.player().position;
            if (position.x - dest_center.x).abs() > 0.2 || (position.z - dest_center.z).abs() > 0.2
            {
                state.set_input(Input::MoveForward, true);
            }
            if ctx.player_feet() == dest {
                state.set_status(MovementStatus::Success);
            }
            return;
        }
        let ladder = mh::is_climbable(from_down);

        let rotation = rotation_utils::calc_rotation_from_vec3d(
            ctx.player_head(),
            vec_utils::get_block_pos_center(position_to_place),
            ctx.player_rotations(),
        );
        if !ladder {
            state.set_target(MovementTarget::new(
                ctx.player_rotations().with_pitch(rotation.get_pitch()),
                true,
            ));
        }

        let mut block_is_there = mh::can_walk_on_ctx(ctx, src) || ladder;
        if ladder {
            if ctx.player_feet() == dest {
                state.set_status(MovementStatus::Success);
                return;
            }

            mh::move_towards(ctx, state, dest);
            state.set_input(Input::Jump, true);
            return;
        } else {
            // Get ready to place a throwaway block
            if !InventoryBehavior::select_throwaway_for_location(
                baritone, true, src.x, src.y, src.z,
            ) {
                state.set_status(MovementStatus::Unreachable);
                return;
            }
            let ctx = &baritone.player_context;

            state.set_input(Input::Sneak, true);
            // since (lower down) we only right click once player.isSneaking, and that happens the tick after we request to sneak

            let player = ctx.player();
            let diff_x = player.position.x - (dest.x as f64 + 0.5);
            let diff_z = player.position.z - (dest.z as f64 + 0.5);
            let dist = (diff_x * diff_x + diff_z * diff_z).sqrt();
            let delta_movement = player.delta_movement;
            let flat_motion =
                (delta_movement.x * delta_movement.x + delta_movement.z * delta_movement.z).sqrt();
            if dist > 0.17 {
                //why 0.17? because it seemed like a good number, that's why
                //[explanation added after baritone port lol] also because it needs to be less than 0.2 because of the 0.3 sneak limit
                //and 0.17 is reasonably less than 0.2

                // If it's been more than forty ticks of trying to jump and we aren't done yet, go forward, maybe we are stuck
                state.set_input(Input::MoveForward, true);

                // revise our target to both yaw and pitch if we're going to be moving forward
                state.set_target(MovementTarget::new(rotation, true));
            } else if flat_motion < 0.05 {
                // If our Y coordinate is above our goal, stop jumping
                state.set_input(Input::Jump, player.position.y < dest.y as f64);
            }

            if !block_is_there {
                let fr_state = BlockStateInterface::get(ctx, src);
                // TODO: Evaluate usage of getMaterial().isReplaceable()
                if !(fr_state.air || fr_state.replaceable) {
                    if let Some(rot) = rotation_utils::reachable_distance(
                        ctx,
                        baritone.look_behavior.get_aim_processor(),
                        src,
                        ctx.player_controller_ref().get_block_reach_distance(),
                    ) {
                        state.set_target(MovementTarget::new(rot, true));
                    }
                    state.set_input(Input::Jump, false); // breaking is like 5x slower when you're jumping
                    state.set_input(Input::ClickLeft, true);
                    block_is_there = false;
                } else if player.crouching
                    && (ctx.is_looking_at(src.below()) || ctx.is_looking_at(src))
                    && player.position.y > dest.y as f64 + 0.1
                {
                    state.set_input(Input::ClickRight, true);
                }
            }
        }

        // If we are at our goal and the block below us is placed
        if baritone.player_context.player_feet() == dest && block_is_there {
            state.set_status(MovementStatus::Success);
        }
    }

    pub(crate) fn prepared(
        m: &mut Movement,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) -> bool {
        let ctx = &baritone.player_context;
        if ctx.player_feet() == m.src || ctx.player_feet() == m.src.below() {
            let block = BlockStateInterface::get_block(ctx, m.src.below());
            if mh::is_climbable(block) {
                state.set_input(Input::Sneak, true);
            }
        }
        if mh::is_water_ctx(ctx, m.dest.above()) {
            return true;
        }
        m.prepared_default(baritone, state)
    }
}

// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementTraverse.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `reset` is `Movement::reset`.

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::helper::{log_debug, println};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext, Rotation, rotation_utils, vec_utils};
use crate::host::{Fluid, Openable, SlabType};
use crate::mc::Vec3;
use crate::pathing::movement::movement::{
    HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP, MovementKind,
};
use crate::pathing::movement::movement_helper::{self as mh, PlaceResult};
use crate::pathing::movement::movement_state::MovementTarget;
use crate::pathing::movement::movements::{is_magma, is_soul_sand, is_water_block};
use crate::pathing::movement::{CalculationContext, Movement, MovementState};
use crate::settings::settings;
use crate::utils::BlockStateInterface;

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

    pub(crate) fn update_state(
        m: &mut Movement,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) {
        m.update_state_default(baritone, state);
        let ctx = &baritone.player_context;
        let pb0 = BlockStateInterface::get(ctx, m.positions_to_break[0]);
        let pb1 = BlockStateInterface::get(ctx, m.positions_to_break[1]);
        let dest = m.dest;
        let src = m.src;
        if state.get_status() != MovementStatus::Running {
            // if the setting is enabled
            if !settings().walk_while_breaking {
                return;
            }
            // and if we're prepping (aka mining the block in front)
            if state.get_status() != MovementStatus::Prepping {
                return;
            }
            // and if it's fine to walk into the blocks in front
            if mh::avoid_walking_into(pb0) {
                return;
            }
            if mh::avoid_walking_into(pb1) {
                return;
            }
            // and we aren't already pressed up against the block
            let position = ctx.player().position;
            let dist = crate::java::max_f64(
                (position.x - (dest.x as f64 + 0.5)).abs(),
                (position.z - (dest.z as f64 + 0.5)).abs(),
            );
            if dist < 0.83 {
                return;
            }
            let Some(target_rotation) = state.get_target().get_rotation() else {
                // this can happen rarely when the server lags and doesn't send the falling sand entity until you've already walked through the block and are now mining the next one
                return;
            };

            // combine the yaw to the center of the destination, and the pitch to the specific block we're trying to break
            // it's safe to do this since the two blocks we break (in a traverse) are right on top of each other and so will have the same yaw
            let yaw_to_dest = rotation_utils::calc_rotation_from_vec3d(
                ctx.player_head(),
                vec_utils::calculate_block_center(ctx.world(), dest),
                ctx.player_rotations(),
            )
            .get_yaw();
            let mut pitch_to_break = target_rotation.get_pitch();
            if mh::is_block_normal_cube(pb0)
                || pb0.air && (mh::is_block_normal_cube(pb1) || pb1.air)
            {
                // in the meantime, before we're right up against the block, we can break efficiently at this angle
                pitch_to_break = 26.0;
            }

            state
                .set_target(MovementTarget::new(
                    Rotation::new(yaw_to_dest, pitch_to_break),
                    true,
                ))
                .set_input(Input::MoveForward, true)
                .set_input(Input::Sprint, true);
            return;
        }

        let fd = BlockStateInterface::get(ctx, src.below());
        let ladder = mh::is_climbable(fd);

        //sneak may have been set to true in the PREPPING state while mining an adjacent block, but we still want it to be true if the player is about to go on magma
        state.set_input(
            Input::Sneak,
            settings().allow_walk_on_magma_blocks
                && mh::stepping_on_blocks(ctx)
                    .into_iter()
                    .any(|block| is_magma(ctx.world().get_block_state(block))),
        );

        let pb0_door = pb0.openable == Some(Openable::Door);
        let pb1_door = pb1.openable == Some(Openable::Door);
        if pb0_door || pb1_door {
            let not_passable = pb0_door && !mh::is_door_passable(ctx, src, dest)
                || pb1_door && !mh::is_door_passable(ctx, dest, src);
            // Blocks.IRON_DOOR
            let can_open = !(pb0_door && !pb0.hand_openable || pb1_door && !pb1.hand_openable);

            if not_passable && can_open {
                state
                    .set_target(MovementTarget::new(
                        rotation_utils::calc_rotation_from_vec3d(
                            ctx.player_head(),
                            vec_utils::calculate_block_center(ctx.world(), m.positions_to_break[0]),
                            ctx.player_rotations(),
                        ),
                        true,
                    ))
                    .set_input(Input::ClickRight, true);
                return;
            }
        }

        if pb0.openable == Some(Openable::FenceGate) || pb1.openable == Some(Openable::FenceGate) {
            let blocked = if !mh::is_gate_passable(ctx, m.positions_to_break[0], src.above()) {
                Some(m.positions_to_break[0])
            } else if !mh::is_gate_passable(ctx, m.positions_to_break[1], src) {
                Some(m.positions_to_break[1])
            } else {
                None
            };
            if let Some(blocked) = blocked
                && let Some(rotation) = rotation_utils::reachable(
                    ctx,
                    baritone.look_behavior.get_aim_processor(),
                    blocked,
                )
            {
                state
                    .set_target(MovementTarget::new(rotation, true))
                    .set_input(Input::ClickRight, true);
                return;
            }
        }

        let position_to_place = m.position_to_place.expect("positionToPlace");
        let is_the_bridge_block_there = mh::can_walk_on_ctx(ctx, position_to_place)
            || ladder
            || mh::can_use_frost_walker_ctx(ctx, position_to_place);
        let feet = ctx.player_feet();
        if feet.y != dest.y && !ladder {
            log_debug("Wrong Y coordinate");
            if feet.y < dest.y {
                println("In movement traverse");
                state.set_input(Input::Jump, true);
                return;
            }
            return;
        }

        let MovementKind::Traverse(this) = &mut m.kind else {
            unreachable!()
        };
        if is_the_bridge_block_there {
            if feet == dest {
                state.set_status(MovementStatus::Success);
                return;
            }
            let direction = dest.subtract(src);
            if settings().overshoot_traverse
                && (feet == dest.offset(direction)
                    || feet == dest.offset(direction).offset(direction))
            {
                state.set_status(MovementStatus::Success);
                return;
            }
            let low = BlockStateInterface::get(ctx, src);
            let high = BlockStateInterface::get(ctx, src.above());
            if ctx.player().position.y > src.y as f64 + 0.1
                && !ctx.player().on_ground
                && (mh::is_climbable(low) || mh::is_climbable(high))
            {
                // hitting W could cause us to climb the ladder instead of going forward
                // wait until we're on the ground
                return;
            }
            let into = dest.subtract(src).offset(dest);
            let into_below = BlockStateInterface::get(ctx, into);
            let into_above = BlockStateInterface::get(ctx, into.above());
            if this.was_the_bridge_block_always_there
                && (!mh::is_liquid_ctx(ctx, feet) || settings().sprint_in_water)
                && (!mh::avoid_walking_into(into_below) || mh::is_water(into_below))
                && !mh::avoid_walking_into(into_above)
            {
                state.set_input(Input::Sprint, true);
            }

            let dest_down = BlockStateInterface::get(ctx, dest.below());
            if feet.y != dest.y && ladder && mh::is_climbable(dest_down) {
                state.set_input(Input::Jump, true);
            }
            mh::move_towards(ctx, state, m.positions_to_break[0]);
        } else {
            this.was_the_bridge_block_always_there = false;
            let standing_on = BlockStateInterface::get(ctx, feet.below());
            if is_soul_sand(standing_on) || standing_on.slab.is_some() {
                // see issue #118
                let position = ctx.player().position;
                let dist = crate::java::max_f64(
                    (dest.x as f64 + 0.5 - position.x).abs(),
                    (dest.z as f64 + 0.5 - position.z).abs(),
                );
                if dist < 0.85 {
                    // 0.5 + 0.3 + epsilon
                    mh::move_towards(ctx, state, dest);
                    state
                        .set_input(Input::MoveForward, false)
                        .set_input(Input::MoveBack, true);
                    return;
                }
            }
            let position = ctx.player().position;
            let dist1 = crate::java::max_f64(
                (position.x - (dest.x as f64 + 0.5)).abs(),
                (position.z - (dest.z as f64 + 0.5)).abs(),
            );
            let assume_safe_walk = settings().assume_safe_walk;
            let p = mh::attempt_to_place_a_block(
                state,
                baritone,
                dest.below(),
                false,
                !assume_safe_walk,
            );
            let ctx = &baritone.player_context;
            if (p == PlaceResult::ReadyToPlace || dist1 < 0.6) && !assume_safe_walk {
                state.set_input(Input::Sneak, true);
            }
            match p {
                PlaceResult::ReadyToPlace => {
                    if ctx.player().crouching || assume_safe_walk {
                        state.set_input(Input::ClickRight, true);
                    }
                    return;
                }
                PlaceResult::Attempting => {
                    let target = state
                        .get_target()
                        .rotation
                        .expect("NullPointerException: rotation");
                    if dist1 > 0.83 {
                        // might need to go forward a bit
                        let yaw = rotation_utils::calc_rotation_from_vec3d(
                            ctx.player_head(),
                            vec_utils::get_block_pos_center(dest),
                            ctx.player_rotations(),
                        )
                        .get_yaw();
                        if ((target.get_yaw() - yaw).abs() as f64) < 0.1 {
                            // but only if our attempted place is straight ahead
                            state.set_input(Input::MoveForward, true);
                            return;
                        }
                    } else if ctx.player_rotations().is_really_close_to(&target) {
                        // well i guess theres something in the way
                        state.set_input(Input::ClickLeft, true);
                        return;
                    }
                    return;
                }
                PlaceResult::NoOption => {}
            }
            if feet == dest {
                // If we are in the block that we are trying to get to, we are sneaking over air and we need to place a block beneath us against the one we just walked off of
                // Out.log(from + " " + to + " " + faceX + "," + faceY + "," + faceZ + " " + whereAmI);
                let face_x = (dest.x.wrapping_add(src.x) as f64 + 1.0) * 0.5;
                let face_y = (dest.y.wrapping_add(src.y) as f64 - 1.0) * 0.5;
                let face_z = (dest.z.wrapping_add(src.z) as f64 + 1.0) * 0.5;
                // faceX, faceY, faceZ is the middle of the face between from and to
                let goal_look = src.below(); // this is the block we were just standing on, and the one we want to place against

                let back_to_face = rotation_utils::calc_rotation_from_vec3d(
                    ctx.player_head(),
                    Vec3::new(face_x, face_y, face_z),
                    ctx.player_rotations(),
                );
                let pitch = back_to_face.get_pitch();
                let position = ctx.player().position;
                let dist2 =
                    crate::java::max_f64((position.x - face_x).abs(), (position.z - face_z).abs());
                if dist2 < 0.29 {
                    // see issue #208
                    let yaw = rotation_utils::calc_rotation_from_vec3d(
                        vec_utils::get_block_pos_center(dest),
                        ctx.player_head(),
                        ctx.player_rotations(),
                    )
                    .get_yaw();
                    state.set_target(MovementTarget::new(Rotation::new(yaw, pitch), true));
                    state.set_input(Input::MoveBack, true);
                } else {
                    state.set_target(MovementTarget::new(back_to_face, true));
                }
                if ctx.is_looking_at(goal_look) {
                    state.set_input(Input::ClickRight, true); // wait to right click until we are able to place
                    return;
                }
                // Out.log("Trying to look at " + goalLook + ", actually looking at" + Baritone.whatAreYouLookingAt());
                let target = state
                    .get_target()
                    .rotation
                    .expect("NullPointerException: rotation");
                if ctx.player_rotations().is_really_close_to(&target) {
                    state.set_input(Input::ClickLeft, true);
                }
                return;
            }
            mh::move_towards_with_slight_rotation(ctx, state, dest);
        }
    }

    pub(crate) fn safe_to_cancel(m: &Movement, baritone: &Baritone, state: &MovementState) -> bool {
        // if we're in the process of breaking blocks before walking forwards
        // or if this isn't a sneak place (the block is already there)
        // then it's safe to cancel this
        state.get_status() != MovementStatus::Running
            || mh::can_walk_on_ctx(&baritone.player_context, m.dest.below())
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
        m.prepared_default(baritone, state)
    }
}
